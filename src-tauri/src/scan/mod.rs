//! Orchestration d'une analyse : choix de la méthode (MFT ou parcours parallèle).

pub mod layout;
pub mod mft;
pub mod walk;

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;

use crate::tree::{ScanInfo, Tree};
use crate::{volumes, win};

pub const PHASE_SCANNING: u8 = 0;
pub const PHASE_READING_MFT: u8 = 1;
pub const PHASE_BUILDING: u8 = 2;

#[derive(Default)]
pub struct ScanCtx {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub denied: AtomicU64,
    /// Progression connue (lecture MFT) : octets lus / total.
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub phase: AtomicU8,
    pub cancel: AtomicBool,
    pub current: Mutex<String>,
}

impl ScanCtx {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    /// MFT si possible, sinon parcours parallèle.
    Auto,
    /// Parcours parallèle uniquement.
    Walk,
    /// MFT uniquement (erreur si impossible).
    Mft,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub denied: u64,
    pub current: String,
    pub elapsed_ms: u64,
    /// Pourcentage estimé (0-100), si calculable.
    pub percent: Option<f64>,
}

pub fn snapshot(ctx: &ScanCtx, started: Instant, expected_bytes: u64) -> Progress {
    let phase = ctx.phase.load(Ordering::Relaxed);
    let bytes = ctx.bytes.load(Ordering::Relaxed);
    let percent = match phase {
        PHASE_READING_MFT => {
            let total = ctx.total.load(Ordering::Relaxed);
            (total > 0).then(|| ctx.done.load(Ordering::Relaxed) as f64 * 100.0 / total as f64)
        }
        PHASE_BUILDING => Some(100.0),
        _ => (expected_bytes > 0).then(|| (bytes as f64 * 100.0 / expected_bytes as f64).min(99.0)),
    };
    Progress {
        phase: match phase {
            PHASE_READING_MFT => "mft",
            PHASE_BUILDING => "building",
            _ => "scanning",
        },
        files: ctx.files.load(Ordering::Relaxed),
        dirs: ctx.dirs.load(Ordering::Relaxed),
        bytes,
        denied: ctx.denied.load(Ordering::Relaxed),
        current: ctx.current.lock().clone(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        percent,
    }
}

/// Normalise le chemin cible (`C:` -> `C:\`, retire le `\` final des dossiers).
pub fn normalize_root(path: &str) -> String {
    let mut p = win::display_path(path.trim()).replace('/', "\\");
    if p.len() == 2 && p.ends_with(':') {
        p.push('\\');
    }
    while p.len() > 3 && p.ends_with('\\') {
        p.pop();
    }
    if p.len() == 3 && p.as_bytes()[1] == b':' {
        p = p.to_uppercase();
    }
    p
}

/// Nombre de threads adapté au support : les disques durs souffrent des accès concurrents.
pub fn thread_count(root: &str) -> usize {
    if let Some(n) = std::env::var("CMW_THREADS").ok().and_then(|v| v.parse().ok()) {
        return n;
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    match win::drive_letter(root).and_then(win::has_seek_penalty) {
        Some(true) => 4,
        // Au-delà, le coût est dominé par le noyau (antivirus, verrous NTFS) : plus de threads n'aide pas.
        _ => cpus.clamp(4, 16),
    }
}

/// Espace attendu (pour estimer la progression d'un parcours).
pub fn expected_bytes(root: &str) -> u64 {
    if is_volume_root(root) {
        volumes::space(root).map(|(total, free)| total.saturating_sub(free)).unwrap_or(0)
    } else {
        0
    }
}

pub fn is_volume_root(root: &str) -> bool {
    root.len() == 3 && root.ends_with(":\\")
}

fn reset(ctx: &ScanCtx) {
    ctx.phase.store(PHASE_SCANNING, Ordering::Relaxed);
    for c in [&ctx.files, &ctx.dirs, &ctx.bytes, &ctx.done, &ctx.total] {
        c.store(0, Ordering::Relaxed);
    }
}

pub fn run(root: &str, mode: ScanMode, ctx: &ScanCtx) -> Result<Tree, String> {
    let started = Instant::now();
    let root = normalize_root(root);
    let elevated = win::is_elevated();
    if elevated {
        // Permet de lister les dossiers protégés (System Volume Information, profils...).
        win::enable_privilege("SeBackupPrivilege");
    }
    let volume_root = is_volume_root(&root);
    let (volume_total, volume_free) = win::drive_letter(&root)
        .and_then(|l| volumes::space(&format!("{l}:\\")))
        .unwrap_or((0, 0));

    let try_mft = match mode {
        ScanMode::Walk => false,
        ScanMode::Mft => true,
        ScanMode::Auto => volume_root && elevated && volumes::filesystem(&root).as_deref() == Some("NTFS"),
    };

    let mut info = ScanInfo {
        root_path: root.clone(),
        method: String::new(),
        elapsed_ms: 0,
        volume_total,
        volume_free,
        is_volume_root: volume_root,
        denied: 0,
        elevated,
        threads: 0,
    };

    let mut tree = None;
    if try_mft {
        // 1. Lecture brute de la MFT, 2. énumération via le système de fichiers.
        let only = std::env::var("CMW_TURBO").unwrap_or_default();
        let mut errors = Vec::new();
        if only != "layout" {
            match mft::scan(&root, ctx) {
                Ok(t) => {
                    info.method = "mft".into();
                    tree = Some(t);
                }
                Err(e) => errors.push(format!("MFT brute : {e}")),
            }
        }
        if tree.is_none() && only != "raw" && !ctx.cancelled() {
            reset(ctx);
            match layout::scan(&root, ctx) {
                Ok(t) => {
                    info.method = "layout".into();
                    tree = Some(t);
                }
                Err(e) => errors.push(format!("Table des fichiers : {e}")),
            }
        }
        if tree.is_none() {
            if mode == ScanMode::Mft {
                return Err(errors.join(" ; "));
            }
            // Repli silencieux sur le parcours parallèle.
            reset(ctx);
        }
    }
    if ctx.cancelled() {
        return Err("Analyse annulée".into());
    }
    let builder = match tree {
        Some(b) => b,
        None => {
            let threads = thread_count(&root);
            info.threads = threads;
            info.method = "parallel".into();
            walk::scan(&root, ctx, threads)?
        }
    };
    if ctx.cancelled() {
        return Err("Analyse annulée".into());
    }
    ctx.phase.store(PHASE_BUILDING, Ordering::Relaxed);
    info.denied = ctx.denied.load(Ordering::Relaxed);
    info.elapsed_ms = started.elapsed().as_millis() as u64;
    let t2 = Instant::now();
    let mut tree = builder.finish(info);
    if std::env::var_os("CMW_TIMING").is_some() { eprintln!("finish: {:?}", t2.elapsed()); }
    tree.info.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(tree)
}
