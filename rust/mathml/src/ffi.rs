//! Southstar — the cairo and southstar-text calls behind MathML layout, and the paint entry point the painter calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_int};

use southstar_cairo::ContextRef;
use southstar_dom::Node;
use southstar_paint::text_layout::{self, FontDescription, Layout};

pub const PANGO_SCALE: f64 = 1024.0;

pub struct Canvas<'a> {
    cr: Option<ContextRef<'a>>,
    probe: Option<Node<'a>>,
    probed: Cell<Option<[f64; 4]>>,
}

impl<'a> Canvas<'a> {
    pub fn probing(target: Node<'a>) -> Canvas<'a> {
        Canvas {
            cr: None,
            probe: Some(target),
            probed: Cell::new(None),
        }
    }

    pub fn note(&self, node: Node<'_>, rect: [f64; 4]) {
        if self
            .probe
            .is_some_and(|target| target.as_ptr() == node.as_ptr())
        {
            self.probed.set(Some(rect));
        }
    }

    pub fn probed(&self) -> Option<[f64; 4]> {
        self.probed.get()
    }

    pub fn save(&self) {
        if let Some(cr) = self.cr {
            cr.save();
        }
    }

    pub fn restore(&self) {
        if let Some(cr) = self.cr {
            cr.restore();
        }
    }

    pub fn move_to(&self, x: f64, y: f64) {
        if let Some(cr) = self.cr {
            cr.move_to(x, y);
        }
    }

    pub fn line_to(&self, x: f64, y: f64) {
        if let Some(cr) = self.cr {
            cr.line_to(x, y);
        }
    }

    pub fn stroke(&self) {
        if let Some(cr) = self.cr {
            cr.stroke();
        }
    }

    pub fn set_line_width(&self, width: f64) {
        if let Some(cr) = self.cr {
            cr.set_line_width(width);
        }
    }

    pub fn set_source_rgba(&self, [r, g, b, a]: [f64; 4]) {
        if let Some(cr) = self.cr {
            cr.set_source_rgba(r, g, b, a);
        }
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
        if let Some(cr) = canvas.cr {
            self.0.show(cr);
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

pub fn paint_math(
    cr: ContextRef<'_>,
    math: Node<'_>,
    x: f64,
    y: f64,
    font_px: f64,
    rgba: [f64; 4],
) {
    let canvas = Canvas {
        cr: Some(cr),
        probe: None,
        probed: Cell::new(None),
    };
    crate::paint(&canvas, math, x, y, font_px, rgba);
}
