//! Southstar — the C ABI of the style sheet text passes css.c's rule parser calls: nesting flattened, an invalid qualified rule skipped, and the container-unit check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_glib::{self as glib, GBoolean};

use crate::nesting;

unsafe fn text_of<'a>(text: *const c_char, len: isize) -> Option<&'a [u8]> {
    if text.is_null() {
        return None;
    }
    let len =
        usize::try_from(len).unwrap_or_else(|_| unsafe { CStr::from_ptr(text) }.to_bytes().len());
    Some(unsafe { core::slice::from_raw_parts(text.cast(), len) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_flatten_nesting(text: *const c_char, len: isize) -> *mut c_char {
    match unsafe { text_of(text, len) } {
        Some(text) => glib::strdup(&nesting::flatten(text)),
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_text_has_container_units(
    text: *const c_char,
    len: isize,
) -> GBoolean {
    glib::boolean(unsafe { text_of(text, len) }.is_some_and(nesting::has_container_units))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_skip_invalid_qualified_rule(
    p: *const c_char,
    end: *const c_char,
    nested: GBoolean,
) -> *const c_char {
    if p.is_null() || end <= p {
        return end;
    }
    let len = unsafe { end.offset_from(p) } as usize;
    let s = unsafe { core::slice::from_raw_parts(p.cast::<u8>(), len) };
    unsafe { p.add(nesting::skip_invalid_qualified_rule(s, 0, len, nested != 0)) }
}
