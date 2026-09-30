//! Lecture directe de la Master File Table NTFS (mode « turbo », administrateur requis).
//!
//! Au lieu d'ouvrir chaque dossier, on lit séquentiellement la MFT du volume
//! (quelques centaines de Mo) et on décode chaque enregistrement FILE. C'est la
//! technique utilisée par WizTree : un disque complet s'analyse en quelques
//! secondes. Accès en LECTURE SEULE.

use std::ptr::null_mut;
use std::sync::atomic::Ordering;

use rayon::prelude::*;
use windows_sys::Win32::Foundation::{GENERIC_READ, GetLastError};
use windows_sys::Win32::Storage::FileSystem::ReadFile;
use windows_sys::Win32::System::IO::{DeviceIoControl, OVERLAPPED};
use windows_sys::Win32::System::Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER};

use super::{PHASE_READING_MFT, ScanCtx};
use crate::tree::{F_DIR, F_HIDDEN, F_META, F_REPARSE, F_SYSTEM, TreeBuilder};
use crate::win;

const ROOT_RECORD: u32 = 5;
const NO_PARENT: u32 = u32::MAX;
const CHUNK: usize = 16 * 1024 * 1024;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_INDEX_ALLOCATION: u32 = 0xA0;
const ATTR_END: u32 = 0xFFFF_FFFF;

/// Informations extraites d'un enregistrement (ou d'un enregistrement d'extension).
#[derive(Default)]
struct Parsed {
    record: u32,
    /// Enregistrement de base si c'est une extension.
    base: u32,
    in_use: bool,
    is_dir: bool,
    parent: u32,
    name: Option<(u8, String)>,
    size: u64,
    alloc: u64,
    mtime: i64,
    attrs: u32,
    has_si: bool,
}

/// Enregistrement consolidé, indexé par numéro d'enregistrement MFT.
#[derive(Clone, Default)]
pub(super) struct Rec {
    pub in_use: bool,
    pub is_dir: bool,
    pub parent: u32,
    pub name_ns: u8,
    pub name: Option<Box<str>>,
    pub size: u64,
    pub alloc: u64,
    pub mtime: i64,
    pub attrs: u32,
}

#[inline]
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
#[inline]
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Applique le tableau de correction (update sequence array) d'un enregistrement.
fn apply_fixup(rec: &mut [u8]) -> bool {
    if rec.len() < 48 || &rec[0..4] != b"FILE" {
        return false;
    }
    let usa_off = u16_at(rec, 4) as usize;
    let usa_count = u16_at(rec, 6) as usize;
    if usa_count == 0 || usa_off + usa_count * 2 > rec.len() {
        return false;
    }
    let usn = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_count {
        let end = i * 512 - 2;
        if end + 2 > rec.len() {
            break;
        }
        if rec[end..end + 2] != usn {
            return false;
        }
        rec[end] = rec[usa_off + i * 2];
        rec[end + 1] = rec[usa_off + i * 2 + 1];
    }
    true
}

/// Décode la liste des extents (runlist) d'un attribut non résident.
fn decode_runs(b: &[u8]) -> Vec<(i64, u64)> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut lcn: i64 = 0;
    while i < b.len() && b[i] != 0 {
        let h = b[i];
        let len_size = (h & 0x0F) as usize;
        let off_size = (h >> 4) as usize;
        i += 1;
        if i + len_size + off_size > b.len() || len_size == 0 || len_size > 8 || off_size > 8 {
            break;
        }
        let mut len: u64 = 0;
        for k in 0..len_size {
            len |= (b[i + k] as u64) << (8 * k);
        }
        i += len_size;
        if off_size == 0 {
            // Extent creux (sparse).
            runs.push((-1, len));
            continue;
        }
        let mut off: i64 = 0;
        for k in 0..off_size {
            off |= (b[i + k] as i64) << (8 * k);
        }
        // Extension de signe.
        let shift = 64 - 8 * off_size as u32;
        off = (off << shift) >> shift;
        i += off_size;
        lcn += off;
        runs.push((lcn, len));
    }
    runs
}

