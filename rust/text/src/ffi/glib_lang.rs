//! Southstar text — the process locale the default language is derived from: GLib's Windows locale name or the C library's LC_CTYPE.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char};

#[cfg(windows)]
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_win32_getlocale() -> *mut c_char;
    fn g_free(mem: *mut core::ffi::c_void);
}

#[cfg(windows)]
pub(crate) fn win32_getlocale() -> String {
    // SAFETY: g_win32_getlocale takes no arguments and returns a newly allocated string or NULL.
    let raw = unsafe { g_win32_getlocale() };
    if raw.is_null() {
        return String::new();
    }
    // SAFETY: raw is a NUL-terminated string GLib just allocated and nothing else references.
    let locale = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: raw came from g_win32_getlocale, which hands ownership to the caller for g_free.
    unsafe { g_free(raw.cast()) };
    locale
}

#[cfg(not(windows))]
const LC_CTYPE: core::ffi::c_int = if cfg!(any(target_os = "linux", target_os = "android")) {
    0
} else {
    2
};

#[cfg(not(windows))]
unsafe extern "C" {
    fn setlocale(category: core::ffi::c_int, locale: *const c_char) -> *mut c_char;
}

#[cfg(not(windows))]
pub(crate) fn lc_ctype() -> String {
    // SAFETY: a NULL locale only queries the current LC_CTYPE setting without changing it.
    let raw = unsafe { setlocale(LC_CTYPE, core::ptr::null()) };
    if raw.is_null() {
        return String::from("C");
    }
    // SAFETY: setlocale returned a NUL-terminated string owned by the C library, copied out at once.
    unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}
