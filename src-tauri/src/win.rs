//! Petits utilitaires autour de l'API Win32 (handles, chemins longs, privilèges).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
    STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Handle Win32 fermé automatiquement.
pub struct Handle(pub HANDLE);

// Un handle noyau peut être utilisé depuis n'importe quel thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    pub fn is_valid(&self) -> bool {
        self.0 != INVALID_HANDLE_VALUE && !self.0.is_null()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.is_valid() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Chaîne UTF-16 terminée par un zéro.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Préfixe `\\?\` pour dépasser la limite MAX_PATH (sans zéro final).
pub fn long_path(path: &str) -> Vec<u16> {
    let p = path.replace('/', "\\");
    let prefixed = if p.starts_with(r"\\?\") {
        p
    } else if let Some(unc) = p.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}")
    } else {
        format!(r"\\?\{p}")
    };
    prefixed.encode_utf16().collect()
}

/// Retire le préfixe `\\?\` pour l'affichage.
pub fn display_path(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(p) = path.strip_prefix(r"\\?\") {
        p.to_string()
    } else {
        path.to_string()
    }
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let token = Handle(token);
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token.0,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Active un privilège (ex. SeBackupPrivilege) sur le processus courant.
pub fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let token = Handle(token);
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        let wname = wide(name);
        if LookupPrivilegeValueW(null(), wname.as_ptr(), &mut luid) == 0 {
            return false;
        }
        let mut tp: TOKEN_PRIVILEGES = std::mem::zeroed();
        tp.PrivilegeCount = 1;
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        AdjustTokenPrivileges(token.0, 0, &tp, 0, null_mut(), null_mut()) != 0
            && windows_sys::Win32::Foundation::GetLastError() == 0
    }
}

/// Ouvre un volume brut (`\\.\C:`). `access` = 0 pour les requêtes de métadonnées.
pub fn open_volume(letter: char, access: u32) -> Option<Handle> {
    let path = wide(&format!(r"\\.\{letter}:"));
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    let h = Handle(h);
    h.is_valid().then_some(h)
}

/// `Some(true)` si le disque est mécanique (pénalité de recherche), `Some(false)` pour un SSD.
pub fn has_seek_penalty(letter: char) -> Option<bool> {
    let h = open_volume(letter, 0)?;
    unsafe {
        let mut query: STORAGE_PROPERTY_QUERY = std::mem::zeroed();
        query.PropertyId = StorageDeviceSeekPenaltyProperty;
        query.QueryType = PropertyStandardQuery;
        let mut desc: DEVICE_SEEK_PENALTY_DESCRIPTOR = std::mem::zeroed();
        let mut ret = 0u32;
        let ok = DeviceIoControl(
            h.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const _,
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            &mut desc as *mut _ as *mut _,
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            &mut ret,
            null_mut(),
        );
        (ok != 0).then_some(desc.IncursSeekPenalty)
    }
}

/// FILETIME (100 ns depuis 1601) -> secondes Unix.
#[inline]
pub fn filetime_to_unix(ft: i64) -> i64 {
    if ft <= 0 {
        0
    } else {
        ft / 10_000_000 - 11_644_473_600
    }
}

/// Lettre de lecteur d'un chemin (`C:\...` -> `C`).
pub fn drive_letter(path: &str) -> Option<char> {
    let p = display_path(path);
    let mut chars = p.chars();
    let c = chars.next()?;
    (c.is_ascii_alphabetic() && chars.next() == Some(':')).then(|| c.to_ascii_uppercase())
}
