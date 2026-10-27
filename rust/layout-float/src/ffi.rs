//! Southstar — the float references block layout keeps, and the style, DOM and overflow accessors the float code reads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::{PropId, StyleRef};

#[derive(Clone, Copy)]
pub struct FloatRef {
    b: Option<BoxRef<'static>>,
    pub side: c_int,
    pub top: f64,
    pub bottom: f64,
    pub outer_w: f64,
}

impl FloatRef {
    pub fn placed(b: BoxRef<'_>, side: i32, top: f64, bottom: f64, outer_w: f64) -> FloatRef {
        FloatRef {
            b: Some(b.into_static()),
            side,
            top,
            bottom,
            outer_w,
        }
    }

    pub fn band(side: i32, top: f64, bottom: f64, outer_w: f64) -> FloatRef {
        FloatRef {
            b: None,
            side,
            top,
            bottom,
            outer_w,
        }
    }

    pub fn placed_box<'a>(&self) -> Option<BoxRef<'a>> {
        self.b
    }
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn overflow_axis_keyword<'a>(s: Option<StyleRef<'a>>, axis: PropId) -> Option<&'a CStr> {
    Some(southstar_css::overflow_keyword(s.map(StyleRef::data), axis))
}

pub use southstar_layout_geom::dispatch::shift_box_tree;
