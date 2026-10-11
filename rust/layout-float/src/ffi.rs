//! Southstar — the float and block formatting context entry points layout.c calls, the float_ref array they share with it and the layout.c calls they make back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::{GArray, GBoolean, boolean, g_array_append_vals};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::{PropId, StyleRef};

use crate::{floats, overflow};

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

pub fn overflow_axis_keyword<'a>(s: *const Style, axis: PropId) -> Option<&'a CStr> {
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

fn float_slice<'a>(a: *const GArray) -> &'a [FloatRef] {
    match unsafe { a.as_ref() } {
        Some(a) if a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.data.cast::<FloatRef>(), a.len as usize)
        },
        _ => &[],
    }
}

fn float_slice_mut<'a>(a: *mut GArray) -> &'a mut [FloatRef] {
    match unsafe { a.as_ref() } {
        Some(arr) if arr.len > 0 => unsafe {
            core::slice::from_raw_parts_mut(arr.data.cast::<FloatRef>(), arr.len as usize)
        },
        _ => &mut [],
    }
}

fn append(a: *mut GArray, refs: &[FloatRef]) {
    if refs.is_empty() {
        return;
    }
    unsafe { g_array_append_vals(a, refs.as_ptr().cast(), refs.len() as c_uint) };
}

fn store(out: *mut f64, v: f64) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = v;
    }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_establishes_bfc(b: *const NsBox) -> GBoolean {
    boolean(box_ref(b).is_none_or(overflow::box_establishes_bfc))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_multicol_distributable_children(b: *const NsBox) -> c_int {
    box_ref(b).map_or(0, overflow::multicol_distributable_children)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_multicol_column_host(b: *const NsBox) -> *mut NsBox {
    box_ref(b)
        .and_then(overflow::multicol_column_host)
        .map_or(ptr::null_mut(), BoxRef::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_collect_escaping_floats(
    b: *const NsBox,
    floats: *mut GArray,
    depth: c_int,
) {
    let Some(b) = box_ref(b) else {
        return;
    };
    if floats.is_null() {
        return;
    }
    let mut out = Vec::new();
    floats::collect_escaping(b, &mut out, depth);
    append(floats, &out);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_offsets_at(
    floats: *const GArray,
    y: f64,
    left_out: *mut f64,
    right_out: *mut f64,
) {
    let (l, r) = floats::offsets_at(float_slice(floats), y);
    store(left_out, l);
    store(right_out, r);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_offsets_over(
    floats: *const GArray,
    top: f64,
    bottom: f64,
    left_out: *mut f64,
    right_out: *mut f64,
) {
    let (l, r) = floats::offsets_over(float_slice(floats), top, bottom);
    store(left_out, l);
    store(right_out, r);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_clear_y(
    floats: *const GArray,
    y: f64,
    clear: c_int,
) -> f64 {
    if floats.is_null() {
        return y;
    }
    floats::clear_y(float_slice(floats), y, clear)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_max_bottom(floats: *const GArray) -> f64 {
    floats::max_bottom(float_slice(floats))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_advance_to_readable_width(
    floats: *const GArray,
    cw: f64,
    y: *mut f64,
    left_out: *mut f64,
    right_out: *mut f64,
) -> GBoolean {
    let refs = float_slice(floats);
    let Some(y) = (unsafe { y.as_mut() }) else {
        return boolean(false);
    };
    if refs.is_empty() {
        return boolean(false);
    }
    let (moved, l, r) = floats::advance_to_readable_width(refs, cw, y);
    store(left_out, l);
    store(right_out, r);
    boolean(moved)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_inherit(
    floats: *mut GArray,
    src: *const GArray,
    from_inner_x: f64,
    from_cw: f64,
    inner_x: f64,
    cw: f64,
    from_y: f64,
) {
    let from = floats::Outer {
        inner_x: from_inner_x,
        cw: from_cw,
    };
    let bands = floats::inherit(float_slice(src), &from, inner_x, cw, from_y);
    append(floats, &bands);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_shift_placed(floats: *mut GArray, dy: f64) {
    floats::shift_placed(float_slice_mut(floats), dy);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_floats_band_bottom(floats: *const GArray, y: f64) -> f64 {
    floats::band_bottom(float_slice(floats), y)
}
