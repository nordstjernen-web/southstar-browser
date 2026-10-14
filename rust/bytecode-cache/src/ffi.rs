//! Southstar — the GLib calls behind the JavaScript bytecode cache: the cache directory and SHA-256.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::fmt::Write;
use std::fs::DirBuilder;
use std::path::{Path, PathBuf};

use southstar_glib as glib;

const SHA256_LEN: usize = 32;

pub(crate) fn user_cache_dir() -> PathBuf {
    let dir = unsafe { glib::bytes(glib::g_get_user_cache_dir()) }.unwrap_or_default();
    path_from_glib(dir)
}

#[cfg(unix)]
fn path_from_glib(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_glib(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn create_private_dir(path: &Path) {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(path);
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    let mut digest = [0u8; SHA256_LEN];
    let mut len = SHA256_LEN;
    unsafe {
        let checksum = glib::g_checksum_new(glib::G_CHECKSUM_SHA256);
        glib::g_checksum_update(checksum, data.as_ptr(), data.len() as isize);
        glib::g_checksum_get_digest(checksum, digest.as_mut_ptr(), &mut len);
        glib::g_checksum_free(checksum);
    }
    let mut hex = String::with_capacity(len * 2);
    for b in &digest[..len] {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
