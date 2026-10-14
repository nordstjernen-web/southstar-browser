//! Southstar — the C ABI of display values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_glib as glib;

use crate::display::{self, Display};

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_display_serialize(d: Display) -> *mut c_char {
    glib::strdup(&display::serialize(d))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_from_keyword(canonical: *const c_char) -> Display {
    display::from_keyword(unsafe { bytes(canonical) })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_display_blockified(d: Display) -> Display {
    display::blockified(d)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_canonical(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(display::canonical))
}
