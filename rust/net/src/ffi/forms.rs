//! Southstar — form encoding over GLib: urlencoded pairs in the submission charset, and multipart boundaries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{self as glib, GError};

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
}

fn converted(text: &[u8]) -> Option<Vec<u8>> {
    let charset = std::ffi::CString::new(forms::submission_charset()?).ok()?;
    // SAFETY: text is len readable bytes, charset and "UTF-8" are NUL-terminated and the out-parameters may be null.
    let out = unsafe {
        g_convert(
            text.as_ptr().cast(),
            text.len() as isize,
            charset.as_ptr(),
            c"UTF-8".as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    // SAFETY: g_convert returned null or a new NUL-terminated string the caller owns.
    unsafe { glib::GStr::take(out) }.map(|s| s.to_bytes().to_vec())
}

fn urlencoded_append(out: &mut Vec<u8>, text: &[u8]) {
    match converted(text) {
        Some(text) => forms::urlencode(out, &text),
        None => forms::urlencode(out, text),
    }
}

pub fn urlencoded_append_pair(
    out: &mut Vec<u8>,
    first: &mut bool,
    name: &[u8],
    value: Option<&[u8]>,
) {
    if !*first {
        out.push(b'&');
    }
    *first = false;
    urlencoded_append(out, name);
    out.push(b'=');
    if let Some(value) = value {
        urlencoded_append(out, value);
    }
}

pub fn multipart_boundary() -> Vec<u8> {
    let mut words = [0u32; 4];
    let mut bytes = [0u8; 16];
    if southstar_sandbox::csprng_fill(&mut bytes) {
        for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *word = u32::from_ne_bytes(*chunk);
        }
    } else {
        for w in &mut words {
            *w = southstar_config::util::random_u32();
        }
    }
    forms::boundary(words)
}
