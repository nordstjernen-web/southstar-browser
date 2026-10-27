//! Southstar — the cairo and southstar-text calls behind MathML layout, and the paint entry point the painter calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_cairo::ContextRef;
use southstar_dom::Node;
use southstar_paint::text_layout::{self, FontDescription, Layout};

pub const PANGO_SCALE: f64 = 1024.0;

pub struct Canvas<'a>(ContextRef<'a>);

impl Canvas<'_> {
    pub fn save(&self) {
        self.0.save();
    }

    pub fn restore(&self) {
        self.0.restore();
    }

    pub fn move_to(&self, x: f64, y: f64) {
        self.0.move_to(x, y);
    }

    pub fn line_to(&self, x: f64, y: f64) {
        self.0.line_to(x, y);
    }

    pub fn stroke(&self) {
        self.0.stroke();
    }

    pub fn set_line_width(&self, width: f64) {
        self.0.set_line_width(width);
    }

    pub fn set_source_rgba(&self, [r, g, b, a]: [f64; 4]) {
        self.0.set_source_rgba(r, g, b, a);
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

    pub fn show(&self, canvas: &Canvas<'_>) {
        self.0.show(canvas.0);
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

pub fn paint_math(
    cr: ContextRef<'_>,
    math: Node<'_>,
    x: f64,
    y: f64,
    font_px: f64,
    rgba: [f64; 4],
) {
    crate::paint(&Canvas(cr), math, x, y, font_px, rgba);
}
