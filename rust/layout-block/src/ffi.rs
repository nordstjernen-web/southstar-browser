//! Southstar — the block layout entry points layout.c calls and the layout.c, css, mathml and layout crate calls block layout makes back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::{align, block, dispatch, fieldset, flex};

unsafe extern "C" {
    fn ns_layout_measure_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_width_of(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_intrinsic_keyword_width(
        b: *const NsBox,
        kw: *const c_char,
        mi: *const Style,
        avail: f64,
    ) -> f64;
    fn ns_layout_text_input_intrinsic_width(b: *const NsBox) -> f64;
    fn ns_layout_size_keyword_is_intrinsic(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_inline_runs_join_splits(b: *mut NsBox);
    fn ns_layout_reorder_children_by_order(b: *mut NsBox);
    fn ns_layout_inline_box_can_fragment_multicol(
        b: *const NsBox,
        style: *const Style,
        basis: f64,
    ) -> GBoolean;
    fn ns_layout_multicol_single_inline(
        b: *mut NsBox,
        inner_x: f64,
        inner_y: f64,
        col_w: f64,
        col_gap: f64,
        n_cols: c_int,
        child_inherited: *const Style,
        cursor_y: *mut f64,
    ) -> GBoolean;
    fn ns_layout_inline_run_break_before(run: *const NsBox, band_height: f64) -> usize;
    fn ns_layout_inline_run_split(run: *mut NsBox, split: usize) -> *mut NsBox;
    fn ns_layout_inline_line_height(parent_style: *const Style) -> f64;
    fn ns_layout_inline(b: *mut NsBox, content_width: f64, parent_style: *const Style);
    fn ns_layout_table(b: *mut NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_image(b: *mut NsBox, parent_content_width: f64);
    fn ns_layout_flex_row(
        b: *mut NsBox,
        cw: f64,
        inner_x: f64,
        inner_y: f64,
        child_inherited: *const Style,
        reverse: GBoolean,
        parent_content_width: f64,
        cursor_y_out: *mut f64,
    );
    fn ns_layout_flex_row_wrap(
        b: *mut NsBox,
        cw: f64,
        inner_x: f64,
        inner_y: f64,
        child_inherited: *const Style,
        reverse: GBoolean,
        cursor_y_out: *mut f64,
    );
    fn ns_layout_flex_column(
        b: *mut NsBox,
        cw: f64,
        inner_x: f64,
        inner_y: f64,
        child_inherited: *const Style,
        reverse: GBoolean,
        parent_content_height: f64,
        cursor_y_out: *mut f64,
    );
    fn ns_layout_grid(
        b: *mut NsBox,
        cw: f64,
        inner_x: f64,
        inner_y: f64,
        child_inherited: *const Style,
        cursor_y_out: *mut f64,
    );
    fn ns_math_measure(math: *const c_void, font_px: f64, w: *mut f64, a: *mut f64, d: *mut f64);
    fn ns_css_set_container_dims(inline_px: f64, block_px: f64);
    fn ns_css_used_column_count(s: *const Style, avail_w: f64, gap_out: *mut f64) -> c_int;
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(ptr::null(), ValueRef::as_ptr)
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn style_ref<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn measure_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_measure_natural_width(b.as_ptr(), parent_style) }
}

pub fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_width_of(b.as_ptr(), parent_style) }
}

pub fn intrinsic_keyword_width(
    b: BoxRef<'_>,
    kw: Option<&CStr>,
    mi: *const Style,
    avail: f64,
) -> f64 {
    let kw = kw.map_or(ptr::null(), CStr::as_ptr);
    unsafe { ns_layout_intrinsic_keyword_width(b.as_ptr(), kw, mi, avail) }
}

pub fn text_input_intrinsic_width(b: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_text_input_intrinsic_width(b.as_ptr()) }
}

pub fn size_keyword_is_intrinsic(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_size_keyword_is_intrinsic(value_ptr(v)) != 0 }
}

pub fn inline_runs_join_splits(b: BoxRef<'_>) {
    unsafe { ns_layout_inline_runs_join_splits(b.as_mut_ptr()) }
}

pub fn reorder_children_by_order(b: BoxRef<'_>) {
    unsafe { ns_layout_reorder_children_by_order(b.as_mut_ptr()) }
}

pub fn inline_box_can_fragment_multicol(b: BoxRef<'_>, style: *const Style, basis: f64) -> bool {
    unsafe { ns_layout_inline_box_can_fragment_multicol(b.as_ptr(), style, basis) != 0 }
}

#[allow(clippy::too_many_arguments)]
pub fn multicol_single_inline(
    b: BoxRef<'_>,
    inner_x: f64,
    inner_y: f64,
    col_w: f64,
    col_gap: f64,
    n_cols: i32,
    child_inherited: *const Style,
    cursor_y: &mut f64,
) -> bool {
    unsafe {
        ns_layout_multicol_single_inline(
            b.as_mut_ptr(),
            inner_x,
            inner_y,
            col_w,
            col_gap,
            n_cols,
            child_inherited,
            cursor_y,
        ) != 0
    }
}

pub fn inline_run_break_before(run: BoxRef<'_>, band_height: f64) -> usize {
    unsafe { ns_layout_inline_run_break_before(run.as_ptr(), band_height) }
}

pub fn inline_run_split(run: BoxRef<'_>, split: usize) {
    unsafe { ns_layout_inline_run_split(run.as_mut_ptr(), split) };
}

