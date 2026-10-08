//! Southstar — the C ABI of colours as text, positions, gradients, image-set(), content and unicode-range.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::value::{self, NsCssValue};
use crate::color::color_text;
use crate::content;
use crate::gradient::{self, Gradient};
use crate::image;
use crate::position;

unsafe extern "C" {
    fn g_string_append(string: *mut c_void, val: *const c_char) -> *mut c_void;
}

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
pub extern "C" fn ns_css_color_text(r: u8, g: u8, b: u8, a: u8) -> *mut c_char {
    glib::strdup(&color_text([r, g, b, a]))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_append_color(s: *mut c_void, r: u8, g: u8, b: u8, a: u8) {
    if s.is_null() {
        return;
    }
    let text = glib::strdup(&color_text([r, g, b, a]));
    unsafe {
        g_string_append(s, text);
        glib::g_free(text.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_wide_keyword_or_default(item: *const c_char) -> GBoolean {
    test(item, content::wide_keyword_or_default)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_position_is_h_edge(t: *const c_char) -> GBoolean {
    test(t, position::is_h_edge)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_position_is_v_edge(t: *const c_char) -> GBoolean {
    test(t, position::is_v_edge)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_position_is_keyword(t: *const c_char) -> GBoolean {
    test(t, position::is_keyword)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_position_split(
    text: *const c_char,
    out_x: *mut *mut c_char,
    out_y: *mut *mut c_char,
) {
    let (x, y) = position::split(unsafe { bytes(text) }.unwrap_or_default());
    unsafe {
        *out_x = glib::strdup(&x);
        *out_y = glib::strdup(&y);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_position_canonical_ex(
    text: *const c_char,
    expand_single: GBoolean,
    allow_three: GBoolean,
) -> *mut c_char {
    owned(
        unsafe { bytes(text) }
            .and_then(|text| position::canonical_ex(text, expand_single != 0, allow_three != 0)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_math_text_has_unit(
    t: *const c_char,
    units: *const *const c_char,
    n_units: usize,
) -> GBoolean {
    let units: Vec<&[u8]> = if units.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(units, n_units) }
            .iter()
            .filter_map(|&unit| unsafe { bytes(unit) })
            .collect()
    };
    test(t, |t| gradient::math_text_has_unit(t, &units))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_angle_deg(s: *const c_char) -> c_double {
    unsafe { bytes(s) }.map_or(0.0, gradient::parse_angle_deg)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_text_starts_gradient(t: *const c_char) -> GBoolean {
    test(t, gradient::text_starts_gradient)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_text_starts_image_set(t: *const c_char) -> GBoolean {
    test(t, image::text_starts_image_set)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_gradient(t: *const c_char) -> *mut NsCssValue {
    let parsed = (!t.is_null())
        .then(|| unsafe { CStr::from_ptr(t) })
        .and_then(gradient::parse_value);
    parsed.map_or(ptr::null_mut(), |gr| value::new_gradient(&gr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_gradient_serialize(gr: *const Gradient) -> *mut c_char {
    match unsafe { gr.as_ref() } {
        Some(gr) => glib::strdup(&gradient::serialize_computed(gr)),
        None => glib::strdup(b""),
    }
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
pub unsafe extern "C" fn ns_css_image_set_canonical(
    text: *const c_char,
    computed: GBoolean,
) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(|text| image::image_set_canonical(text, computed != 0)))
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
pub unsafe extern "C" fn ns_css_content_symbols_canonical(args: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(args) }.and_then(content::symbols_canonical))
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_unescape_url(u: *const c_char, len: usize) -> *mut c_char {
    let text = if u.is_null() {
        &[][..]
    } else {
        unsafe { glib::slice(u.cast(), len) }
    };
    glib::strdup(&image::unescape_url(text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_pick_image_set_url(t: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(t) }.and_then(image::pick_image_set_url))
}
