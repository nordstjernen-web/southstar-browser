//! Southstar — the C ABI of the border-image longhands, the shorthand's tokens, and the parameters painting reads from a computed style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean, GPtrArray};

use super::value::{self, NsCssValue};
use crate::border_image::{self, Params};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn keyword(text: Option<Vec<u8>>) -> *mut NsCssValue {
    text.map_or(ptr::null_mut(), |text| value::new_keyword(&text))
}

unsafe extern "C" fn free_string(p: *mut c_void) {
    unsafe { glib::g_free(p) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_border_image_slice(t: *const c_char) -> *mut NsCssValue {
    keyword(unsafe { bytes(t) }.and_then(border_image::slice_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_border_image_width(t: *const c_char) -> *mut NsCssValue {
    keyword(unsafe { bytes(t) }.and_then(|t| border_image::quad_canonical(t, true, true)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_border_image_outset(t: *const c_char) -> *mut NsCssValue {
    keyword(unsafe { bytes(t) }.and_then(|t| border_image::quad_canonical(t, false, false)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_border_image_repeat(t: *const c_char) -> *mut NsCssValue {
    keyword(unsafe { bytes(t) }.and_then(border_image::repeat_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_image_length_serialize(
    token: *const c_char,
    allow_auto: GBoolean,
    allow_percent: GBoolean,
) -> *mut c_char {
    unsafe { bytes(token) }
        .and_then(|t| border_image::length_serialize(t, allow_auto != 0, allow_percent != 0))
        .map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_image_tile_keyword(token: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(token) }.is_some_and(border_image::tile_keyword))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_image_tokens(text: *const c_char) -> *mut GPtrArray {
    let array = unsafe { glib::g_ptr_array_new_with_free_func(Some(free_string)) };
    for tok in border_image::shorthand_tokens(unsafe { bytes(text) }.unwrap_or_default()) {
        unsafe { glib::g_ptr_array_add(array, glib::strdup(&tok).cast()) };
    }
    array
}

fn prop_ids() -> &'static [c_int; 4] {
    static IDS: OnceLock<[c_int; 4]> = OnceLock::new();
    IDS.get_or_init(|| {
        [
            c"border-image-slice",
            c"border-image-width",
            c"border-image-outset",
            c"border-image-repeat",
        ]
        .map(|name| unsafe { ns_css_prop_id(name.as_ptr()) })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_image_params(style: *const c_void, out: *mut Params) {
    if out.is_null() {
        return;
    }
    let ids = prop_ids();
    let text = |i: usize, fallback: &'static [u8]| -> &[u8] {
        if style.is_null() {
            return fallback;
        }
        unsafe { value::keyword_of(value::style_value(style, ids[i])) }
            .filter(|t| !t.is_empty())
            .unwrap_or(fallback)
    };
    let params = border_image::params(
        text(0, b"100%"),
        text(1, b"1"),
        text(2, b"0"),
        text(3, b"stretch"),
    );
    unsafe { *out = params };
}
