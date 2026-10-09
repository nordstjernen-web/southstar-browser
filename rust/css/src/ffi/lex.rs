//! Southstar — the C ABI of reading CSS identifiers and strings and of serializing an identifier.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::slice;

use southstar_glib as glib;

use crate::lex;

unsafe fn read_with(
    pp: *mut *const c_char,
    end: *const c_char,
    read: fn(&[u8], &mut usize, usize) -> Vec<u8>,
) -> *mut c_char {
    if pp.is_null() || unsafe { *pp }.is_null() {
        return glib::strdup(b"");
    }
    let start = unsafe { *pp };
    let len = usize::try_from(unsafe { end.offset_from(start) }).unwrap_or(0);
    let s = unsafe { slice::from_raw_parts(start.cast::<u8>(), len) };
    let mut pos = 0;
    let out = read(s, &mut pos, len);
    unsafe { *pp = start.add(pos) };
    glib::strdup(&out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_read_ident(
    pp: *mut *const c_char,
    end: *const c_char,
) -> *mut c_char {
    unsafe { read_with(pp, end, lex::read_ident) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_read_string(
    pp: *mut *const c_char,
    end: *const c_char,
) -> *mut c_char {
    unsafe { read_with(pp, end, lex::read_string) }
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