fn parse_record(rec: &[u8], record: u32, bpc: u64) -> Option<Parsed> {
    let flags = u16_at(rec, 22);
    let mut p = Parsed {
        record,
        in_use: flags & 1 != 0,
        is_dir: flags & 2 != 0,
        parent: NO_PARENT,
        base: (u64_at(rec, 32) & 0x0000_FFFF_FFFF_FFFF) as u32,
        ..Default::default()
    };
    if !p.in_use {
        return None;
    }
    let used = (u32_at(rec, 24) as usize).min(rec.len());
    let mut off = u16_at(rec, 20) as usize;
    while off + 16 <= used {
        let ty = u32_at(rec, off);
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, off + 4) as usize;
        if len < 16 || off + len > used {
            break;
        }
        let a = &rec[off..off + len];
        let non_resident = a[8] != 0;
        let name_len = a[9];
        match ty {
            ATTR_STANDARD_INFORMATION if !non_resident && a.len() >= 24 => {
                let v = u16_at(a, 20) as usize;
                if v + 36 <= a.len() {
                    p.mtime = win::filetime_to_unix(u64_at(a, v + 8) as i64);
                    p.attrs = u32_at(a, v + 32);
                    p.has_si = true;
                }
            }
            ATTR_FILE_NAME if !non_resident && a.len() >= 24 => {
                let v = u16_at(a, 20) as usize;
                if v + 66 <= a.len() {
                    let parent = (u64_at(a, v) & 0x0000_FFFF_FFFF_FFFF) as u32;
                    let nlen = a[v + 64] as usize;
                    let ns = a[v + 65];
                    // Espace de noms 2 = nom court DOS 8.3 : ignoré s'il existe un nom long.
                    let better = match &p.name {
                        None => true,
                        Some((cur, _)) => *cur == 2 && ns != 2,
                    };
                    if better && v + 66 + nlen * 2 <= a.len() {
                        let units: Vec<u16> = (0..nlen).map(|k| u16_at(a, v + 66 + k * 2)).collect();
                        p.name = Some((ns, String::from_utf16_lossy(&units)));
                        p.parent = parent;
                    }
                }
            }
            ATTR_DATA | ATTR_INDEX_ALLOCATION => {
                if non_resident && a.len() >= 64 {
                    // Espace réel = clusters effectivement alloués de CET extent
                    // (les extents creux/compressés ne comptent pas). Chaque extent
                    // couvre des VCN distincts : pas de double comptage.
                    let mp = u16_at(a, 32) as usize;
                    if mp < a.len() {
                        let clusters: u64 = decode_runs(&a[mp..]).iter().filter(|r| r.0 >= 0).map(|r| r.1).sum();
                        p.alloc += clusters * bpc;
                    }
                    // Seul le premier extent (VCN 0) porte la taille logique.
                    if u64_at(a, 16) == 0 && ty == ATTR_DATA && name_len == 0 {
                        p.size += u64_at(a, 48);
                    }
                } else if ty == ATTR_DATA && name_len == 0 && a.len() >= 20 {
                    // Données résidentes : stockées dans la MFT elle-même.
                    p.size += u32_at(a, 16) as u64;
                }
            }
            _ => {}
        }
        off += len;
    }
    Some(p)
}

/// Lecture positionnée sur le volume brut (offset et taille alignés sur le secteur).
fn read_at(vol: &win::Handle, offset: u64, buf: &mut [u8]) -> Result<(), String> {
    let mut done = 0usize;
    while done < buf.len() {
        let pos = offset + done as u64;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.Anonymous.Anonymous.Offset = pos as u32;
        ov.Anonymous.Anonymous.OffsetHigh = (pos >> 32) as u32;
        let want = (buf.len() - done).min(u32::MAX as usize & !0xFFFF) as u32;
        let mut read = 0u32;
        let ok = unsafe { ReadFile(vol.0, buf[done..].as_mut_ptr(), want, &mut read, &mut ov) };
        if ok == 0 {
            return Err(format!("erreur Win32 {} à l'offset {pos}", unsafe { GetLastError() }));
        }
        if read == 0 {
            return Err(format!("fin de volume inattendue à l'offset {pos}"));
        }
        done += read as usize;
    }
    Ok(())
}

pub(super) struct VolumeData {
    pub bytes_per_cluster: u64,
    pub record_size: usize,
    pub mft_start_lcn: i64,
    pub mft_valid: u64,
}