pub fn inline_line_height(parent_style: *const Style) -> f64 {
    unsafe { ns_layout_inline_line_height(parent_style) }
}

pub fn layout_inline(b: BoxRef<'_>, content_width: f64, parent_style: *const Style) {
    unsafe { ns_layout_inline(b.as_mut_ptr(), content_width, parent_style) }
}

pub fn layout_table(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_table(b.as_mut_ptr(), parent_content_width, inherited) }
}

pub fn layout_image(b: BoxRef<'_>, parent_content_width: f64) {
    unsafe { ns_layout_image(b.as_mut_ptr(), parent_content_width) }
}

pub struct ContentBox {
    pub cw: f64,
    pub inner_x: f64,
    pub inner_y: f64,
    pub child_inherited: *const Style,
}

pub fn layout_flex_row(
    b: BoxRef<'_>,
    cb: &ContentBox,
    reverse: bool,
    wrap: bool,
    parent_content_width: f64,
    cursor_y: &mut f64,
) {
    unsafe {
        if wrap {
            ns_layout_flex_row_wrap(
                b.as_mut_ptr(),
                cb.cw,
                cb.inner_x,
                cb.inner_y,
                cb.child_inherited,
                boolean(reverse),
                cursor_y,
            );
        } else {
            ns_layout_flex_row(
                b.as_mut_ptr(),
                cb.cw,
                cb.inner_x,
                cb.inner_y,
                cb.child_inherited,
                boolean(reverse),
                parent_content_width,
                cursor_y,
            );
        }
    }
}

pub fn layout_flex_column(
    b: BoxRef<'_>,
    cb: &ContentBox,
    reverse: bool,
    parent_content_width: f64,
    cursor_y: &mut f64,
) {
    unsafe {
        ns_layout_flex_column(
            b.as_mut_ptr(),
            cb.cw,
            cb.inner_x,
            cb.inner_y,
            cb.child_inherited,
            boolean(reverse),
            parent_content_width,
            cursor_y,
        );
    }
}

pub fn layout_grid(b: BoxRef<'_>, cb: &ContentBox, cursor_y: &mut f64) {
    unsafe {
        ns_layout_grid(
            b.as_mut_ptr(),
            cb.cw,
            cb.inner_x,
            cb.inner_y,
            cb.child_inherited,
            cursor_y,
        );
    }
}

pub fn math_measure(dom: *const c_void, font_px: f64) -> (f64, f64, f64) {
    let (mut w, mut a, mut d) = (0.0, 0.0, 0.0);
    unsafe { ns_math_measure(dom, font_px, &mut w, &mut a, &mut d) };
    (w, a, d)
}

pub fn set_container_dims(inline_px: f64, block_px: f64) {
    unsafe { ns_css_set_container_dims(inline_px, block_px) }
}

pub fn used_column_count(s: StyleRef<'_>, avail_w: f64, gap: &mut f64) -> i32 {
    unsafe { ns_css_used_column_count(s.as_ptr(), avail_w, gap) }
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box(
    b: *mut NsBox,
    parent_content_width: f64,
    inherited_style: *const Style,
) {
    if let Some(b) = box_ref(b) {
        dispatch::layout_box(b, parent_content_width, inherited_style);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_block(
    b: *mut NsBox,
    parent_content_width: f64,
    inherited_style: *const Style,
) {
    if let Some(b) = box_ref(b) {
        block::layout_block(b, parent_content_width, inherited_style);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_reset_container_queries() {
    dispatch::reset_container_queries();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_shift_box_tree(b: *mut NsBox, dx: f64, dy: f64) {
    if let Some(b) = box_ref(b) {
        dispatch::shift_box_tree(b, dx, dy);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_legacy_align_block_child(
    c: *mut NsBox,
    avail_x: f64,
    avail_w: f64,
    inherited: *const Style,
) {
    if let Some(c) = box_ref(c) {
        align::legacy_align_block_child(c, avail_x, avail_w, inherited);
    }
}

fn out(p: *mut f64, v: f64) {
    if let Some(p) = unsafe { p.as_mut() } {
        *p = v;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_fieldset_legend_gap(
    fs: *const NsBox,
    border_inset: *mut f64,
    gap_x0: *mut f64,
    gap_x1: *mut f64,
    gap_y0: *mut f64,
    gap_y1: *mut f64,
) -> GBoolean {
    let Some(gap) = box_ref(fs).and_then(fieldset::legend_gap) else {
        return boolean(false);
    };
    out(border_inset, gap.inset);
    out(gap_x0, gap.x0);
    out(gap_x1, gap.x1);
    out(gap_y0, gap.y0);
    out(gap_y1, gap.y1);
    boolean(true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_box_is_border_box(c: *const NsBox) -> GBoolean {
    boolean(box_ref(c).is_some_and(flex::box_is_border_box))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_grow_of(c: *const NsBox) -> f64 {
    box_ref(c).map_or(0.0, flex::grow_of)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_shrink_of(c: *const NsBox) -> f64 {
    box_ref(c).map_or(1.0, flex::shrink_of)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_gap_px(
    specific: *const NsCssValue,
    shorthand: *const NsCssValue,
    basis: f64,
) -> f64 {
    flex::gap_px(
        unsafe { ValueRef::from_ptr(specific) },
        unsafe { ValueRef::from_ptr(shorthand) },
        basis,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_gap_of(s: *const Style, basis: f64) -> f64 {
    flex::gap_of(style_ref(s), basis)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_wraps(s: *const Style) -> GBoolean {
    boolean(flex::wraps(style_ref(s)))
}
