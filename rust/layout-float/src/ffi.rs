//! Southstar — the float references block layout keeps, and the style, DOM and overflow accessors the float code reads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr;

use southstar_dom::Node;
use southstar_layout::{BoxRef, NsBox};
use southstar_style::{PropId, StyleRef};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloatRef {
    b: *mut NsBox,
    pub side: c_int,
    pub top: f64,
    pub bottom: f64,
    pub outer_w: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<FloatRef>() == 40);

impl FloatRef {
    pub fn placed(b: BoxRef<'_>, side: i32, top: f64, bottom: f64, outer_w: f64) -> FloatRef {
        FloatRef {
            b: b.as_mut_ptr(),
            side,
            top,
            bottom,
            outer_w,
        }
    }

    pub fn band(side: i32, top: f64, bottom: f64, outer_w: f64) -> FloatRef {
        FloatRef {
            b: ptr::null_mut(),
            side,
            top,
            bottom,
            outer_w,
        }
    }

    pub fn placed_box<'a>(&self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.b) }
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

pub use southstar_layout::dispatch::shift_box_tree;
