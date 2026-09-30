//! Actions sur le disque : nettoyage, corbeille, ouverture dans l'Explorateur.
//!
//! Toute suppression passe par [`plan`] qui refuse les emplacements critiques
//! (racine du disque, dossiers système, profils utilisateur...).

use std::path::Path;
use std::process::Command;
use std::ptr::{null, null_mut};

use rayon::prelude::*;
use serde::Serialize;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHFILEOPSTRUCTW,
    SHFileOperationW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::tree::Tree;
use crate::win;

#[derive(Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanMode {
    /// Envoi à la Corbeille (récupérable).
    Trash,
    /// Suppression définitive.
    Permanent,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub path: String,
    pub error: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub freed: u64,
    pub removed: usize,
    pub failed_count: usize,
    pub failed: Vec<Failure>,
}

/// Élément validé, prêt à être supprimé.
pub struct Target {
    pub id: u32,
    pub path: String,
    pub is_dir: bool,
    pub alloc: u64,
    /// Pour une suppression définitive d'un dossier : descendants (post-ordre).
    pub descendants: Vec<(u32, String, bool)>,
}

/// Dossiers qu'on ne supprime jamais en entier (relatifs à la racine d'un volume).
const PROTECTED_EXACT: &[&str] = &[
    "windows",
    "windows\\system32",
    "windows\\syswow64",
    "windows\\winsxs",
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "boot",
    "recovery",
];

fn refuse_reason(tree: &Tree, id: u32) -> Option<&'static str> {
    if id == 0 {
        return Some("la racine de l'analyse ne peut pas être supprimée");
    }
    let n = tree.node(id);
    if n.is_deleted() {
        return Some("déjà supprimé");
    }
    if !tree.info.is_volume_root {
        return None;
    }
    let chain = tree.ancestry(id);
    // Dossiers à la racine d'un volume (Windows, Users, Program Files...).
    if chain.len() == 2 && n.is_dir() {
        return Some("dossier à la racine du disque");
    }
    let rel: Vec<String> = chain[1..].iter().map(|c| tree.name(*c).to_ascii_lowercase()).collect();
    let joined = rel.join("\\");
    if PROTECTED_EXACT.contains(&joined.as_str()) {
        return Some("dossier système protégé");
    }
    // Profils utilisateur et leurs dossiers principaux.
    if rel.first().map(String::as_str) == Some("users") && rel.len() <= 2 {
        return Some("profil utilisateur");
    }
    if rel.first().map(String::as_str) == Some("users")
        && rel.len() <= 4
        && rel.get(2).map(String::as_str) == Some("appdata")
    {
        return Some("dossier AppData principal");
    }
    // Dans Windows, seuls quelques emplacements de nettoyage connus sont autorisés.
    if rel.first().map(String::as_str) == Some("windows") {
        let allowed = [
            "windows\\temp\\",
            "windows\\softwaredistribution\\download\\",
            "windows\\minidump\\",
            "windows\\livekernelreports\\",
            "windows\\serviceprofiles\\networkservice\\appdata\\local\\microsoft\\windows\\deliveryoptimization\\cache\\",
        ];
        let with_sep = format!("{joined}\\");
        let ok = joined == "windows\\memory.dmp" || allowed.iter().any(|a| with_sep.starts_with(a) && with_sep.len() > a.len());
        if !ok {
            return Some("fichier système Windows");
        }
    }
    None
}

/// Valide les éléments demandés et prépare leur suppression.
pub fn plan(tree: &Tree, ids: &[u32], mode: CleanMode) -> (Vec<Target>, Vec<Failure>) {
    let mut targets = Vec::new();
    let mut refused = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let requested: std::collections::HashSet<u32> = ids.iter().copied().collect();
    for &id in ids {
        if (id as usize) >= tree.len() || !seen.insert(id) {
            continue;
        }
        if let Some(reason) = refuse_reason(tree, id) {
            refused.push(Failure { path: tree.path(id), error: format!("Refusé : {reason}") });
            continue;
        }
        // Ignore un élément dont un ancêtre est déjà demandé.
        let mut cur = tree.node(id).parent;
        let mut covered = false;
        while cur != 0 {
            if requested.contains(&cur) && refuse_reason(tree, cur).is_none() {
                covered = true;
                break;
            }
            cur = tree.node(cur).parent;
        }
        if covered {
            continue;
        }
        let n = tree.node(id);
        let mut descendants = Vec::new();
        if mode == CleanMode::Permanent && n.is_dir() {
            collect_post_order(tree, id, &mut descendants);
        }
        targets.push(Target { id, path: tree.path(id), is_dir: n.is_dir(), alloc: n.alloc, descendants });
    }
    (targets, refused)
}

fn collect_post_order(tree: &Tree, id: u32, out: &mut Vec<(u32, String, bool)>) {
    // Pile explicite : (id, enfants déjà empilés ?)
    let mut stack = vec![(id, false)];
    while let Some((cur, expanded)) = stack.pop() {
        if expanded || !tree.node(cur).is_dir() {
            if cur != id {
                out.push((cur, tree.path(cur), tree.node(cur).is_dir()));
            }
            continue;
        }
        stack.push((cur, true));
        for c in tree.children(cur) {
            stack.push((c, false));
        }
    }
}

