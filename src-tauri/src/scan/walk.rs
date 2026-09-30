//! Parcours parallèle des dossiers.
//!
//! - Chaque dossier est ouvert avec `NtOpenFile` *relativement au handle de son
//!   parent* : le noyau n'a pas à re-traverser le chemin complet, ce qui évite la
//!   contention sur les dossiers de haut niveau (`C:\Users`...).
//! - Le contenu est lu par lots (`GetFileInformationByHandleEx`) : taille, taille
//!   allouée, date et attributs de centaines d'entrées par appel, sans `stat`.
//! - La récursion est répartie sur un pool rayon (vol de travail), ce qui
//!   équilibre automatiquement les gros dossiers comme `C:\Windows`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering;

use rayon::prelude::*;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, GetLastError, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileFullDirectoryInfo, GetFileInformationByHandleEx, OPEN_EXISTING,
};

use super::ScanCtx;
use crate::tree::{F_CLOUD, F_COMPRESSED, F_DENIED, F_DIR, F_HIDDEN, F_REPARSE, F_SYSTEM, TreeBuilder};
use crate::win::{self, Handle};

const BUF_BYTES: usize = 256 * 1024;

thread_local! {
    static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0u64; BUF_BYTES / 8]);
}

// --- ntdll -----------------------------------------------------------------

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    security_quality_of_service: *const c_void,
}

#[repr(C)]
struct IoStatusBlock {
    status: usize,
    information: usize,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtOpenFile(
        file_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *const ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        share_access: u32,
        open_options: u32,
    ) -> i32;
}

const SYNCHRONIZE: u32 = 0x0010_0000;
const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const FILE_DIRECTORY_FILE: u32 = 0x1;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_OPEN_FOR_BACKUP_INTENT: u32 = 0x4000;

/// Ouvre `name` (relatif au dossier `parent`) pour lister son contenu.
fn open_relative(parent: HANDLE, name: &[u16]) -> Option<Handle> {
    let us = UnicodeString {
        length: (name.len() * 2) as u16,
        maximum_length: (name.len() * 2) as u16,
        buffer: name.as_ptr(),
    };
    let oa = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent,
        object_name: &us,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: null(),
        security_quality_of_service: null(),
    };
    let mut iosb = IoStatusBlock { status: 0, information: 0 };
    let mut h: HANDLE = null_mut();
    let status = unsafe {
        NtOpenFile(
            &mut h,
            FILE_LIST_DIRECTORY | SYNCHRONIZE,
            &oa,
            &mut iosb,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_FOR_BACKUP_INTENT,
        )
    };
    let h = Handle(h);
    (status >= 0 && h.is_valid()).then_some(h)
}

fn open_absolute(path: &[u16]) -> Option<Handle> {
    let mut z = path.to_vec();
    z.push(0);
    let h = Handle(unsafe {
        CreateFileW(
            z.as_ptr(),
            FILE_LIST_DIRECTORY | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    });
    h.is_valid().then_some(h)
}

// --- Parcours ----------------------------------------------------------------

struct Entry {
    name: Box<str>,
    size: u64,
    alloc: u64,
    mtime: i64,
    flags: u16,
}

struct Dir {
    entry: Entry,
    files: Vec<Entry>,
    dirs: Vec<Dir>,
}

/// Handle partageable entre threads (lecture seule pendant la durée de vie du parent).
#[derive(Clone, Copy)]
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

fn flags_from_attrs(attrs: u32) -> u16 {
    let mut f = 0;
    if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
        f |= F_DIR;
    }
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        f |= F_REPARSE;
    }
    if attrs & FILE_ATTRIBUTE_HIDDEN != 0 {
        f |= F_HIDDEN;
    }
    if attrs & FILE_ATTRIBUTE_SYSTEM != 0 {
        f |= F_SYSTEM;
    }
    if attrs & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN) != 0 {
        f |= F_CLOUD;
    }
    if attrs & FILE_ATTRIBUTE_COMPRESSED != 0 {
        f |= F_COMPRESSED;
    }
    f
}

/// Lit tout le contenu d'un dossier ouvert.
fn list_dir(h: HANDLE, files: &mut Vec<Entry>, dirs: &mut Vec<Entry>) -> Result<(), u32> {
    BUF.with_borrow_mut(|buf| {
        let byte_len = (buf.len() * 8) as u32;
        loop {
            let ok = unsafe { GetFileInformationByHandleEx(h, FileFullDirectoryInfo, buf.as_mut_ptr().cast(), byte_len) };
            if ok == 0 {
                let err = unsafe { GetLastError() };
                return if err == ERROR_NO_MORE_FILES || err == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(err) };
            }
            let base = buf.as_ptr().cast::<u8>();
            let mut off = 0usize;
            loop {
                // Structure FILE_FULL_DIR_INFO (winbase.h).
                unsafe {
                    let p = base.add(off);
                    let next = (p as *const u32).read_unaligned();
                    let mtime = (p.add(24) as *const i64).read_unaligned();
                    let eof = (p.add(40) as *const i64).read_unaligned();
                    let alloc = (p.add(48) as *const i64).read_unaligned();
                    let attrs = (p.add(56) as *const u32).read_unaligned();
                    let name_bytes = (p.add(60) as *const u32).read_unaligned() as usize;
                    let name = std::slice::from_raw_parts(p.add(68) as *const u16, name_bytes / 2);
                    let dot = name.len() <= 2 && name.iter().all(|c| *c == b'.' as u16);
                    if !dot {
                        let entry = Entry {
                            name: String::from_utf16_lossy(name).into_boxed_str(),
                            size: eof.max(0) as u64,
                            alloc: alloc.max(0) as u64,
                            mtime: win::filetime_to_unix(mtime),
                            flags: flags_from_attrs(attrs),
                        };
                        if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                            dirs.push(entry);
                        } else {
                            files.push(entry);
                        }
                    }
                    if next == 0 {
                        break;
                    }
                    off += next as usize;
                }
            }
        }
    })
}

