//! Southstar — the C ABI of the JavaScript bytecode cache, as declared in src/bytecode_cache.h, and the GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::fmt::Write;
use core::ptr;
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

fn g_malloc_copy(bytes: &[u8]) -> *mut u8 {
    unsafe {
        let out = glib::g_malloc(bytes.len()).cast::<u8>();
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        out
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_bytecode_cache_init() {
    crate::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_bytecode_cache_shutdown() {
    crate::shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bytecode_cache_get(
    src: *const c_char,
    src_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    if src.is_null() {
        return ptr::null_mut();
    }
    let src = unsafe { glib::slice(src.cast(), src_len) };
    let Some(bytecode) = crate::get(src) else {
        return ptr::null_mut();
    };
    if let Some(out_len) = unsafe { out_len.as_mut() } {
        *out_len = bytecode.len();
    }
    g_malloc_copy(&bytecode)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bytecode_cache_put(
    src: *const c_char,
    src_len: usize,
    bc: *const u8,
    bc_len: usize,
) {
    if src.is_null() || bc.is_null() {
        return;
    }
    let src = unsafe { glib::slice(src.cast(), src_len) };
    let bytecode = unsafe { glib::slice(bc, bc_len) };
    crate::put(src, bytecode);
}
