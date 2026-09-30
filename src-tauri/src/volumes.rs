//! Informations sur les lecteurs disponibles.

use std::ptr::null_mut;

use serde::Serialize;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;

use crate::win;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub root: String,
    pub letter: String,
    pub label: String,
    pub filesystem: String,
    pub kind: &'static str,
    pub total: u64,
    pub free: u64,
    pub is_system: bool,
    pub is_ssd: Option<bool>,
}

pub fn space(root: &str) -> Option<(u64, u64)> {
    let w = win::wide(root);
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some((total, free))
}

fn volume_info(root: &str) -> (String, String) {
    let w = win::wide(root);
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let ok = unsafe {
        GetVolumeInformationW(
            w.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            null_mut(),
            null_mut(),
            null_mut(),
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        return (String::new(), String::new());
    }
    let s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|c| *c == 0).unwrap_or(b.len())]);
    (s(&label), s(&fs))
}

pub fn filesystem(root: &str) -> Option<String> {
    let letter = win::drive_letter(root)?;
    let (_, fs) = volume_info(&format!("{letter}:\\"));
    (!fs.is_empty()).then_some(fs)
}

/// Lettre du lecteur système (généralement `C`).
pub fn system_letter() -> char {
    let mut buf = [0u16; 261];
    let n = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
        .chars()
        .next()
        .unwrap_or('C')
        .to_ascii_uppercase()
}

pub fn list() -> Vec<Volume> {
    let mask = unsafe { GetLogicalDrives() };
    let sys = system_letter();
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let w = win::wide(&root);
        let kind = match unsafe { GetDriveTypeW(w.as_ptr()) } {
            2 => "removable",
            3 => "fixed",
            4 => "network",
            5 => "cdrom",
            6 => "ramdisk",
            _ => continue,
        };
        let Some((total, free)) = space(&root) else { continue };
        let (label, filesystem) = volume_info(&root);
        out.push(Volume {
            letter: letter.to_string(),
            label,
            filesystem,
            kind,
            total,
            free,
            is_system: letter == sys,
            is_ssd: if kind == "fixed" { win::has_seek_penalty(letter).map(|hdd| !hdd) } else { None },
            root,
        });
    }
    out
}
