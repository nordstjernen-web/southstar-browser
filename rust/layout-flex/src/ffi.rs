//! Southstar — the layout.c calls flex layout makes back into block layout, and the entry points layout.c calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, Edges, NsBox, Style};
use southstar_style::{NsCssValue, PropId, StyleRef, ValueRef};

unsafe extern "C" {
    fn ns_layout_length_resolve(v: *const NsCssValue, basis: f64, fallback: f64) -> f64;
    fn ns_layout_value_is_percent(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_resolve_used_height(
        b: *const NsBox,
        hv: *const NsCssValue,
        width_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_resolve_height_with_basis(
        hv: *const NsCssValue,
        width_basis: f64,
        height_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_containing_block_definite_height(b: *const NsBox) -> f64;
    fn ns_layout_size_keyword_is_intrinsic(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_height_keyword_stretches(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_intrinsic_keyword_width(
        b: *const NsBox,
        kw: *const c_char,
        mi: *const Style,
        avail: f64,
    ) -> f64;
    fn ns_layout_measure_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_content_width_of(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_box_is_scroll_container(b: *const NsBox) -> GBoolean;
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_box_read_definite_height(b: *const NsBox) -> f64;
    fn ns_layout_style_is_absolute_or_fixed(s: *const Style) -> GBoolean;
    fn ns_layout_style_is_flex_container(s: *const Style) -> GBoolean;
    fn ns_layout_keyword_or(s: *const Style, p: c_int, fallback: *const c_char) -> *const c_char;
    fn ns_layout_overflow_axis_keyword(s: *const Style, axis: c_int) -> *const c_char;
    fn ns_layout_overflow_kw_scrolls(ov: *const c_char) -> GBoolean;
    fn ns_layout_edges_from_style(
        s: *const Style,
        basis: f64,
        margin: *mut Edges,
        padding: *mut Edges,
        border: *mut Edges,
    );
    fn ns_layout_aspect_ratio_number(v: *const NsCssValue, with_auto: *mut GBoolean) -> f64;
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_layout_gap_px(
        specific: *const NsCssValue,
        shorthand: *const NsCssValue,
        basis: f64,
    ) -> f64;
    fn ns_layout_flex_box_is_border_box(c: *const NsBox) -> GBoolean;
    fn ns_layout_flex_grow_of(c: *const NsBox) -> f64;
    fn ns_layout_flex_shrink_of(c: *const NsBox) -> f64;
    fn ns_layout_flex_gap_of(s: *const Style, basis: f64) -> f64;
    fn ns_layout_flex_wraps(s: *const Style) -> GBoolean;
    fn ns_layout_flex_item_align(c: *const NsBox, container_align: *const c_char) -> *const c_char;
    fn ns_layout_flex_align_is_baseline(align: *const c_char) -> GBoolean;
    fn ns_layout_flex_item_baseline(c: *const NsBox, fallback: f64) -> f64;
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(ptr::null(), ValueRef::as_ptr)
}

fn c_str<'a>(p: *const c_char) -> &'a CStr {
    unsafe { CStr::from_ptr(p) }
}

pub fn style_of<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn dom_of<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(value_ptr(v), basis, fallback) }
}

pub fn value_is_percent(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_value_is_percent(value_ptr(v)) != 0 }
}

pub fn resolve_used_height(
    b: BoxRef<'_>,
    hv: Option<ValueRef<'_>>,
    width_basis: f64,
    fallback: f64,
) -> f64 {
    unsafe { ns_layout_resolve_used_height(b.as_ptr(), value_ptr(hv), width_basis, fallback) }
}

pub fn resolve_height_with_basis(
    hv: Option<ValueRef<'_>>,
    width_basis: f64,
    height_basis: f64,
    fallback: f64,
) -> f64 {
    unsafe {
        ns_layout_resolve_height_with_basis(value_ptr(hv), width_basis, height_basis, fallback)
    }
}

pub fn containing_block_definite_height(b: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_containing_block_definite_height(b.as_ptr()) }
}

pub fn size_keyword_is_intrinsic(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_size_keyword_is_intrinsic(value_ptr(v)) != 0 }
}

pub fn height_keyword_stretches(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_height_keyword_stretches(value_ptr(v)) != 0 }
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

pub fn box_is_scroll_container(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_is_scroll_container(b.as_ptr()) != 0 }
}

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), parent_content_width, inherited) }
}

pub fn box_read_definite_height(b: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_box_read_definite_height(b.as_ptr()) }
}

pub fn style_is_absolute_or_fixed(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_absolute_or_fixed(s) != 0 }
}

pub fn style_is_flex_container(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_flex_container(s) != 0 }
}

pub fn keyword_or<'a>(s: *const Style, p: PropId, fallback: &'static CStr) -> &'a CStr {
    c_str(unsafe { ns_layout_keyword_or(s, p as c_int, fallback.as_ptr()) })
}

pub fn overflow_scrolls(s: *const Style, axis: PropId) -> bool {
    unsafe { ns_layout_overflow_kw_scrolls(ns_layout_overflow_axis_keyword(s, axis as c_int)) != 0 }
}

pub fn edges_from_style(b: BoxRef<'_>, basis: f64) {
    let (margin, padding, border) = b.edges_mut();
    unsafe { ns_layout_edges_from_style(b.style(), basis, margin, padding, border) }
}

pub fn aspect_ratio_number(v: Option<ValueRef<'_>>) -> f64 {
    unsafe { ns_layout_aspect_ratio_number(value_ptr(v), ptr::null_mut()) }
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub fn gap_px(specific: Option<ValueRef<'_>>, shorthand: Option<ValueRef<'_>>, basis: f64) -> f64 {
    unsafe { ns_layout_gap_px(value_ptr(specific), value_ptr(shorthand), basis) }
}

pub fn flex_box_is_border_box(c: BoxRef<'_>) -> bool {
    unsafe { ns_layout_flex_box_is_border_box(c.as_ptr()) != 0 }
}

pub fn flex_grow_of(c: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_flex_grow_of(c.as_ptr()) }
}

pub fn flex_shrink_of(c: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_flex_shrink_of(c.as_ptr()) }
}

pub fn flex_gap_of(s: *const Style, basis: f64) -> f64 {
    unsafe { ns_layout_flex_gap_of(s, basis) }
}

pub fn flex_wraps(s: *const Style) -> bool {
    unsafe { ns_layout_flex_wraps(s) != 0 }
}

pub fn flex_item_align<'a>(c: BoxRef<'_>, container_align: &'a CStr) -> &'a CStr {
    c_str(unsafe { ns_layout_flex_item_align(c.as_ptr(), container_align.as_ptr()) })
}

pub fn flex_align_is_baseline(align: &CStr) -> bool {
    unsafe { ns_layout_flex_align_is_baseline(align.as_ptr()) != 0 }
}

pub fn flex_item_baseline(c: BoxRef<'_>, fallback: f64) -> f64 {
    unsafe { ns_layout_flex_item_baseline(c.as_ptr(), fallback) }
}

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
