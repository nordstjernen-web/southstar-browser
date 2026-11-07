//! Southstar GLib — the user and system directories and the user's languages, resolved the way GLib resolves them, without GLib.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::path::Path;

fn env_dir(name: &str) -> Option<Vec<u8>> {
    let value = env::var_os(name)?;
    let bytes = crate::path_bytes(Path::new(&value));
    (!bytes.is_empty()).then_some(bytes)
}

fn joined(base: &[u8], parts: &[&str]) -> Vec<u8> {
    let mut path = crate::path_from_bytes(base).unwrap_or_default();
    for part in parts {
        path.push(part);
    }
    crate::path_bytes(&path)
}

pub fn user_config_dir() -> Vec<u8> {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(platform::config_dir)
}

pub fn user_data_dir() -> Vec<u8> {
    env_dir("XDG_DATA_HOME").unwrap_or_else(platform::data_dir)
}

pub fn user_cache_dir() -> Vec<u8> {
    env_dir("XDG_CACHE_HOME").unwrap_or_else(platform::cache_dir)
}

pub fn user_runtime_dir() -> Vec<u8> {
    env_dir("XDG_RUNTIME_DIR").unwrap_or_else(user_cache_dir)
}

pub fn home_dir() -> Vec<u8> {
    platform::home_dir()
}

pub fn tmp_dir() -> Vec<u8> {
    platform::tmp_dir()
}

pub fn download_dir() -> Vec<u8> {
    platform::download_dir()
}

pub fn system_data_dirs() -> Vec<Vec<u8>> {
    platform::system_data_dirs()
}

const CODESET: u8 = 1;
const TERRITORY: u8 = 2;
const MODIFIER: u8 = 4;

fn locale_variants(locale: &str, out: &mut Vec<Vec<u8>>) {
    let (rest, modifier) = locale
        .split_once('@')
        .map_or((locale, None), |(rest, m)| (rest, Some(m)));
    let (rest, codeset) = rest
        .split_once('.')
        .map_or((rest, None), |(rest, c)| (rest, Some(c)));
    let (language, territory) = rest
        .split_once('_')
        .map_or((rest, None), |(l, t)| (l, Some(t)));
    let mask = u8::from(codeset.is_some()) * CODESET
        + u8::from(territory.is_some()) * TERRITORY
        + u8::from(modifier.is_some()) * MODIFIER;
    for i in (0..=mask).rev().filter(|i| i & !mask == 0) {
        let mut name = language.to_owned();
        if i & TERRITORY != 0 {
            name.push('_');
            name.push_str(territory.unwrap_or_default());
        }
        if i & CODESET != 0 {
            name.push('.');
            name.push_str(codeset.unwrap_or_default());
        }
        if i & MODIFIER != 0 {
            name.push('@');
            name.push_str(modifier.unwrap_or_default());
        }
        out.push(name.into_bytes());
    }
}

fn locale_setting() -> String {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .or_else(platform::user_locale)
        .unwrap_or_else(|| "C".to_owned())
}

pub fn language_names() -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    for locale in locale_setting().split(':').filter(|l| !l.is_empty()) {
        locale_variants(locale, &mut names);
    }
    if !names.iter().any(|name| name == b"C") {
        names.push(b"C".to_vec());
    }
    names
}

#[cfg(not(windows))]
mod platform {
    use std::env;

    use super::{env_dir, joined};

    pub(super) fn home_dir() -> Vec<u8> {
        env_dir("HOME").unwrap_or_else(|| b"/".to_vec())
    }

    pub(super) fn config_dir() -> Vec<u8> {
        joined(&home_dir(), &[".config"])
    }

    pub(super) fn data_dir() -> Vec<u8> {
        joined(&home_dir(), &[".local", "share"])
    }

    pub(super) fn cache_dir() -> Vec<u8> {
        joined(&home_dir(), &[".cache"])
    }

