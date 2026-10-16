//! Southstar — the float and block formatting context entry points layout.c calls, the float references block layout keeps and the layout.c calls they make back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox};
use southstar_style::{PropId, StyleRef};

use crate::overflow;

unsafe extern "C" {
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
}

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

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_clips_out_point(b: *const NsBox, x: f64, y: f64) -> GBoolean {
    boolean(box_ref(b).is_some_and(|b| overflow::clips_out_point(b, x, y)))
}
