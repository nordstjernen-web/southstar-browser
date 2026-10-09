//! Southstar — the C ABI of gradient geometry, image values, content and unicode-range.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::content;
use crate::gradient::{self, Gradient};
use crate::image;

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

fn test(s: *const c_char, f: impl FnOnce(&[u8]) -> bool) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(f))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_gradient_angle(
    gr: *const Gradient,
    w: c_double,
    h: c_double,
) -> c_double {
    unsafe { gr.as_ref() }.map_or(0.0, |gr| gradient::angle(gr, w, h))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_gradient_radii(
    gr: *const Gradient,
    w: c_double,
    h: c_double,
    cx: c_double,
    cy: c_double,
    rx: *mut c_double,
    ry: *mut c_double,
) {
    let Some(gr) = (unsafe { gr.as_ref() }) else {
        return;
    };
    let (x, y) = gradient::radii(gr, w, h, cx, cy);
    unsafe {
        *rx = x;
        *ry = y;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_image_value_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(image::image_value_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_unicode_range_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(image::unicode_range_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_content_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(content::content_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_content_ident_valid(s: *const c_char) -> GBoolean {
    test(s, content::ident_valid)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_quoted_end(u: *const c_char, quote: c_char) -> *const c_char {
    match unsafe { bytes(u) }.and_then(|text| image::quoted_end(text, quote as u8)) {
        Some(end) => unsafe { u.add(end) },
        None => ptr::null(),
    }
}
