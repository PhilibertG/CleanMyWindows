//! Recherche de fichiers en double (LECTURE SEULE).
//!
//! 1. regroupement par taille exacte ;
//! 2. empreinte partielle (début + fin du fichier) ;
//! 3. empreinte complète xxh3-128 pour les candidats restants.
//! Les liens durs (même identifiant de fichier) et les fichiers cloud non
//! téléchargés sont ignorés : les lire déclencherait un téléchargement.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
use xxhash_rust::xxh3::{Xxh3, xxh3_128};

use crate::tree::{F_CLOUD, F_META, F_REPARSE, Tree};

const PARTIAL: usize = 64 * 1024;

#[derive(Default)]
pub struct DupCtx {
    pub phase: AtomicU8,
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DupProgress {
    pub phase: &'static str,
    pub done: u64,
    pub total: u64,
    pub bytes: u64,
}

pub fn progress(ctx: &DupCtx) -> DupProgress {
    DupProgress {
        phase: match ctx.phase.load(Ordering::Relaxed) {
            0 => "sizes",
            1 => "partial",
            _ => "full",
        },
        done: ctx.done.load(Ordering::Relaxed),
        total: ctx.total.load(Ordering::Relaxed),
        bytes: ctx.bytes.load(Ordering::Relaxed),
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DupFile {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub mtime: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    pub hash: String,
    pub size: u64,
    pub alloc: u64,
    pub wasted: u64,
    pub files: Vec<DupFile>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DupResult {
    pub groups: Vec<DupGroup>,
    pub total_groups: usize,
    pub wasted: u64,
    pub scanned_files: u64,
}

/// Dossiers dont le contenu est géré par une application : y supprimer une
/// « copie » casserait l'ensemble (sauvegarde iPhone, dépendances, jeux...).
const EXCLUDED: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "$recycle.bin",
    "system volume information",
    "mobilesync",
    "node_modules",
    "site-packages",
    "steamapps",
    "steamlibrary",
    "windowsapps",
    "epic games",
    "xboxgames",
    "__pycache__",
];

fn excluded(tree: &Tree, mut id: u32) -> bool {
    while id != 0 {
        id = tree.node(id).parent;
        if id != 0 {
            let name = tree.name(id);
            if name.starts_with('.') || EXCLUDED.iter().any(|e| name.eq_ignore_ascii_case(e)) {
                return true;
            }
        }
    }
    false
}

/// Identifiant unique du fichier (volume + index) pour détecter les liens durs.
fn file_id(f: &File) -> Option<(u32, u64)> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) };
    (ok != 0).then(|| (info.dwVolumeSerialNumber, ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64))
}

fn partial_hash(path: &str, size: u64) -> Option<(u128, (u32, u64))> {
    let mut f = File::open(path).ok()?;
    let id = file_id(&f)?;
    let mut buf = vec![0u8; PARTIAL * 2];
    let head = (size as usize).min(PARTIAL);
    f.read_exact(&mut buf[..head]).ok()?;
    let mut len = head;
    if size as usize > PARTIAL * 2 {
        f.seek(SeekFrom::End(-(PARTIAL as i64))).ok()?;
        f.read_exact(&mut buf[head..head + PARTIAL]).ok()?;
        len += PARTIAL;
    } else if size as usize > PARTIAL {
        let rest = size as usize - PARTIAL;
        f.read_exact(&mut buf[head..head + rest]).ok()?;
        len += rest;
    }
    Some((xxh3_128(&buf[..len]), id))
}

fn full_hash(path: &str, ctx: &DupCtx) -> Option<u128> {
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        ctx.bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(h.digest128())
}

pub fn find(tree: &Tree, min_size: u64, ctx: &DupCtx) -> Result<DupResult, String> {
    // 1. Regroupement par taille.
    ctx.phase.store(0, Ordering::Relaxed);
    let mut by_size: HashMap<u64, Vec<u32>> = HashMap::new();
    for id in tree.live_ids() {
        let n = tree.node(id);
        if n.is_dir() || n.size < min_size.max(1) || n.flags & (F_CLOUD | F_META | F_REPARSE) != 0 {
            continue;
        }
        by_size.entry(n.size).or_default().push(id);
    }
    let candidates: Vec<(u64, u32)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|(s, v)| v.into_iter().map(move |id| (s, id)))
        .filter(|(_, id)| !excluded(tree, *id))
        .collect();
    let scanned = candidates.len() as u64;

    // 2. Empreinte partielle (lecture parallèle limitée pour ménager le disque).
    ctx.phase.store(1, Ordering::Relaxed);
    ctx.total.store(candidates.len() as u64, Ordering::Relaxed);
    ctx.done.store(0, Ordering::Relaxed);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().map_err(|e| e.to_string())?;
    let partial: Vec<(u64, u32, u128, (u32, u64))> = pool.install(|| {
        candidates
            .par_iter()
            .filter_map(|(size, id)| {
                if ctx.cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let r = partial_hash(&tree.path(*id), *size);
                ctx.done.fetch_add(1, Ordering::Relaxed);
                r.map(|(h, fid)| (*size, *id, h, fid))
            })
            .collect()
    });
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err("Recherche annulée".into());
    }
    let mut groups: HashMap<(u64, u128), Vec<(u32, (u32, u64))>> = HashMap::new();
    for (size, id, h, fid) in partial {
        let g = groups.entry((size, h)).or_default();
        // Un lien dur vers un fichier déjà présent n'est pas un doublon.
        if !g.iter().any(|(_, f)| *f == fid) {
            g.push((id, fid));
        }
    }
    let second: Vec<(u64, u32)> = groups
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|((s, _), v)| v.into_iter().map(move |(id, _)| (s, id)))
        .collect();

    // 3. Empreinte complète.
    ctx.phase.store(2, Ordering::Relaxed);
    ctx.total.store(second.iter().map(|(s, _)| *s).sum(), Ordering::Relaxed);
    ctx.bytes.store(0, Ordering::Relaxed);
    let full: Vec<(u64, u32, u128)> = pool.install(|| {
        second
            .par_iter()
            .filter_map(|(size, id)| full_hash(&tree.path(*id), ctx).map(|h| (*size, *id, h)))
            .collect()
    });
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err("Recherche annulée".into());
    }
    let mut finals: HashMap<(u64, u128), Vec<u32>> = HashMap::new();
    for (size, id, h) in full {
        finals.entry((size, h)).or_default().push(id);
    }
    let mut out: Vec<DupGroup> = finals
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((size, h), mut ids)| {
            // Le plus ancien d'abord : c'est généralement l'original à conserver.
            ids.sort_by_key(|id| tree.node(*id).mtime);
            let alloc = tree.node(ids[0]).alloc;
            DupGroup {
                hash: format!("{h:032x}"),
                size,
                alloc,
                wasted: alloc * (ids.len() as u64 - 1),
                files: ids
                    .iter()
                    .map(|id| DupFile { id: *id, name: tree.name(*id).to_string(), path: tree.path(*id), mtime: tree.node(*id).mtime })
                    .collect(),
            }
        })
        .collect();
    out.sort_unstable_by(|a, b| b.wasted.cmp(&a.wasted));
    let wasted = out.iter().map(|g| g.wasted).sum();
    let total_groups = out.len();
    out.truncate(1000);
    Ok(DupResult { groups: out, total_groups, wasted, scanned_files: scanned })
}
