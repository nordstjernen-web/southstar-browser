//! Southstar — the C ABI of media queries, as declared in src/css.h, and the engine state they read.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use southstar_glib::{self as glib, FALSE, GBoolean, TRUE};

const COLOR_SCHEME_DARK: c_int = 1;
const REDUCED_MOTION_REDUCE: c_int = 1;

unsafe extern "C" {
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_css_stylesheet_cache_drop();
    fn ns_css_get_color_scheme() -> c_int;
    fn ns_css_get_reduced_motion() -> c_int;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
}

pub(crate) fn css_viewport_w() -> f64 {
    unsafe { ns_css_viewport_w() }
}

pub(crate) fn css_viewport_h() -> f64 {
    unsafe { ns_css_viewport_h() }
}

pub(crate) fn stylesheet_cache_drop() {
    unsafe { ns_css_stylesheet_cache_drop() };
}

pub(crate) fn prefers_dark() -> bool {
    unsafe { ns_css_get_color_scheme() == COLOR_SCHEME_DARK }
}

pub(crate) fn prefers_reduced_motion() -> bool {
    unsafe { ns_css_get_reduced_motion() == REDUCED_MOTION_REDUCE }
}

pub(crate) fn ascii_strtod(text: &[u8]) -> (f64, usize) {
    glib::ascii_strtod_prefix(text)
}

pub(crate) fn format_g(value: f64) -> Vec<u8> {
    unsafe {
        let text = g_strdup_printf(c"%g".as_ptr(), value);
        let bytes = CStr::from_ptr(text).to_bytes().to_vec();
        glib::g_free(text.cast());
        bytes
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_device_size(w: f64, h: f64) {
    crate::set_device_size(w, h);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_device_pixel_ratio(dppx: f64) {
    crate::set_device_pixel_ratio(dppx);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_device_pixel_ratio() -> f64 {
    crate::device_pixel_ratio()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_media_viewport_push(w: f64, h: f64) {
    crate::viewport_push(w, h);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_media_viewport_pop() {
    crate::viewport_pop();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_media_viewport_current_w() -> f64 {
    crate::viewport_w()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_media_viewport_current_h() -> f64 {
    crate::viewport_h()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_print_media(printing: GBoolean) {
    crate::set_print_media(printing != FALSE);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_print_media() -> GBoolean {
    if crate::print_media() { TRUE } else { FALSE }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_media_query_matches(query: *const c_char) -> GBoolean {
    match unsafe { glib::bytes(query) } {
        Some(query) => glib::boolean(crate::query_matches(query)),
        None => TRUE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_media_list_serialize(query: *const c_char) -> *mut c_char {
    let query = unsafe { glib::bytes(query) }.unwrap_or_default();
    glib::strdup(&crate::list_serialize(query))
}