fn clear_readonly(path: &str) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(path, perm);
        }
    }
}

fn remove_file(path: &str) -> std::io::Result<()> {
    let long = format!(r"\\?\{path}");
    match std::fs::remove_file(&long) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            clear_readonly(&long);
            std::fs::remove_file(&long)
        }
        r => r,
    }
}

fn remove_dir(path: &str) -> std::io::Result<()> {
    std::fs::remove_dir(format!(r"\\?\{path}"))
}

/// Exécute la suppression. Renvoie les ids effectivement supprimés.
pub fn execute(targets: &[Target], mode: CleanMode, result: &mut CleanResult) -> Vec<u32> {
    match mode {
        CleanMode::Trash => trash(targets, result),
        CleanMode::Permanent => permanent(targets, result),
    }
}

fn push_failure(result: &mut CleanResult, path: &str, error: String) {
    result.failed_count += 1;
    if result.failed.len() < 200 {
        result.failed.push(Failure { path: path.to_string(), error });
    }
}

fn permanent(targets: &[Target], result: &mut CleanResult) -> Vec<u32> {
    let mut removed = Vec::new();
    for t in targets {
        if !t.is_dir {
            match remove_file(&t.path) {
                Ok(()) => removed.push(t.id),
                Err(e) => push_failure(result, &t.path, e.to_string()),
            }
            continue;
        }
        // Fichiers en parallèle, puis dossiers du plus profond au moins profond.
        let files: Vec<&(u32, String, bool)> = t.descendants.iter().filter(|d| !d.2).collect();
        let outcomes: Vec<(u32, &str, Option<String>)> = files
            .par_iter()
            .map(|(id, path, _)| (*id, path.as_str(), remove_file(path).err().map(|e| e.to_string())))
            .collect();
        let mut failed_any = false;
        for (id, path, err) in outcomes {
            match err {
                None => removed.push(id),
                Some(e) => {
                    failed_any = true;
                    push_failure(result, path, e);
                }
            }
        }
        for (id, path, is_dir) in &t.descendants {
            if *is_dir && remove_dir(path).is_ok() {
                removed.push(*id);
            }
        }
        match remove_dir(&t.path) {
            Ok(()) => removed.push(t.id),
            Err(e) if !failed_any => push_failure(result, &t.path, e.to_string()),
            Err(_) => {}
        }
    }
    removed
}

fn trash(targets: &[Target], result: &mut CleanResult) -> Vec<u32> {
    let mut removed = Vec::new();
    unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
    for chunk in targets.chunks(64) {
        // Liste de chemins séparés par des zéros, terminée par un double zéro.
        let mut from: Vec<u16> = Vec::new();
        for t in chunk {
            from.extend(t.path.encode_utf16());
            from.push(0);
        }
        from.push(0);
        let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        op.wFunc = FO_DELETE;
        op.pFrom = from.as_ptr();
        // FOF_WANTNUKEWARNING : Windows prévient si un élément est trop gros pour la Corbeille.
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING) as u16;
        let code = unsafe { SHFileOperationW(&mut op) };
        // L'opération peut réussir partiellement : on vérifie chaque élément.
        for t in chunk {
            if Path::new(&format!(r"\\?\{}", t.path)).exists() {
                let msg = if op.fAnyOperationsAborted != 0 { "Opération annulée".to_string() } else { format!("Échec de l'envoi à la Corbeille (code {code:#x})") };
                push_failure(result, &t.path, msg);
            } else {
                removed.push(t.id);
            }
        }
    }
    unsafe { CoUninitialize() };
    removed
}

/// Vide la Corbeille d'un lecteur (ex. `C:\`).
pub fn empty_recycle_bin(root: &str) -> Result<(), String> {
    let w = win::wide(root);
    let hr = unsafe { SHEmptyRecycleBinW(null_mut(), w.as_ptr(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
    // S_OK, ou E_UNEXPECTED quand la Corbeille est déjà vide.
    if hr >= 0 || hr as u32 == 0x8000_FFFF { Ok(()) } else { Err(format!("Échec du vidage de la Corbeille ({hr:#x})")) }
}

pub fn reveal(path: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn open(path: &str) -> Result<(), String> {
    Command::new("explorer.exe").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn disk_cleanup(letter: &str) -> Result<(), String> {
    let l: String = letter.chars().filter(|c| c.is_ascii_alphabetic()).take(1).collect();
    Command::new("cleanmgr.exe")
        .arg("/d")
        .arg(if l.is_empty() { "C".into() } else { l })
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Relance l'application avec les droits administrateur (mode Turbo).
pub fn restart_as_admin() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let file = win::wide(&exe.to_string_lossy());
    let verb = win::wide("runas");
    let r = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    if r as isize > 32 { Ok(()) } else { Err("Élévation refusée ou annulée".into()) }
}