pub(super) fn volume_data(letter: char) -> Result<VolumeData, String> {
    let h = win::open_volume(letter, GENERIC_READ)
        .ok_or("Impossible d'ouvrir le volume (droits administrateur requis)")?;
    let mut data: NTFS_VOLUME_DATA_BUFFER = unsafe { std::mem::zeroed() };
    let mut ret = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            h.0,
            FSCTL_GET_NTFS_VOLUME_DATA,
            std::ptr::null(),
            0,
            &mut data as *mut _ as *mut _,
            size_of::<NTFS_VOLUME_DATA_BUFFER>() as u32,
            &mut ret,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err("FSCTL_GET_NTFS_VOLUME_DATA a échoué (droits administrateur requis)".into());
    }
    Ok(VolumeData {
        bytes_per_cluster: data.BytesPerCluster as u64,
        record_size: data.BytesPerFileRecordSegment as usize,
        mft_start_lcn: data.MftStartLcn,
        mft_valid: data.MftValidDataLength as u64,
    })
}

pub fn scan(root: &str, ctx: &ScanCtx) -> Result<TreeBuilder, String> {
    let letter = win::drive_letter(root).ok_or("Lecteur invalide")?;
    let vd = volume_data(letter)?;
    if vd.record_size < 1024 || vd.bytes_per_cluster == 0 {
        return Err("Géométrie NTFS inattendue".into());
    }
    let vol = win::open_volume(letter, GENERIC_READ).ok_or("Lecture du volume refusée")?;

    // 1. Enregistrement 0 ($MFT) -> extents de la MFT.
    let rs = vd.record_size;
    let mut rec0 = vec![0u8; rs.max(4096)];
    read_at(&vol, vd.mft_start_lcn as u64 * vd.bytes_per_cluster, &mut rec0)
        .map_err(|e| format!("Lecture de $MFT impossible : {e}"))?;
    let rec0 = &mut rec0[..rs];
    if !apply_fixup(rec0) {
        return Err("Enregistrement $MFT invalide".into());
    }
    let runs = mft_runs(rec0).ok_or("Extents de $MFT introuvables")?;

    // 2. Lecture séquentielle de la MFT par gros blocs + décodage parallèle.
    let total_bytes = vd.mft_valid;
    ctx.phase.store(PHASE_READING_MFT, Ordering::Relaxed);
    ctx.total.store(total_bytes, Ordering::Relaxed);
    *ctx.current.lock() = format!("{letter}:\\$MFT");
    let record_count = (total_bytes / rs as u64) as usize;
    let mut recs: Vec<Rec> = vec![Rec::default(); record_count];
    let mut extensions: Vec<Parsed> = Vec::new();

    let mut record_index: u64 = 0;
    let mut buf = vec![0u8; CHUNK];
    'outer: for (lcn, clusters) in runs {
        if lcn < 0 {
            record_index += clusters * vd.bytes_per_cluster / rs as u64;
            continue;
        }
        let mut pos = lcn as u64 * vd.bytes_per_cluster;
        let mut remaining = clusters * vd.bytes_per_cluster;
        while remaining > 0 {
            if ctx.cancelled() {
                return Err("Analyse annulée".into());
            }
            let left_records = (record_count as u64).saturating_sub(record_index);
            if left_records == 0 {
                break 'outer;
            }
            let n = remaining.min(CHUNK as u64).min(left_records * rs as u64) as usize;
            // Lecture alignée sur le cluster (multiple de la taille de secteur).
            let aligned = n.div_ceil(vd.bytes_per_cluster as usize) * vd.bytes_per_cluster as usize;
            let aligned = aligned.min(CHUNK);
            read_at(&vol, pos, &mut buf[..aligned]).map_err(|e| format!("Lecture MFT : {e}"))?;

            let start = record_index as u32;
            let parsed: Vec<Parsed> = buf[..n]
                .par_chunks_mut(rs)
                .enumerate()
                .filter_map(|(i, rec)| {
                    if !apply_fixup(rec) {
                        return None;
                    }
                    parse_record(rec, start + i as u32, vd.bytes_per_cluster)
                })
                .collect();
            for p in parsed {
                if p.base != 0 && p.base != p.record {
                    extensions.push(p);
                    continue;
                }
                let r = &mut recs[p.record as usize];
                r.in_use = true;
                r.is_dir = p.is_dir;
                r.size += p.size;
                r.alloc += p.alloc;
                r.mtime = p.mtime;
                r.attrs = p.attrs;
                if let Some((ns, name)) = p.name {
                    r.name_ns = ns;
                    r.name = Some(name.into_boxed_str());
                    r.parent = p.parent;
                }
            }
            let recs_read = (n / rs) as u64;
            record_index += recs_read;
            pos += n as u64;
            remaining -= n as u64;
            ctx.done.fetch_add(n as u64, Ordering::Relaxed);
            ctx.files.fetch_add(recs_read, Ordering::Relaxed);
        }
    }

    // 3. Fusion des enregistrements d'extension dans leur enregistrement de base.
    for p in extensions {
        let Some(r) = recs.get_mut(p.base as usize) else { continue };
        r.size += p.size;
        r.alloc += p.alloc;
        if let Some((ns, name)) = p.name {
            if r.name.is_none() || (r.name_ns == 2 && ns != 2) {
                r.name_ns = ns;
                r.name = Some(name.into_boxed_str());
                r.parent = p.parent;
            }
        }
    }

    build(root, recs, ctx)
}