    pub(super) fn tmp_dir() -> Vec<u8> {
        let tmp = env_dir("TMPDIR").unwrap_or_else(|| b"/tmp".to_vec());
        match tmp.strip_suffix(b"/") {
            Some(trimmed) if !trimmed.is_empty() => trimmed.to_vec(),
            _ => tmp,
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn download_dir() -> Vec<u8> {
        joined(&home_dir(), &["Downloads"])
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn download_dir() -> Vec<u8> {
        let file = joined(&super::user_config_dir(), &["user-dirs.dirs"]);
        let Some(text) = crate::path_from_bytes(&file).and_then(|p| std::fs::read(p).ok()) else {
            return Vec::new();
        };
        let home = home_dir();
        for line in text.split(|&b| b == b'\n') {
            let Some(value) = line.trim_ascii().strip_prefix(b"XDG_DOWNLOAD_DIR=") else {
                continue;
            };
            let value = value
                .strip_prefix(b"\"")
                .and_then(|v| v.strip_suffix(b"\""))
                .unwrap_or(value);
            let mut path = if let Some(rest) = value.strip_prefix(b"$HOME") {
                [&home[..], rest].concat()
            } else if value.starts_with(b"/") {
                value.to_vec()
            } else {
                continue;
            };
            while path.len() > 1 && path.ends_with(b"/") {
                path.pop();
            }
            return path;
        }
        Vec::new()
    }

    pub(super) fn system_data_dirs() -> Vec<Vec<u8>> {
        let value = env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share/:/usr/share/".to_owned());
        value
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| dir.as_bytes().to_vec())
            .collect()
    }

    pub(super) fn user_locale() -> Option<String> {
        None
    }
}

#[cfg(windows)]
mod platform {
    use core::ffi::c_void;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use super::{env_dir, joined};

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const LOCAL_APP_DATA: Guid = Guid {
        data1: 0xF1B3_2785,
        data2: 0x6FBA,
        data3: 0x4FCF,
        data4: [0x9D, 0x55, 0x7B, 0x8E, 0x7F, 0x15, 0x70, 0x91],
    };
    const INTERNET_CACHE: Guid = Guid {
        data1: 0x3524_81E8,
        data2: 0x33BE,
        data3: 0x4251,
        data4: [0xBA, 0x85, 0x60, 0x07, 0xCA, 0xED, 0xCF, 0x9D],
    };
    const DOWNLOADS: Guid = Guid {
        data1: 0x374D_E290,
        data2: 0x123F,
        data3: 0x4565,
        data4: [0x91, 0x64, 0x39, 0xC4, 0x92, 0x5E, 0x46, 0x7B],
    };
    const PROFILE: Guid = Guid {
        data1: 0x5E6C_858F,
        data2: 0x0E22,
        data3: 0x4760,
        data4: [0x9A, 0xFE, 0xEA, 0x33, 0x17, 0xB6, 0x71, 0x73],
    };
    const PUBLIC_DOCUMENTS: Guid = Guid {
        data1: 0xED48_24AF,
        data2: 0xDCE4,
        data3: 0x45A8,
        data4: [0x81, 0xE2, 0xFC, 0x79, 0x65, 0x08, 0x36, 0x34],
    };
    const PROGRAM_DATA: Guid = Guid {
        data1: 0x62AB_5D82,
        data2: 0xFDC1,
        data3: 0x4DC3,
        data4: [0xA9, 0xDD, 0x07, 0x0D, 0x1D, 0x49, 0x5D, 0x97],
    };

    const LOCALE_NAME_MAX_LENGTH: usize = 85;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHGetKnownFolderPath(
            folder: *const Guid,
            flags: u32,
            token: *mut c_void,
            path: *mut *mut u16,
        ) -> i32;
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(memory: *mut c_void);
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, length: i32) -> i32;
    }

    fn wide_len(text: *const u16) -> usize {
        let mut len = 0;
        loop {
            let unit = text.wrapping_add(len);
            // SAFETY: text is a NUL-terminated wide string and len has not passed its terminator, so unit points into it.
            if unsafe { *unit } == 0 {
                return len;
            }
            len += 1;
        }
    }

    fn known_folder(folder: &Guid) -> Option<Vec<u8>> {
        let mut path = core::ptr::null_mut();
        // SAFETY: folder is a live GUID, a null token selects the current user and path is a valid out-pointer.
        let hr = unsafe { SHGetKnownFolderPath(folder, 0, core::ptr::null_mut(), &mut path) };
        if path.is_null() {
            return None;
        }
        let text = (hr >= 0).then(|| {
            // SAFETY: on success path is a NUL-terminated wide string the shell allocated.
            let wide = unsafe { core::slice::from_raw_parts(path, wide_len(path)) };
            OsString::from_wide(wide)
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        });
        // SAFETY: path was allocated by SHGetKnownFolderPath and is freed exactly once, here.
        unsafe { CoTaskMemFree(path.cast()) };
        text.filter(|text| !text.is_empty())
    }

    pub(super) fn home_dir() -> Vec<u8> {
        env_dir("USERPROFILE")
            .or_else(|| known_folder(&PROFILE))
            .unwrap_or_else(|| b"C:\\".to_vec())
    }

    pub(super) fn config_dir() -> Vec<u8> {
        known_folder(&LOCAL_APP_DATA).unwrap_or_else(|| joined(&home_dir(), &[".config"]))
    }

    pub(super) fn data_dir() -> Vec<u8> {
        known_folder(&LOCAL_APP_DATA).unwrap_or_else(|| joined(&home_dir(), &[".local", "share"]))
    }

    pub(super) fn cache_dir() -> Vec<u8> {
        known_folder(&INTERNET_CACHE).unwrap_or_else(|| joined(&home_dir(), &[".cache"]))
    }

    pub(super) fn tmp_dir() -> Vec<u8> {
        env_dir("TEMP").unwrap_or_else(|| b"C:\\".to_vec())
    }

    pub(super) fn download_dir() -> Vec<u8> {
        known_folder(&DOWNLOADS).unwrap_or_default()
    }

    pub(super) fn system_data_dirs() -> Vec<Vec<u8>> {
        let mut dirs: Vec<Vec<u8>> = [&PROGRAM_DATA, &PUBLIC_DOCUMENTS]
            .into_iter()
            .filter_map(known_folder)
            .collect();
        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        {
            let in_bin_or_lib = exe_dir
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("bin") || name.eq_ignore_ascii_case("lib")
                });
            let prefix = if in_bin_or_lib {
                exe_dir
                    .parent()
                    .map_or(exe_dir.clone(), std::path::Path::to_path_buf)
            } else {
                exe_dir
            };
            dirs.push(crate::path_bytes(&prefix.join("share")));
        }
        dirs
    }

    pub(super) fn user_locale() -> Option<String> {
        let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
        // SAFETY: buffer is a writable array of LOCALE_NAME_MAX_LENGTH wide characters, passed with its length.
        let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
        let len = usize::try_from(len).ok().filter(|&len| len > 1)? - 1;
        let name = String::from_utf16_lossy(&buffer[..len]);
        let mut parts = name.split('-');
        let language = parts.next()?.to_owned();
        let territory = parts.rfind(|part| part.len() == 2 || part.len() == 3);
        Some(match territory {
            Some(territory) => format!("{language}_{territory}"),
            None => language,
        })
    }
}
