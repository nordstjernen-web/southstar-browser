//! Southstar — the C ABI of MathML layout, as declared in src/mathml.h, over Cairo, ns-pango token layouts and GLib's Unicode tables.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;

pub const PANGO_SCALE: f64 = 1024.0;
const PANGO_STYLE_NORMAL: c_int = 0;
const PANGO_STYLE_ITALIC: c_int = 2;

#[repr(C)]
struct PangoRectangle {
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
}

unsafe extern "C" {
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_line_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_stroke(cr: *mut c_void);
    fn cairo_set_line_width(cr: *mut c_void, width: f64);
    fn cairo_set_source_rgba(cr: *mut c_void, red: f64, green: f64, blue: f64, alpha: f64);
    fn ns_paint_text_context() -> *mut c_void;
    fn ns_paint_pango_font_size(size_px: f64) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_new")]
    fn pango_layout_new(context: *mut c_void) -> *mut c_void;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_description_new")]
    fn pango_font_description_new() -> *mut c_void;
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_font_description_set_family"
    )]
    fn pango_font_description_set_family(desc: *mut c_void, family: *const c_char);
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_font_description_set_absolute_size"
    )]
    fn pango_font_description_set_absolute_size(desc: *mut c_void, size: f64);
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_font_description_set_style"
    )]
    fn pango_font_description_set_style(desc: *mut c_void, style: c_int);
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_layout_set_font_description"
    )]
    fn pango_layout_set_font_description(layout: *mut c_void, desc: *const c_void);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_description_free")]
    fn pango_font_description_free(desc: *mut c_void);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_set_text")]
    fn pango_layout_set_text(layout: *mut c_void, text: *const c_char, length: c_int);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_get_baseline")]
    fn pango_layout_get_baseline(layout: *mut c_void) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_get_extents")]
    fn pango_layout_get_extents(
        layout: *mut c_void,
        ink: *mut PangoRectangle,
        logical: *mut PangoRectangle,
    );
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_cairo_show_layout")]
    fn pango_cairo_show_layout(cr: *mut c_void, layout: *mut c_void);
    fn g_object_unref(object: *mut c_void);
    fn g_utf8_strlen(p: *const c_char, max: isize) -> c_long;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_isalpha(c: u32) -> GBoolean;
}

pub struct Canvas(*mut c_void);

impl Canvas {
    pub fn save(&self) {
        unsafe { cairo_save(self.0) };
    }

    pub fn restore(&self) {
        unsafe { cairo_restore(self.0) };
    }

    pub fn move_to(&self, x: f64, y: f64) {
        unsafe { cairo_move_to(self.0, x, y) };
    }

    pub fn line_to(&self, x: f64, y: f64) {
        unsafe { cairo_line_to(self.0, x, y) };
    }

    pub fn stroke(&self) {
        unsafe { cairo_stroke(self.0) };
    }

    pub fn set_line_width(&self, width: f64) {
        unsafe { cairo_set_line_width(self.0, width) };
    }

    pub fn set_source_rgba(&self, [r, g, b, a]: [f64; 4]) {
        unsafe { cairo_set_source_rgba(self.0, r, g, b, a) };
    }
}

pub struct Token(*mut c_void);

impl Token {
    pub fn new(text: &CStr, fpx: f64, italic: bool) -> Token {
        let size = if fpx < 1.0 { 1.0 } else { fpx };
        unsafe {
            let layout = pango_layout_new(ns_paint_text_context());
            let desc = pango_font_description_new();
            pango_font_description_set_family(desc, c"serif".as_ptr());
            pango_font_description_set_absolute_size(
                desc,
                f64::from(ns_paint_pango_font_size(size)),
            );
            pango_font_description_set_style(
                desc,
                if italic {
                    PANGO_STYLE_ITALIC
                } else {
                    PANGO_STYLE_NORMAL
                },
            );
            pango_layout_set_font_description(layout, desc);
            pango_font_description_free(desc);
            pango_layout_set_text(layout, text.as_ptr(), -1);
            Token(layout)
        }
    }

    pub fn baseline(&self) -> c_int {
        unsafe { pango_layout_get_baseline(self.0) }
    }

    pub fn logical_size(&self) -> (c_int, c_int) {
        let mut logical = PangoRectangle {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        unsafe { pango_layout_get_extents(self.0, ptr::null_mut(), &mut logical) };
        (logical.width, logical.height)
    }

    pub fn show(&self, canvas: &Canvas) {
        unsafe { pango_cairo_show_layout(canvas.0, self.0) };
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0) };
    }
}

pub fn is_single_letter(text: &CStr) -> bool {
    unsafe {
        g_utf8_strlen(text.as_ptr(), -1) == 1
            && g_unichar_isalpha(g_utf8_get_char(text.as_ptr())) != 0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_math_measure(
    math: *const NsNode,
    font_px: f64,
    out_w: *mut f64,
    out_ascent: *mut f64,
    out_descent: *mut f64,
) {
    let (width, ascent, descent) = crate::measure(unsafe { Node::from_ptr(math) }, font_px);
    for (out, value) in [(out_w, width), (out_ascent, ascent), (out_descent, descent)] {
        if !out.is_null() {
            unsafe { *out = value };
        }
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_math_paint(
    cr: *mut c_void,
    math: *const NsNode,
    x: f64,
    y: f64,
    font_px: f64,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) {
    let Some(math) = (unsafe { Node::from_ptr(math) }) else {
        return;
    };
    if cr.is_null() {
        return;
    }
    crate::paint(&Canvas(cr), math, x, y, font_px, [r, g, b, a]);
}
