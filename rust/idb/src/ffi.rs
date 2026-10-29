//! Southstar — the file, hashing, clock and logging helpers behind IndexedDB storage, and the nsr_idb_install entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::{CString, OsStr};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use southstar_glib as glib;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

#[cfg(unix)]
fn path_of(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(until_nul(bytes)))
}

#[cfg(not(unix))]
fn path_of(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(until_nul(bytes)).into_owned())
}

#[cfg(unix)]
fn os_bytes(text: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    text.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn os_bytes(text: &OsStr) -> Vec<u8> {
    text.to_string_lossy().into_owned().into_bytes()
}

pub fn sha256_hex(input: &[u8]) -> Vec<u8> {
    southstar_digest::sha256_hex(until_nul(input)).into_bytes()
}

pub fn user_data_dir() -> Vec<u8> {
    // SAFETY: g_get_user_data_dir returns GLib's own NUL-terminated string, which lives for the whole process.
    unsafe { glib::bytes(glib::g_get_user_data_dir()) }
        .unwrap_or_default()
        .to_vec()
}

pub fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    if !(2..=4).contains(&parts.len()) {
        return Vec::new();
    }
    let joined = parts
        .iter()
        .fold(PathBuf::new(), |acc, part| acc.join(path_of(part)));
    os_bytes(joined.as_os_str())
}

#[cfg(unix)]
fn make_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn make_private(_path: &Path) {}

pub fn make_private_dir(dir: &[u8]) {
    let dir = path_of(dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir);
    }
    #[cfg(not(unix))]
    let _ = std::fs::create_dir_all(&dir);
    make_private(&dir);
}

pub fn unlink(path: &[u8]) {
    let _ = std::fs::remove_file(path_of(path));
}

pub fn dir_entries(dir: &[u8]) -> Option<Vec<Vec<u8>>> {
    let entries = std::fs::read_dir(path_of(dir)).ok()?;
    Some(
        entries
            .map_while(Result::ok)
            .map(|entry| os_bytes(&entry.file_name()))
            .collect(),
    )
}

pub fn monotonic_us() -> i64 {
    static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
    i64::try_from(EPOCH.elapsed().as_micros()).unwrap_or(i64::MAX)
}

pub fn ascii_strtoll(text: &[u8]) -> i64 {
    let text = until_nul(text);
    let start = text
        .iter()
        .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(text.len());
    let mut rest = &text[start..];
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'-' | b'+')) {
        rest = &rest[1..];
    }
    let magnitude = rest
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i128, |acc, &d| {
            (acc * 10 + i128::from(d - b'0')).min(i128::from(i64::MAX) + 1)
        });
    let value = if negative { -magnitude } else { magnitude };
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

pub fn env(name: &CStr) -> Option<Vec<u8>> {
    let name = name.to_str().ok()?;
    std::env::var_os(name).map(|v| os_bytes(&v))
}

pub fn warning(message: &[u8]) {
    southstar_glib::warning(message);
}

pub fn partition(scope: &Scope<'_>) -> Option<Vec<u8>> {
    api::partition_key(api::Page::current(scope)).map(CString::into_bytes)
}

pub fn nsr_idb_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install(scope, global);
}
