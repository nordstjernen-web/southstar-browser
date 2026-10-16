//! Southstar — the C ABI of element state: the pseudo-class test css.c's matcher delegates, the target fragment, visited links and the document language, with the GLib Unicode, regex and URL calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::GBoolean;

const RTL_SCRIPTS: [c_int; 7] = [19, 2, 34, 37, 66, 90, 95];

unsafe extern "C" {
    fn g_unichar_get_script(c: u32) -> c_int;
    fn g_unichar_isalpha(c: u32) -> GBoolean;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
    fn g_clear_error(error: *mut *mut c_void);
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

fn nul_free_or_empty(text: &[u8]) -> &[u8] {
    if text.contains(&0) { b"" } else { text }
}

pub(crate) fn unichar_is_rtl_script(c: u32) -> bool {
    RTL_SCRIPTS.contains(&unsafe { g_unichar_get_script(c) })
}

pub(crate) fn unichar_is_alpha(c: u32) -> bool {
    unsafe { g_unichar_isalpha(c) != 0 }
}

pub(crate) fn url_resolve(base: &[u8], href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(nul_free_or_empty(base)), nul_free_or_empty(href))
}

pub(crate) fn url_is_valid_absolute(url: &CStr) -> bool {
    southstar_url::bytes::is_valid_absolute(url.to_bytes())
}

pub(crate) fn regex_matches_whole(pattern: Option<&[u8]>, value: &[u8]) -> bool {
    let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    let anchored = c_string(&[b"^(?:".as_slice(), pattern, b")$"].concat());
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(anchored.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        unsafe { g_clear_error(&mut error) };
        return true;
    }
    let value = c_string(value);
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    matched
}
