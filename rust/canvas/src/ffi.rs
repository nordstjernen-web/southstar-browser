//! Southstar — the unsafe edge of the canvas: its C ABI, the engine calls it makes, and the GLib, CSS and cairo calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_glib as glib;

pub(crate) mod cairo;
pub(crate) mod context;
mod draw;
mod exports;
mod imports;
mod objects;
mod source;
pub(crate) mod state;
pub(crate) mod text_layout;

pub(crate) use draw::{ctx_state, mark_mutated};
pub(crate) use source::drawimage_source;

pub(crate) use objects::{
    canvas_realm, canvas_state_for, computed_color, context_cairo, ctx2d_new, decode_image,
    drawimage_source_surface, element_attr, in_realm, is_image_bitmap, is_path2d, new_gradient,
    new_imagedata, new_imagedata_from, new_in_realm, new_offscreen_canvas_node, new_pattern,
    new_textmetrics, path2d_context, set_element_attr, with_bitmap, with_hidden,
};

pub(crate) fn font_string(css: &[u8]) -> Option<Vec<u8>> {
    if css.contains(&0) {
        return None;
    }
    let canon = southstar_css::font_shorthand_canonical(css)?;
    Some(crate::font::canonical(&canon))
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
    southstar_css::parse_color(&CString::new(s).ok()?)
}
