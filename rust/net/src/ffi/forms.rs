//! Southstar — the C ABI of form encoding: urlencoded pairs appended to a GString in the submission charset, multipart field quoting and boundaries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean, GError};

use super::sinks::{GString, gstring_append};
use crate::forms;

unsafe extern "C" {
    fn g_convert(
        text: *const c_char,
        len: isize,
        to_codeset: *const c_char,
        from_codeset: *const c_char,
        bytes_read: *mut usize,
        bytes_written: *mut usize,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn ns_security_csprng_fill(buf: *mut c_void, len: usize) -> GBoolean;
    fn g_random_int() -> u32;
}

fn append(out: *mut GString, bytes: &[u8]) {
    gstring_append(out, bytes);
}

fn converted(text: *const c_char) -> Option<Vec<u8>> {
    let charset = std::ffi::CString::new(forms::submission_charset()?).ok()?;
    let out = unsafe {
        g_convert(
            text,
            -1,
            charset.as_ptr(),
            c"UTF-8".as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    unsafe { glib::GStr::take(out) }.map(|s| s.to_bytes().to_vec())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_set_submission_charset(charset: *const c_char) {
    forms::set_submission_charset(unsafe { glib::bytes(charset) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_urlencoded_append(out: *mut GString, s: *const c_char) {
    if out.is_null() || s.is_null() {
        return;
    }
    let text =
        converted(s).unwrap_or_else(|| unsafe { glib::bytes(s) }.unwrap_or_default().to_vec());
    let mut encoded = Vec::with_capacity(text.len());
    forms::urlencode(&mut encoded, &text);
    append(out, &encoded);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_urlencoded_append_pair(
    out: *mut GString,
    first: *mut GBoolean,
    name: *const c_char,
    value: *const c_char,
) {
    if out.is_null() || first.is_null() || name.is_null() {
        return;
    }
    unsafe {
        if *first == 0 {
            append(out, b"&");
        }
        *first = 0;
        ns_form_urlencoded_append(out, name);
        append(out, b"=");
        ns_form_urlencoded_append(out, value);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_multipart_quote_field(out: *mut GString, s: *const c_char) {
    let (false, Some(text)) = (out.is_null(), unsafe { glib::bytes(s) }) else {
        return;
    };
    let mut quoted = Vec::with_capacity(text.len());
    forms::quote_field(&mut quoted, text);
    append(out, &quoted);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_multipart_boundary() -> *mut c_char {
    let mut words = [0u32; 4];
    let filled = unsafe {
        ns_security_csprng_fill(words.as_mut_ptr().cast(), core::mem::size_of_val(&words))
    };
    if filled == 0 {
        for w in &mut words {
            *w = unsafe { g_random_int() };
        }
    }
    glib::strdup(&forms::boundary(words))
}
