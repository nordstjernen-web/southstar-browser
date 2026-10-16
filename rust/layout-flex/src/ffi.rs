//! Southstar — the layout.c calls flex layout makes back into block layout, and the entry points layout.c calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_layout_float::{kw_scrolls, overflow_axis_keyword};
use southstar_style::{NsCssValue, PropId, StyleRef, ValueRef};

unsafe extern "C" {
    fn ns_layout_size_keyword_is_intrinsic(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_intrinsic_keyword_width(
        b: *const NsBox,
        kw: *const c_char,
        mi: *const Style,
        avail: f64,
    ) -> f64;
    fn ns_layout_measure_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_content_width_of(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(ptr::null(), ValueRef::as_ptr)
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn size_keyword_is_intrinsic(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_size_keyword_is_intrinsic(value_ptr(v)) != 0 }
}

pub fn intrinsic_keyword_width(b: BoxRef<'_>, kw: &CStr, mi: *const Style, avail: f64) -> f64 {
    unsafe { ns_layout_intrinsic_keyword_width(b.as_ptr(), kw.as_ptr(), mi, avail) }
}

pub fn measure_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_measure_natural_width(b.as_ptr(), parent_style) }
}

pub fn min_content_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_content_width_of(b.as_ptr(), parent_style) }
}

pub use southstar_layout_float::box_is_scroll_container;

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), parent_content_width, inherited) }
}

pub fn overflow_scrolls(s: StyleRef<'_>, axis: PropId) -> bool {
    kw_scrolls(overflow_axis_keyword(Some(s), axis))
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub use southstar_layout_geom::flex::{
    align_is_baseline as flex_align_is_baseline, box_is_border_box as flex_box_is_border_box,
    gap_of as flex_gap_of, gap_px, grow_of as flex_grow_of, item_align as flex_item_align,
    item_baseline as flex_item_baseline, shrink_of as flex_shrink_of, wraps as flex_wraps,
};

fn entry<'a>(b: *mut NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn store(out: *mut f64, v: f64) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = v;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_row(
    b: *mut NsBox,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
    reverse: GBoolean,
    parent_content_width: f64,
    cursor_y_out: *mut f64,
) {
    let Some(b) = entry(b) else { return };
    let at = crate::Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse: reverse != 0,
    };
    store(
        cursor_y_out,
        crate::row::layout(b, &at, parent_content_width),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_row_wrap(
    b: *mut NsBox,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
    reverse: GBoolean,
    cursor_y_out: *mut f64,
) {
    let Some(b) = entry(b) else { return };
    let at = crate::Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse: reverse != 0,
    };
    store(cursor_y_out, crate::wrap::layout(b, &at));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_column(
    b: *mut NsBox,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
    reverse: GBoolean,
    parent_content_height: f64,
    cursor_y_out: *mut f64,
) {
    let Some(b) = entry(b) else { return };
    let at = crate::Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse: reverse != 0,
    };
    store(
        cursor_y_out,
        crate::column::layout(b, &at, parent_content_height),
    );
}