fn mft_runs(rec: &[u8]) -> Option<Vec<(i64, u64)>> {
    let used = (u32_at(rec, 24) as usize).min(rec.len());
    let mut off = u16_at(rec, 20) as usize;
    while off + 16 <= used {
        let ty = u32_at(rec, off);
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, off + 4) as usize;
        if len < 16 || off + len > used {
            break;
        }
        let a = &rec[off..off + len];
        if ty == ATTR_DATA && a[8] != 0 && a[9] == 0 {
            let mp = u16_at(a, 32) as usize;
            return Some(decode_runs(&a[mp..]));
        }
        off += len;
    }
    None
}

pub(super) fn build(root: &str, recs: Vec<Rec>, ctx: &ScanCtx) -> Result<TreeBuilder, String> {
    ctx.phase.store(super::PHASE_BUILDING, Ordering::Relaxed);
    let n = recs.len();
    if n <= ROOT_RECORD as usize || !recs[ROOT_RECORD as usize].in_use {
        return Err("Racine NTFS introuvable".into());
    }
    // Liste d'enfants par tri comptage.
    let mut counts = vec![0u32; n + 1];
    for (i, r) in recs.iter().enumerate() {
        if i as u32 != ROOT_RECORD && r.in_use && r.name.is_some() && (r.parent as usize) < n {
            counts[r.parent as usize + 1] += 1;
        }
    }
    for i in 1..=n {
        counts[i] += counts[i - 1];
    }
    let mut starts = counts.clone();
    let mut kids = vec![0u32; counts[n] as usize];
    for (i, r) in recs.iter().enumerate() {
        if i as u32 != ROOT_RECORD && r.in_use && r.name.is_some() && (r.parent as usize) < n {
            let s = &mut starts[r.parent as usize];
            kids[*s as usize] = i as u32;
            *s += 1;
        }
    }

    let mut files = 0u64;
    let mut dirs = 0u64;
    let mut b = TreeBuilder::with_capacity(n, n * 24);
    let root_rec = &recs[ROOT_RECORD as usize];
    let root_id = b.push(0, root, 0, root_rec.alloc, root_rec.mtime, F_DIR);
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((ROOT_RECORD, root_id, true));
    while let Some((rec, id, is_root)) = queue.pop_front() {
        let range = counts[rec as usize] as usize..counts[rec as usize + 1] as usize;
        let first = b.len() as u32;
        let mut count = 0u32;
        for &k in &kids[range] {
            let r = &recs[k as usize];
            // Évite les cycles éventuels (MFT incohérente).
            if !recs[r.parent as usize].is_dir {
                continue;
            }
            let name = r.name.as_deref().unwrap_or("?");
            let mut flags = 0u16;
            if r.is_dir {
                flags |= F_DIR;
            }
            if r.attrs & 0x2 != 0 {
                flags |= F_HIDDEN;
            }
            if r.attrs & 0x4 != 0 {
                flags |= F_SYSTEM;
            }
            if r.attrs & 0x400 != 0 {
                flags |= F_REPARSE;
            }
            if is_root && name.starts_with('$') && k < 32 {
                flags |= F_META;
            }
            let (size, alloc) = if r.is_dir { (0, r.alloc) } else { (r.size, r.alloc) };
            let cid = b.push(id, name, size, alloc, r.mtime, flags);
            count += 1;
            if r.is_dir {
                dirs += 1;
                if k != rec {
                    queue.push_back((k, cid, false));
                }
            } else {
                files += 1;
            }
        }
        b.set_children(id, first, count);
    }
    ctx.files.store(files, Ordering::Relaxed);
    ctx.dirs.store(dirs, Ordering::Relaxed);
    Ok(b)
}
