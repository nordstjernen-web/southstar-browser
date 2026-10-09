//! Southstar — the C ABI of the ported canvas sections, as declared in src/js_internal.h, and the GLib and CSS calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;

unsafe extern "C" {
    fn g_ascii_formatd(
        buffer: *mut c_char,
        buf_len: c_int,
        format: *const c_char,
        d: c_double,
    ) -> *mut c_char;
    fn ns_css_parse_color(
        s: *const c_char,
        r: *mut u8,
        g: *mut u8,
        b: *mut u8,
        a: *mut u8,
    ) -> c_int;
    fn ns_css_font_shorthand_canonical(css: *const c_char) -> *mut c_char;
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

pub(crate) fn strtod_prefix(text: &[u8]) -> (f64, usize) {
    glib::ascii_strtod_prefix(text)
}

pub(crate) fn strtod(text: &[u8]) -> f64 {
    glib::ascii_strtod(text)
}

pub(crate) fn format_g(value: f64) -> Vec<u8> {
    let mut buffer = [0 as c_char; 39];
    unsafe {
        g_ascii_formatd(
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            c"%g".as_ptr(),
            value,
        );
        CStr::from_ptr(buffer.as_ptr()).to_bytes().to_vec()
    }
}

pub(crate) fn css_parse_color(s: &[u8]) -> Option<[u8; 4]> {
    let s = CString::new(s).ok()?;
    let mut rgba = [0u8; 4];
    let [r, g, b, a] = &mut rgba;
    let ok = unsafe { ns_css_parse_color(s.as_ptr(), r, g, b, a) };
    (ok != 0).then_some(rgba)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_parse_color(
    s: *const c_char,
    r: *mut c_double,
    g: *mut c_double,
    b: *mut c_double,
    a: *mut c_double,
) -> c_int {
    let Some(rgba) = (unsafe { text(s) }).and_then(crate::color::parse) else {
        return 0;
    };
    unsafe {
        *r = rgba[0];
        *g = rgba[1];
        *b = rgba[2];
        *a = rgba[3];
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_color_string(css: *const c_char) -> *mut c_char {
    match unsafe { text(css) }.and_then(crate::color::to_string) {
        Some(out) => glib::strdup(&out),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_filter_valid(s: *const c_char) -> c_int {
    c_int::from(unsafe { text(s) }.is_some_and(crate::validate::filter_valid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_length_valid(s: *const c_char) -> c_int {
    c_int::from(unsafe { text(s) }.is_some_and(crate::validate::length_valid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_font_string(css: *const c_char) -> *mut c_char {
    let Some(canon) = (unsafe { glib::GStr::take(ns_css_font_shorthand_canonical(css)) }) else {
        return ptr::null_mut();
    };
    glib::strdup(&crate::font::canonical(canon.to_bytes()))
}