/// Analyse un dossier déjà ouvert puis ses sous-dossiers en parallèle.
/// `path` ne sert qu'à l'affichage de la progression.
fn walk(handle: Option<Handle>, path: &str, mut entry: Entry, ctx: &ScanCtx) -> Dir {
    entry.size = 0;
    entry.alloc = 0;
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    let listed = match &handle {
        Some(h) => list_dir(h.0, &mut files, &mut subdirs).is_ok(),
        None => false,
    };
    if !listed {
        entry.flags |= F_DENIED;
        ctx.denied.fetch_add(1, Ordering::Relaxed);
    }

    let bytes: u64 = files.iter().map(|f| f.alloc).sum();
    ctx.files.fetch_add(files.len() as u64, Ordering::Relaxed);
    let dirs_done = ctx.dirs.fetch_add(1, Ordering::Relaxed);
    ctx.bytes.fetch_add(bytes, Ordering::Relaxed);
    if dirs_done % 256 == 0 {
        if let Some(mut cur) = ctx.current.try_lock() {
            cur.clear();
            cur.push_str(path);
        }
    }

    let parent = handle.as_ref().map(|h| SendHandle(h.0));
    let dirs: Vec<Dir> = subdirs
        .into_par_iter()
        .map(|e| {
            // Ne suit pas les jonctions / liens symboliques (évite boucles et doublons).
            if e.flags & F_REPARSE != 0 || ctx.cancelled() {
                return Dir { entry: e, files: Vec::new(), dirs: Vec::new() };
            }
            let wname: Vec<u16> = e.name.encode_utf16().collect();
            let child = parent.and_then(|p| open_relative(p.0, &wname));
            let child_path = if path.ends_with('\\') { format!("{path}{}", e.name) } else { format!("{path}\\{}", e.name) };
            walk(child, &child_path, e, ctx)
        })
        .collect();
    // Le handle du dossier reste ouvert jusqu'ici : il sert de racine aux ouvertures relatives.
    drop(handle);

    Dir { entry, files, dirs }
}

pub fn scan(root: &str, ctx: &ScanCtx, threads: usize) -> Result<TreeBuilder, String> {
    let meta = std::fs::metadata(root).map_err(|e| format!("Impossible d'ouvrir {root} : {e}"))?;
    if !meta.is_dir() {
        return Err(format!("{root} n'est pas un dossier"));
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(16 * 1024 * 1024)
        .thread_name(|i| format!("cmw-scan-{i}"))
        .build()
        .map_err(|e| e.to_string())?;

    let root_handle = open_absolute(&win::long_path(root)).ok_or_else(|| format!("Accès refusé : {root}"))?;
    let root_entry = Entry { name: root.into(), size: 0, alloc: 0, mtime: 0, flags: F_DIR };
    let t0 = std::time::Instant::now();
    let dir = pool.install(|| walk(Some(root_handle), root, root_entry, ctx));
    if std::env::var_os("CMW_TIMING").is_some() {
        eprintln!("walk: {:?}", t0.elapsed());
    }
    if ctx.cancelled() {
        return Err("Analyse annulée".into());
    }
    ctx.phase.store(super::PHASE_BUILDING, Ordering::Relaxed);

    // Aplatissement en largeur d'abord : enfants contigus, parent < enfant.
    let t1 = std::time::Instant::now();
    let total = (ctx.files.load(Ordering::Relaxed) + ctx.dirs.load(Ordering::Relaxed)) as usize + 1;
    let mut b = TreeBuilder::with_capacity(total, total * 24);
    let Dir { entry, files, dirs } = dir;
    let root_id = b.push(0, &entry.name, 0, 0, entry.mtime, entry.flags);
    let mut queue: VecDeque<(u32, Vec<Entry>, Vec<Dir>)> = VecDeque::new();
    queue.push_back((root_id, files, dirs));
    while let Some((id, files, dirs)) = queue.pop_front() {
        let first = b.len() as u32;
        let count = (files.len() + dirs.len()) as u32;
        let mut pending = Vec::with_capacity(dirs.len());
        for d in dirs {
            let cid = b.push(id, &d.entry.name, 0, 0, d.entry.mtime, d.entry.flags);
            pending.push((cid, d.files, d.dirs));
        }
        for f in files {
            b.push(id, &f.name, f.size, f.alloc, f.mtime, f.flags);
        }
        b.set_children(id, first, count);
        queue.extend(pending);
    }
    if std::env::var_os("CMW_TIMING").is_some() {
        eprintln!("flatten: {:?}", t1.elapsed());
    }
    Ok(b)
}
