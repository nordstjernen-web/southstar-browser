//! Southstar — the C ABI of the shared network helpers, and the GLib and locale calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};

unsafe extern "C" {
    fn g_uri_escape_string(
        unescaped: *const c_char,
        reserved_chars_allowed: *const c_char,
        allow_utf8: GBoolean,
    ) -> *mut c_char;
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut c_void,
    ) -> *mut c_char;
}

const FILE_TEST_EXISTS: glib::GFileTest = 1 << 4;
const FILE_TEST_IS_DIR: glib::GFileTest = 1 << 2;

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Option<Vec<u8>> {
    let bytes = unsafe { glib::bytes(p) }?.to_vec();
    unsafe { glib::g_free(p.cast()) };
    Some(bytes)
}

pub(crate) fn configured_search_engine() -> Option<Vec<u8>> {
    let config = southstar_config::get()?;
    unsafe { glib::bytes(config.search_engine) }.map(<[u8]>::to_vec)
}

pub(crate) fn uri_escape(text: &[u8]) -> Vec<u8> {
    let text = cstring(text);
    unsafe { take(g_uri_escape_string(text.as_ptr(), ptr::null(), 1)) }.unwrap_or_default()
}

fn file_test(path: &[u8], test: glib::GFileTest) -> bool {
    let path = cstring(path);
    unsafe { glib::g_file_test(path.as_ptr(), test) != 0 }
}

pub(crate) fn file_exists(path: &[u8]) -> bool {
    file_test(path, FILE_TEST_EXISTS)
}

pub(crate) fn is_dir(path: &[u8]) -> bool {
    file_test(path, FILE_TEST_IS_DIR)
}

pub(crate) fn canonicalize_filename(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstring(path);
    unsafe { take(g_canonicalize_filename(path.as_ptr(), ptr::null())) }
}

pub(crate) fn filename_to_uri(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstring(path);
    unsafe {
        take(g_filename_to_uri(
            path.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
        ))
    }
}

#[cfg(windows)]
fn windows_locale() -> Option<Vec<u8>> {
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, length: core::ffi::c_int) -> core::ffi::c_int;
    }
    let mut name = [0u16; LOCALE_NAME_MAX_LENGTH];
    if unsafe {
        GetUserDefaultLocaleName(
            name.as_mut_ptr(),
            LOCALE_NAME_MAX_LENGTH as core::ffi::c_int,
        )
    } <= 0
    {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    String::from_utf16(&name[..end])
        .ok()
        .map(String::into_bytes)
        .filter(|locale| !locale.is_empty())
}

#[cfg(not(windows))]
fn windows_locale() -> Option<Vec<u8>> {
    None
}

fn language_names() -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    let list = unsafe { glib::g_get_language_names() };
    if list.is_null() {
        return names;
    }
    let mut i = 0;
    loop {
        let name = unsafe { *list.add(i) };
        if name.is_null() {
            break;
        }
        names.push(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec());
        i += 1;
    }
    names
}

fn default_accept_language_c() -> &'static CStr {
    static CACHED: OnceLock<CString> = OnceLock::new();
    CACHED.get_or_init(|| {
        let value = match windows_locale() {
            Some(locale) => crate::accept_language_for_windows_locale(&locale),
            None => crate::accept_language_from_language_names(&language_names())
                .unwrap_or_else(|| b"en-US,en;q=0.9".to_vec()),
        };
        cstring(&value)
    })
}

pub fn effective_accept_language() -> Vec<u8> {
    southstar_config::get()
        .and_then(|c| unsafe { glib::bytes(c.accept_language) })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(default_accept_language)
        .to_vec()
}

pub fn effective_navigator_languages() -> Vec<Vec<u8>> {
    crate::navigator_languages(&effective_accept_language())
}

pub fn default_accept_language() -> &'static [u8] {
    default_accept_language_c().to_bytes()
}
