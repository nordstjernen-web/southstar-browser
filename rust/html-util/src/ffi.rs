//! Southstar — the C ABI of the HTML helpers, as declared in src/html.h, and the GLib and uchardet calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};

use crate::Decoded;

#[repr(C)]
pub struct GString {
    str_: *mut c_char,
    len: usize,
    allocated_len: usize,
}

unsafe extern "C" {
    fn g_string_append_len(string: *mut GString, val: *const c_char, len: isize) -> *mut GString;
    fn g_path_get_basename(file_name: *const c_char) -> *mut c_char;
    fn g_utf8_make_valid(str_: *const c_char, len: isize) -> *mut c_char;
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

pub struct GlibString(NonNull<c_char>);

impl GlibString {
    fn into_raw(self) -> *mut c_char {
        let p = self.0.as_ptr();
        core::mem::forget(self);
        p
    }
}

impl Drop for GlibString {
    fn drop(&mut self) {
        unsafe { glib::g_free(self.0.as_ptr().cast()) };
    }
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

pub(crate) fn utf8_make_valid(text: &[u8]) -> Decoded {
    let out = unsafe { g_utf8_make_valid(text.as_ptr().cast(), text.len() as isize) };
    NonNull::new(out).map_or(Decoded::Bytes(Vec::new()), |out| {
        Decoded::Glib(GlibString(out))
    })
}

pub(crate) fn utf8_validate(text: &[u8]) -> bool {
    unsafe { g_utf8_validate(text.as_ptr().cast(), text.len() as isize, ptr::null_mut()) != 0 }
}

pub(crate) fn convert(text: &[u8], from: &[u8]) -> Option<GlibString> {
    let from = cstring(from);
    let out = unsafe {
        g_convert(
            text.as_ptr().cast(),
            text.len() as isize,
            c"UTF-8".as_ptr(),
            from.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    NonNull::new(out).map(GlibString)
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

unsafe fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_float(s: *const c_char, out: *mut f64) -> GBoolean {
    let Some(s) = (unsafe { text(s) }) else {
        return 0;
    };
    match crate::parse_float(s) {
        Some(v) => {
            unsafe { *out = v };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_is_void(tag: *const c_char) -> GBoolean {
    glib::boolean(unsafe { text(tag) }.is_some_and(crate::is_void))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_is_raw_text(tag: *const c_char) -> GBoolean {
    glib::boolean(unsafe { text(tag) }.is_some_and(crate::is_raw_text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_escape_append(
    out: *mut GString,
    s: *const c_char,
    escape_quotes: GBoolean,
) {
    let mut escaped = Vec::new();
    crate::escape_append(
        &mut escaped,
        unsafe { text(s) }.unwrap_or_default(),
        escape_quotes != 0,
    );
    unsafe { g_string_append_len(out, escaped.as_ptr().cast(), escaped.len() as isize) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_escape_text(s: *const c_char) -> *mut c_char {
    glib::strdup(&crate::escape_text(unsafe { text(s) }.unwrap_or_default()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_image_document(url: *const c_char) -> *mut c_char {
    glib::strdup(&crate::image_document(
        unsafe { text(url) }.unwrap_or_default(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_json_document(
    url: *const c_char,
    json: *const c_char,
    len: usize,
) -> *mut c_char {
    if json.is_null() {
        return ptr::null_mut();
    }
    let body = unsafe { glib::slice(json.cast(), len) };
    crate::json_document(unsafe { text(url) }, body)
        .map_or(ptr::null_mut(), |html| glib::strdup(&html))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_xml_document(
    url: *const c_char,
    xml: *const c_char,
    len: usize,
) -> *mut c_char {
    if xml.is_null() {
        return ptr::null_mut();
    }
    let body = unsafe { glib::slice(xml.cast(), len) };
    glib::strdup(&crate::xml_document(unsafe { text(url) }, body))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_declared_charset(
    body: *const c_char,
    len: usize,
    content_type: *const c_char,
) -> *mut c_char {
    let body = (!body.is_null()).then(|| unsafe { glib::slice(body.cast(), len) });
    crate::declared_charset(body, unsafe { text(content_type) })
        .map_or(ptr::null_mut(), |name| glib::strdup(name.as_bytes()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_decode_body_full(
    body: *const c_char,
    len: usize,
    content_type: *const c_char,
    charset_out: *mut *mut c_char,
) -> *mut c_char {
    if !charset_out.is_null() {
        unsafe { *charset_out = ptr::null_mut() };
    }
    let body = if body.is_null() {
        &[][..]
    } else {
        unsafe { glib::slice(body.cast(), len) }
    };
    let (decoded, charset) = crate::decode_body(body, unsafe { text(content_type) });
    if let (Some(charset), false) = (charset, charset_out.is_null()) {
        unsafe { *charset_out = glib::strdup(&charset) };
    }
    match decoded {
        Decoded::Glib(out) => out.into_raw(),
        Decoded::Bytes(bytes) => glib::strdup(&bytes),
    }
}
