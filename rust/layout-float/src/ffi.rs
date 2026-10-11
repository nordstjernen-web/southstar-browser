//! Southstar — the float and block formatting context entry points layout.c calls, the float references block layout keeps and the layout.c calls they make back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::{PropId, StyleRef};

use crate::overflow;

unsafe extern "C" {
    fn ns_style_overflow_keyword(s: *const Style, axis: c_int) -> *const c_char;
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_layout_style_is_absolute_or_fixed(s: *const Style) -> GBoolean;
    fn ns_layout_style_is_flex_container(s: *const Style) -> GBoolean;
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

pub fn style_of<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn dom_of<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub fn overflow_axis_keyword<'a>(s: Option<StyleRef<'a>>, axis: PropId) -> Option<&'a CStr> {
    let s = s.map_or(ptr::null(), StyleRef::as_ptr);
    c_str(unsafe { ns_style_overflow_keyword(s, axis as c_int) })
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub fn style_is_absolute_or_fixed(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_absolute_or_fixed(s) != 0 }
}

pub fn style_is_flex_container(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_flex_container(s) != 0 }
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn style_ref<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_overflow_axis_keyword(
    s: *const Style,
    axis: c_int,
) -> *const c_char {
    unsafe { ns_style_overflow_keyword(s, axis) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_overflow_kw_scrolls(ov: *const c_char) -> GBoolean {
    boolean(overflow::kw_scrolls(c_str(ov)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_clips_children(b: *const NsBox) -> GBoolean {
    boolean(box_ref(b).is_some_and(overflow::box_clips_children))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_is_scroll_container(b: *const NsBox) -> GBoolean {
    boolean(box_ref(b).is_some_and(overflow::box_is_scroll_container))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_clips_out_point(b: *const NsBox, x: f64, y: f64) -> GBoolean {
    boolean(box_ref(b).is_some_and(|b| overflow::clips_out_point(b, x, y)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_float_side_of(s: *const Style) -> c_int {
    overflow::float_side_of(style_ref(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_clear_kind_of(s: *const Style) -> c_int {
    overflow::clear_kind_of(style_ref(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_overflow_establishes_bfc(s: *const Style) -> GBoolean {
    boolean(overflow::overflow_establishes_bfc(style_ref(s)))
}
