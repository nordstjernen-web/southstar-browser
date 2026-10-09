//! Southstar — the C ABI of serializing a CSS identifier and reading one escaped character of CSS text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};

use southstar_glib as glib;

use crate::{content, lex};

unsafe extern "C" {
    fn g_string_append_len(string: *mut c_void, val: *const c_char, len: isize) -> *mut c_void;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ident_serialize(name: *const c_char) -> *mut c_char {
    if name.is_null() {
        return glib::strdup(b"");
    }
    glib::strdup(&lex::ident_serialize(
        unsafe { CStr::from_ptr(name) }.to_bytes(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_append_unescaped(out: *mut c_void, pp: *mut *const c_char) {
    let Some(at) = (unsafe { pp.as_mut() }).filter(|p| !p.is_null()) else {
        return;
    };
    let text = unsafe { CStr::from_ptr(*at) }.to_bytes();
    let mut unescaped = Vec::new();
    let mut pos = 0;
    content::append_unescaped(&mut unescaped, text, &mut pos);
    unsafe {
        g_string_append_len(out, unescaped.as_ptr().cast(), unescaped.len() as isize);
        *at = at.add(pos);
    }
}
