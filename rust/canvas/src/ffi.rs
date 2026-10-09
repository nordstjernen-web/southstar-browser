//! Southstar — the C ABI of the ported canvas sections, as declared in src/js_internal.h, and the GLib and CSS calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;

pub(crate) mod cairo;
pub(crate) mod context;
mod draw;
mod objects;
pub(crate) mod state;
mod style;

pub(crate) use draw::{ctx_state, mark_mutated};

pub(crate) use objects::{
    c, canvas_state_for, computed_color, context_cairo, ctx2d_new, decode_image, drawimage_source,
    element_attr, is_path2d, new_gradient, new_imagedata, new_imagedata_from,
    new_offscreen_canvas_node, new_pattern, ns_pattern_set_transform, path2d_context,
    set_element_attr, with_bitmap, with_hidden,
};

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
    fn cairo_move_to(cr: *mut Cairo, x: c_double, y: c_double);
    fn cairo_line_to(cr: *mut Cairo, x: c_double, y: c_double);
    fn cairo_curve_to(
        cr: *mut Cairo,
        x1: c_double,
        y1: c_double,
        x2: c_double,
        y2: c_double,
        x3: c_double,
        y3: c_double,
    );
    fn cairo_close_path(cr: *mut Cairo);
    fn cairo_save(cr: *mut Cairo);
    fn cairo_restore(cr: *mut Cairo);
    fn cairo_translate(cr: *mut Cairo, tx: c_double, ty: c_double);
    fn cairo_rotate(cr: *mut Cairo, angle: c_double);
    fn cairo_scale(cr: *mut Cairo, sx: c_double, sy: c_double);
    fn cairo_arc(
        cr: *mut Cairo,
        xc: c_double,
        yc: c_double,
        radius: c_double,
        angle1: c_double,
        angle2: c_double,
    );
    fn cairo_arc_negative(
        cr: *mut Cairo,
        xc: c_double,
        yc: c_double,
        radius: c_double,
        angle1: c_double,
        angle2: c_double,
    );
}

#[repr(C)]
pub struct Cairo {
    _private: [u8; 0],
}

struct CairoPath(*mut Cairo);

impl crate::path::PathSink for CairoPath {
    fn move_to(&mut self, x: f64, y: f64) {
        unsafe { cairo_move_to(self.0, x, y) };
    }

    fn line_to(&mut self, x: f64, y: f64) {
        unsafe { cairo_line_to(self.0, x, y) };
    }

    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        unsafe { cairo_curve_to(self.0, x1, y1, x2, y2, x, y) };
    }

    fn close_path(&mut self) {
        unsafe { cairo_close_path(self.0) };
    }

    fn unit_arc(
        &mut self,
        center: (f64, f64),
        phi: f64,
        radii: (f64, f64),
        angles: (f64, f64),
        sweep: bool,
    ) {
        unsafe {
            cairo_save(self.0);
            cairo_translate(self.0, center.0, center.1);
            cairo_rotate(self.0, phi);
            cairo_scale(self.0, radii.0, radii.1);
            if sweep {
                cairo_arc(self.0, 0.0, 0.0, 1.0, angles.0, angles.1);
            } else {
                cairo_arc_negative(self.0, 0.0, 0.0, 1.0, angles.0, angles.1);
            }
            cairo_restore(self.0);
        }
    }
}

pub(crate) fn font_string(css: &[u8]) -> Option<Vec<u8>> {
    let css = CString::new(css).ok()?;
    let canon = unsafe { glib::GStr::take(ns_css_font_shorthand_canonical(css.as_ptr())) }?;
    Some(crate::font::canonical(canon.to_bytes()))
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_path2d_parse_svg(cr: *mut Cairo, d: *const c_char) {
    if cr.is_null() {
        return;
    }
    if let Some(d) = unsafe { text(d) } {
        crate::path::parse(&mut CairoPath(cr), d);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_blur_argb(
    data: *mut u8,
    w: c_int,
    h: c_int,
    stride: c_int,
    radius: c_int,
) {
    if data.is_null() || w <= 0 || h <= 0 || stride <= 0 {
        return;
    }
    let len = stride as usize * h as usize;
    let pixels = unsafe { core::slice::from_raw_parts_mut(data, len) };
    crate::raster::box_blur(pixels, w, h, stride, radius);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_parse_composite(s: *const c_char) -> c_int {
    crate::raster::composite_operator(unsafe { text(s) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_parse_fill_rule(s: *const c_char) -> c_int {
    crate::raster::fill_rule(unsafe { text(s) })
}
