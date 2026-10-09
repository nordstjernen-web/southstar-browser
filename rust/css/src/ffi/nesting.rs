//! Southstar — the C ABI of the container-unit check over style sheet text.
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
pub unsafe extern "C" fn ns_css_text_has_container_units(
    text: *const c_char,
    len: isize,
) -> GBoolean {
    glib::boolean(unsafe { text_of(text, len) }.is_some_and(nesting::has_container_units))
}
