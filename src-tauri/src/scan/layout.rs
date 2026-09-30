//! Énumération complète d'un volume NTFS via `FSCTL_QUERY_FILE_LAYOUT`
//! (mode « turbo », administrateur requis).
//!
//! C'est l'API utilisée par les outils de sauvegarde Windows : le système de
//! fichiers parcourt lui-même la MFT et renvoie, par gros lots, les noms, le
//! dossier parent, les attributs, les dates et la taille de chaque flux. Elle
//! fonctionne même quand la lecture brute du disque est bloquée. LECTURE SEULE.

use std::ptr::null_mut;
use std::sync::atomic::Ordering;

use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, GENERIC_READ, GetLastError};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::mft::{Rec, build, volume_data};
use super::{PHASE_READING_MFT, ScanCtx};
use crate::tree::TreeBuilder;
use crate::win;

const FSCTL_QUERY_FILE_LAYOUT: u32 = 0x0009_0277;

const QUERY_FILE_LAYOUT_RESTART: u32 = 0x1;
const QUERY_FILE_LAYOUT_INCLUDE_NAMES: u32 = 0x2;
const QUERY_FILE_LAYOUT_INCLUDE_STREAMS: u32 = 0x4;
const QUERY_FILE_LAYOUT_INCLUDE_EXTRA_INFO: u32 = 0x10;
const QUERY_FILE_LAYOUT_INCLUDE_STREAMS_WITH_NO_CLUSTERS_ALLOCATED: u32 = 0x20;

const FILE_LAYOUT_NAME_ENTRY_DOS: u32 = 0x2;
const ATTR_DATA: u32 = 0x80;
const OUT_BUF: usize = 4 * 1024 * 1024;

#[repr(C)]
struct QueryFileLayoutInput {
    number_of_pairs: u32,
    flags: u32,
    filter_type: u32,
    reserved: u32,
    filter: [u64; 2],
}

#[inline]
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
#[inline]
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn scan(root: &str, ctx: &ScanCtx) -> Result<TreeBuilder, String> {
    let letter = win::drive_letter(root).ok_or("Lecteur invalide")?;
    let vd = volume_data(letter)?;
    let vol = win::open_volume(letter, GENERIC_READ).ok_or("Ouverture du volume refusée")?;
    let capacity = (vd.mft_valid / vd.record_size as u64) as usize;
    let mut recs: Vec<Rec> = vec![Rec::default(); capacity];

    ctx.phase.store(PHASE_READING_MFT, Ordering::Relaxed);
    ctx.total.store(capacity as u64, Ordering::Relaxed);
    *ctx.current.lock() = format!("{letter}:\\ (table des fichiers)");

    let mut out = vec![0u64; OUT_BUF / 8];
    let mut first = true;
    let mut seen = 0u64;
    loop {
        if ctx.cancelled() {
            return Err("Analyse annulée".into());
        }
        let mut flags = QUERY_FILE_LAYOUT_INCLUDE_NAMES
            | QUERY_FILE_LAYOUT_INCLUDE_STREAMS
            | QUERY_FILE_LAYOUT_INCLUDE_EXTRA_INFO
            | QUERY_FILE_LAYOUT_INCLUDE_STREAMS_WITH_NO_CLUSTERS_ALLOCATED;
        if first {
            flags |= QUERY_FILE_LAYOUT_RESTART;
        }
        let input = QueryFileLayoutInput { number_of_pairs: 0, flags, filter_type: 0, reserved: 0, filter: [0; 2] };
        let mut ret = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                vol.0,
                FSCTL_QUERY_FILE_LAYOUT,
                &input as *const _ as *const _,
                size_of::<QueryFileLayoutInput>() as u32,
                out.as_mut_ptr().cast(),
                OUT_BUF as u32,
                &mut ret,
                null_mut(),
            )
        };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_HANDLE_EOF {
                break;
            }
            return Err(format!("FSCTL_QUERY_FILE_LAYOUT a échoué (erreur Win32 {err})"));
        }
        first = false;
        let buf: &[u8] = unsafe { std::slice::from_raw_parts(out.as_ptr().cast(), (ret as usize).min(OUT_BUF)) };
        if buf.len() < 16 {
            break;
        }
        let count = u32_at(buf, 0);
        let mut off = u32_at(buf, 4) as usize;
        for _ in 0..count {
            if off == 0 || off + 40 > buf.len() {
                break;
            }
            parse_entry(buf, off, &mut recs);
            seen += 1;
            let next = u32_at(buf, off + 4) as usize;
            if next == 0 {
                break;
            }
            off += next;
        }
        ctx.done.store(seen.min(capacity as u64), Ordering::Relaxed);
        ctx.files.store(seen, Ordering::Relaxed);
    }
    build(root, recs, ctx)
}

/// Décode un FILE_LAYOUT_ENTRY situé à `off` dans `buf`.
fn parse_entry(buf: &[u8], off: usize, recs: &mut Vec<Rec>) {
    let e = &buf[off..];
    let attrs = u32_at(e, 12);
    let record = (u64_at(e, 16) & 0x0000_FFFF_FFFF_FFFF) as usize;
    let first_name = u32_at(e, 24) as usize;
    let first_stream = u32_at(e, 28) as usize;
    let extra_off = u32_at(e, 32) as usize;
    let extra_len = u32_at(e, 36) as usize;
    if record >= recs.len() {
        recs.resize(record + 1, Rec::default());
    }
    let r = &mut recs[record];
    r.in_use = true;
    r.is_dir = attrs & 0x10 != 0;
    r.attrs = attrs;

    // Noms (liens durs + nom court DOS) : on garde le premier nom long.
    let mut noff = first_name;
    while noff != 0 && noff + 24 <= e.len() {
        let n = &e[noff..];
        let flags = u32_at(n, 4);
        let parent = (u64_at(n, 8) & 0x0000_FFFF_FFFF_FFFF) as u32;
        let len = u32_at(n, 16) as usize;
        let is_dos = flags == FILE_LAYOUT_NAME_ENTRY_DOS;
        if 24 + len <= n.len() && (r.name.is_none() || (r.name_ns == 2 && !is_dos)) {
            let units: Vec<u16> = (0..len / 2).map(|k| u16::from_le_bytes([n[24 + k * 2], n[25 + k * 2]])).collect();
            r.name = Some(String::from_utf16_lossy(&units).into_boxed_str());
            r.name_ns = if is_dos { 2 } else { 1 };
            r.parent = parent;
        }
        let next = u32_at(n, 0) as usize;
        if next == 0 {
            break;
        }
        noff += next;
    }

    // Flux : taille du flux de données principal, espace alloué de tous les flux.
    let mut soff = first_stream;
    while soff != 0 && soff + 48 <= e.len() {
        let s = &e[soff..];
        let alloc = u64_at(s, 16);
        let eof = u64_at(s, 24);
        let type_code = u32_at(s, 36);
        let id_len = u32_at(s, 44) as usize;
        r.alloc += alloc;
        // Le flux principal a un identifiant vide ou « ::$DATA » ; les flux nommés « :nom:$DATA ».
        let main = id_len == 0 || (48 + 4 <= s.len() && s[48] == b':' && s[50] == b':');
        if type_code == ATTR_DATA && main {
            r.size += eof;
        }
        let next = u32_at(s, 4) as usize;
        if next == 0 {
            break;
        }
        soff += next;
    }

    // Informations complémentaires : date de dernière modification.
    if extra_off != 0 && extra_len >= 24 && extra_off + 24 <= e.len() {
        r.mtime = win::filetime_to_unix(u64_at(e, extra_off + 16) as i64);
    }
}
