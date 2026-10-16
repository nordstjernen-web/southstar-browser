//! Southstar — the GLib and uchardet calls behind the HTML helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};

unsafe extern "C" {
    fn g_path_get_basename(file_name: *const c_char) -> *mut c_char;
    fn g_utf8_validate(str_: *const c_char, max_len: isize, end: *mut *const c_char) -> GBoolean;
    fn g_convert(
        str_: *const c_char,
        len: isize,
        to_codeset: *const c_char,
        from_codeset: *const c_char,
        bytes_read: *mut usize,
        bytes_written: *mut usize,
        error: *mut *mut c_void,
    ) -> *mut c_char;
    fn uchardet_new() -> *mut c_void;
    fn uchardet_delete(ud: *mut c_void);
    fn uchardet_handle_data(ud: *mut c_void, data: *const c_char, len: usize) -> c_int;
    fn uchardet_data_end(ud: *mut c_void);
    fn uchardet_get_charset(ud: *const c_void) -> *const c_char;
}

fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes)).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Vec<u8> {
    let bytes = unsafe { glib::bytes(p) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(p.cast()) };
    bytes
}

pub(crate) fn ascii_strtod(text: &[u8]) -> f64 {
    glib::ascii_strtod(text)
}

pub(crate) fn path_basename(path: &[u8]) -> Vec<u8> {
    let path = cstring(path);
    unsafe { take(g_path_get_basename(path.as_ptr())) }
}

pub(crate) fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(text).into_owned().into_bytes()
}

pub(crate) fn utf8_validate(text: &[u8]) -> bool {
    unsafe { g_utf8_validate(text.as_ptr().cast(), text.len() as isize, ptr::null_mut()) != 0 }
}

pub(crate) fn convert(text: &[u8], from: &[u8]) -> Option<Vec<u8>> {
    let from = cstring(from);
    let mut written = 0usize;
    let out = unsafe {
        g_convert(
            text.as_ptr().cast(),
            text.len() as isize,
            c"UTF-8".as_ptr(),
            from.as_ptr(),
            ptr::null_mut(),
            &mut written,
            ptr::null_mut(),
        )
    };
    let out = NonNull::new(out)?;
    let bytes = unsafe { glib::slice(out.as_ptr().cast::<u8>(), written) }.to_vec();
    unsafe { glib::g_free(out.as_ptr().cast()) };
    Some(bytes)
}

pub(crate) fn detect_charset(sample: &[u8]) -> Option<Vec<u8>> {
    let detector = unsafe { uchardet_new() };
    if detector.is_null() {
        return None;
    }
    let mut charset = None;
    if unsafe { uchardet_handle_data(detector, sample.as_ptr().cast(), sample.len()) } == 0 {
        unsafe { uchardet_data_end(detector) };
        let name = unsafe { glib::bytes(uchardet_get_charset(detector)) }.unwrap_or_default();
        if !name.is_empty()
            && !name.eq_ignore_ascii_case(b"ASCII")
            && !name.eq_ignore_ascii_case(b"UTF-8")
        {
            charset = Some(name.to_vec());
        }
    }
    unsafe { uchardet_delete(detector) };
    charset
}
