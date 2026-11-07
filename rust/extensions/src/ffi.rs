//! Southstar — the C ABI of the extension host, its rule regexes, and the file and GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use southstar_unicode::until_nul;
use std::path::PathBuf;

use southstar_glib::{self as glib, os_bytes};

#[cfg(windows)]
pub(crate) const SEARCHPATH_SEPARATOR: u8 = b';';
#[cfg(not(windows))]
pub(crate) const SEARCHPATH_SEPARATOR: u8 = b':';

#[cfg(windows)]
const DIR_SEPARATOR: u8 = b'\\';
#[cfg(not(windows))]
const DIR_SEPARATOR: u8 = b'/';

pub(crate) struct Regex(regress::Regex);

impl Regex {
    pub(crate) fn new(pattern: &[u8], case_sensitive: bool) -> Option<Regex> {
        let pattern = core::str::from_utf8(until_nul(pattern)).ok()?;
        let flags = if case_sensitive { "" } else { "i" };
        regress::Regex::with_flags(pattern, flags).ok().map(Regex)
    }

    pub(crate) fn is_match(&self, text: &[u8]) -> bool {
        self.0
            .find(&String::from_utf8_lossy(until_nul(text)))
            .is_some()
    }
}

fn path_of(bytes: &[u8]) -> PathBuf {
    southstar_glib::path_from_bytes_lossy(until_nul(bytes))
}

fn is_dir_separator(c: u8) -> bool {
    c == b'/' || (cfg!(windows) && c == b'\\')
}

pub(crate) fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut separator = DIR_SEPARATOR;
    let mut is_first = true;
    let mut have_leading = false;
    let mut single_element: Option<&[u8]> = None;
    let mut last_trailing: &[u8] = &[];
    for element in parts.iter().map(until_nul) {
        if element.is_empty() {
            continue;
        }
        let start = element
            .iter()
            .position(|&c| !is_dir_separator(c))
            .unwrap_or(element.len());
        if cfg!(windows) && start > 0 {
            separator = element[start - 1];
        }
        let end = element[start..]
            .iter()
            .rposition(|&c| !is_dir_separator(c))
            .map_or(start, |i| start + i + 1);
        if cfg!(windows) && end < element.len() {
            separator = element[end];
        }
        let trailing_from = if end > start { end } else { 0 };
        last_trailing = &element[trailing_from..];
        if !have_leading {
            if end == start {
                single_element = Some(element);
            }
            result.extend_from_slice(&element[..start]);
            have_leading = true;
        } else {
            single_element = None;
        }
        if end == start {
            continue;
        }
        if !is_first {
            result.push(separator);
        }
        result.extend_from_slice(&element[start..end]);
        is_first = false;
    }
    match single_element {
        Some(element) => element.to_vec(),
        None => {
            result.extend_from_slice(last_trailing);
            result
        }
    }
}

pub(crate) fn basename(path: &[u8]) -> Vec<u8> {
    let path = until_nul(path);
    if path.is_empty() {
        return b".".to_vec();
    }
    let Some(last) = path.iter().rposition(|&c| !is_dir_separator(c)) else {
        return vec![DIR_SEPARATOR];
    };
    let has_drive =
        cfg!(windows) && path.len() >= 2 && path[0].is_ascii_alphabetic() && path[1] == b':';
    if has_drive && last == 1 {
        return vec![DIR_SEPARATOR];
    }
    let base = match path[..=last].iter().rposition(|&c| is_dir_separator(c)) {
        Some(i) => i + 1,
        None if has_drive => 2,
        None => 0,
    };
    path[base..=last].to_vec()
}

pub(crate) fn user_data_dir() -> Vec<u8> {
    southstar_glib::user_data_dir()
}

pub(crate) fn getenv(name: &CStr) -> Option<Vec<u8>> {
    let name = name.to_str().ok()?;
    std::env::var_os(name).map(|value| os_bytes(&value))
}

pub(crate) fn language_name() -> Option<Vec<u8>> {
    glib::language_names().into_iter().next()
}

pub(crate) fn sha256_hex(text: &[u8]) -> Vec<u8> {
    southstar_digest::sha256_hex(until_nul(text)).into_bytes()
}

pub(crate) fn exists(path: &[u8]) -> bool {
    path_of(path).exists()
}

pub(crate) fn is_dir(path: &[u8]) -> bool {
    path_of(path).is_dir()
}

pub(crate) fn list_dir(path: &[u8]) -> Vec<Vec<u8>> {
    std::fs::read_dir(path_of(path)).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .map_while(Result::ok)
                .map(|entry| os_bytes(&entry.file_name()))
                .collect()
        },
    )
}

pub(crate) fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(path_of(path)).ok()
}

pub(crate) fn make_private_dir(path: &[u8]) {
    southstar_config::util::make_private_dirs(&path_of(path));
}

pub(crate) fn write_private_file(path: &[u8], data: &[u8]) -> bool {
    let path = path_of(path);
    if southstar_config::util::write_replacing(&path, data).is_err() {
        return false;
    }
    let _ = southstar_config::util::set_mode(&path, 0o600);
    true
}

pub fn registrable_domain(host: &[u8]) -> Option<Vec<u8>> {
    let host = host.split(|&b| b == 0).next().unwrap_or_default();
    southstar_psl::registrable_domain(host).map(<[u8]>::to_vec)
}
