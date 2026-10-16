//! Southstar — the GLib calls behind the debug log: clocks, the environment and the log file's location.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
}

pub fn monotonic_us() -> i64 {
    // SAFETY: g_get_monotonic_time has no preconditions.
    unsafe { g_get_monotonic_time() }
}

pub fn real_time_us() -> i64 {
    // SAFETY: g_get_real_time has no preconditions.
    unsafe { g_get_real_time() }
}

pub fn env(name: &CStr) -> Option<&'static CStr> {
    // SAFETY: name is NUL-terminated; GLib keeps the returned value for the process.
    let value = unsafe { glib::g_getenv(name.as_ptr()) };
    // SAFETY: a non-null value is a NUL-terminated string GLib keeps.
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
}

fn take(path: *mut c_char) -> Option<CString> {
    if path.is_null() {
        return None;
    }
    // SAFETY: path is a newly allocated NUL-terminated string from GLib.
    let owned = unsafe { CStr::from_ptr(path) }.to_owned();
    // SAFETY: path was allocated by GLib and is not used after this.
    unsafe { glib::g_free(path.cast()) };
    Some(owned)
}

pub fn default_log_path() -> Option<CString> {
    // SAFETY: g_get_user_data_dir has no preconditions and returns a string GLib keeps.
    let data_dir = unsafe { glib::g_get_user_data_dir() };
    // SAFETY: every element is NUL-terminated and the list ends with a null pointer.
    take(unsafe {
        g_build_filename(
            data_dir,
            c"Southstar".as_ptr(),
            c"southstar-debug.log".as_ptr(),
            ptr::null::<c_char>(),
        )
    })
}

pub fn make_parent_dir(path: &CStr) {
    // SAFETY: path is NUL-terminated.
    let Some(dir) = take(unsafe { g_path_get_dirname(path.as_ptr()) }) else {
        return;
    };
    // SAFETY: dir is a NUL-terminated path.
    unsafe { g_mkdir_with_parents(dir.as_ptr(), 0o700) };
}
