//! Southstar — the C ABI of MathML layout, over Cairo, southstar-text token layouts and GLib's Unicode tables.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};

use southstar_dom::{Node, NsNode};
use southstar_paint::text_layout::{self, FontDescription, Layout};
use southstar_text::font::CairoContext;

pub const PANGO_SCALE: f64 = 1024.0;

unsafe extern "C" {
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_line_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_stroke(cr: *mut c_void);
    fn cairo_set_line_width(cr: *mut c_void, width: f64);
    fn cairo_set_source_rgba(cr: *mut c_void, red: f64, green: f64, blue: f64, alpha: f64);
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

pub struct Token(Layout);

impl Token {
    pub fn new(text: &CStr, fpx: f64, italic: bool) -> Token {
        let size = if fpx < 1.0 { 1.0 } else { fpx };
        let layout = Layout::new(southstar_paint::text_context());
        let mut desc = FontDescription::new();
        desc.set_family(c"serif");
        desc.set_absolute_size(f64::from(southstar_paint::pango_font_size(size)));
        desc.set_style(if italic {
            text_layout::STYLE_ITALIC
        } else {
            text_layout::STYLE_NORMAL
        });
        layout.set_font_description(&desc);
        layout.set_text(text);
        Token(layout)
    }

    pub fn baseline(&self) -> c_int {
        self.0.baseline()
    }

    pub fn logical_size(&self) -> (c_int, c_int) {
        let logical = self.0.logical_extents();
        (logical.width, logical.height)
    }

    pub fn show(&self, canvas: &Canvas) {
        // SAFETY: a Canvas wraps the live cairo_t ns_math_paint was handed.
        if let Some(cr) = unsafe { CairoContext::from_raw(canvas.0) } {
            self.0.show_on(&cr);
        }
    }
}

pub fn is_single_letter(text: &CStr) -> bool {
    let text = text.to_string_lossy();
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(letter), None) if southstar_unicode::is_alpha(u32::from(letter))
    )
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
