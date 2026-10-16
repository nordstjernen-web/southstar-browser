//! Southstar — the GLib file, hashing, clock and logging calls behind IndexedDB storage, and the nsr_idb_install entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GStr};
use southstar_js_engine::{Scope, Value};

const LOG_LEVEL_WARNING: c_int = 1 << 4;

#[repr(C)]
struct GDir {
    _private: [u8; 0],
}

unsafe extern "C" {

    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_unlink(filename: *const c_char) -> c_int;
    fn g_dir_open(path: *const c_char, flags: u32, error: *mut *mut c_void) -> *mut GDir;
    fn g_dir_read_name(dir: *mut GDir) -> *const c_char;
    fn g_dir_close(dir: *mut GDir);
    fn g_get_monotonic_time() -> i64;
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: u32) -> i64;
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
    fn ns_js_storage_partition(js: *const c_void) -> *const c_char;
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn sha256_hex(input: &[u8]) -> Vec<u8> {
    let end = input.iter().position(|&b| b == 0).unwrap_or(input.len());
    southstar_digest::sha256_hex(&input[..end]).into_bytes()
}

pub fn user_data_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_data_dir()) }
        .unwrap_or_default()
        .to_vec()
}

pub fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let parts: Vec<CString> = parts.iter().map(|p| cstring(p)).collect();
    let joined = unsafe {
        match parts.as_slice() {
            [a, b] => g_build_filename(a.as_ptr(), b.as_ptr(), ptr::null::<c_char>()),
            [a, b, c] => {
                g_build_filename(a.as_ptr(), b.as_ptr(), c.as_ptr(), ptr::null::<c_char>())
            }
            [a, b, c, d] => g_build_filename(
                a.as_ptr(),
                b.as_ptr(),
                c.as_ptr(),
                d.as_ptr(),
                ptr::null::<c_char>(),
            ),
            _ => ptr::null_mut(),
        }
    };
    unsafe { GStr::take(joined) }.map_or_else(Vec::new, |p| p.to_bytes().to_vec())
}

pub fn make_private_dir(dir: &[u8]) {
    let dir = cstring(dir);
    unsafe {
        g_mkdir_with_parents(dir.as_ptr(), 0o700);
        g_chmod(dir.as_ptr(), 0o700);
    }
}

pub fn unlink(path: &[u8]) {
    let path = cstring(path);
    unsafe { g_unlink(path.as_ptr()) };
}

pub fn dir_entries(dir: &[u8]) -> Option<Vec<Vec<u8>>> {
    let dir = cstring(dir);
    let handle = unsafe { g_dir_open(dir.as_ptr(), 0, ptr::null_mut()) };
    if handle.is_null() {
        return None;
    }
    let mut names = Vec::new();
    loop {
        let entry = unsafe { g_dir_read_name(handle) };
        if entry.is_null() {
            break;
        }
        names.push(unsafe { CStr::from_ptr(entry) }.to_bytes().to_vec());
    }
    unsafe { g_dir_close(handle) };
    Some(names)
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn ascii_strtoll(text: &[u8]) -> i64 {
    let text = cstring(text);
    unsafe { g_ascii_strtoll(text.as_ptr(), ptr::null_mut(), 10) }
}

pub fn env(name: &CStr) -> Option<Vec<u8>> {
    unsafe { glib::bytes(glib::g_getenv(name.as_ptr())) }.map(<[u8]>::to_vec)
}

pub fn warning(message: &[u8]) {
    let message = cstring(message);
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
}

pub fn partition(scope: &Scope<'_>) -> Option<Vec<u8>> {
    let partition = unsafe { glib::bytes(ns_js_storage_partition(scope.context_opaque())) }?;
    Some(partition.to_vec())
}

#[unsafe(no_mangle)]
pub fn nsr_idb_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install(scope, global);
}
