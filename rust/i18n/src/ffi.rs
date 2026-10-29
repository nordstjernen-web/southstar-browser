//! Southstar — the operating-system calls behind UI string translation: environment, language names and reading catalogue files.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use southstar_glib as glib;

fn path_of(path: &CStr) -> Option<PathBuf> {
    glib::path_from_bytes(path.to_bytes())
}

fn c_path(path: &Path) -> CString {
    CString::new(glib::path_bytes(path)).unwrap_or_default()
}

pub(crate) fn getenv(name: &CStr) -> Option<CString> {
    let value = glib::env_bytes(name.to_str().ok()?)?;
    CString::new(value).ok()
}

pub(crate) fn language_names() -> Vec<CString> {
    glib::language_names()
        .into_iter()
        .filter_map(|name| CString::new(name).ok())
        .collect()
}

pub(crate) fn build_filename(parts: &[&CStr]) -> CString {
    let mut path = PathBuf::new();
    for part in parts.iter().filter_map(|part| path_of(part)) {
        path.push(part);
    }
    c_path(&path)
}

pub(crate) fn path_get_dirname(path: &CStr) -> CString {
    match path_of(path).as_deref().and_then(Path::parent) {
        Some(dir) if !dir.as_os_str().is_empty() => c_path(dir),
        _ => CString::from(c"."),
    }
}

pub(crate) fn is_regular_file(path: &CStr) -> bool {
    path_of(path).is_some_and(|path| path.is_file())
}

pub(crate) fn file_text(path: &CStr) -> Option<Vec<u8>> {
    std::fs::read(path_of(path)?).ok()
}
