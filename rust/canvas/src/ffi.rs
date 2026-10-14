//! Southstar — the unsafe edge of the canvas: its C ABI, the engine calls it makes, and the GLib, CSS and cairo calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_glib as glib;

pub(crate) mod cairo;
pub(crate) mod context;
mod draw;
mod exports;
mod imports;
mod objects;
pub(crate) mod pango;
mod source;
pub(crate) mod state;

pub(crate) use draw::{ctx_state, mark_mutated};
pub(crate) use source::drawimage_source;

pub(crate) use objects::{
    canvas_realm, canvas_state_for, computed_color, context_cairo, ctx2d_new, decode_image,
    drawimage_source_surface, element_attr, in_realm, is_image_bitmap, is_path2d, new_gradient,
    new_imagedata, new_imagedata_from, new_in_realm, new_offscreen_canvas_node, new_pattern,
    new_textmetrics, path2d_context, set_element_attr, with_bitmap, with_hidden,
};

unsafe extern "C" {
    fn ns_css_parse_color(
        s: *const c_char,
        r: *mut u8,
        g: *mut u8,
        b: *mut u8,
        a: *mut u8,
    ) -> c_int;
    fn ns_css_font_shorthand_canonical(css: *const c_char) -> *mut c_char;
}

pub(crate) fn font_string(css: &[u8]) -> Option<Vec<u8>> {
    let css = CString::new(css).ok()?;
    let canon = unsafe { glib::GStr::take(ns_css_font_shorthand_canonical(css.as_ptr())) }?;
    Some(crate::font::canonical(canon.to_bytes()))
}

pub(crate) fn state_font(st: &state::CanvasState) -> &[u8] {
    unsafe { text(st.font) }.unwrap_or_default()
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

pub(crate) fn css_parse_color(s: &[u8]) -> Option<[u8; 4]> {
    let s = CString::new(s).ok()?;
    let mut rgba = [0u8; 4];
    let [r, g, b, a] = &mut rgba;
    let ok = unsafe { ns_css_parse_color(s.as_ptr(), r, g, b, a) };
    (ok != 0).then_some(rgba)
}
