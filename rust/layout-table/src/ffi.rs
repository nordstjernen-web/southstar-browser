//! Southstar — the table layout entry points layout.c calls and the layout.c internals table layout calls back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, Edges, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::table;

unsafe extern "C" {
    fn ns_layout_length_resolve(v: *const NsCssValue, basis: f64, fallback: f64) -> f64;
    fn ns_layout_value_is_percent(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_edges_from_style(
        s: *const Style,
        basis: f64,
        margin: *mut Edges,
        padding: *mut Edges,
        border: *mut Edges,
    );
    fn ns_layout_resolve_used_height(
        b: *const NsBox,
        hv: *const NsCssValue,
        width_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_min_width_of(b: *mut NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_measure_natural_width(b: *mut NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_content_width_of(b: *mut NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_box(b: *mut NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_block(b: *mut NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_legacy_align_block_child(
        c: *mut NsBox,
        avail_x: f64,
        avail_w: f64,
        inherited: *const Style,
    );
    fn ns_layout_shift_box_tree(b: *mut NsBox, dx: f64, dy: f64);
    fn ns_layout_translate_subtree(b: *mut NsBox, dx: f64, dy: f64);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_table(
    b: *mut NsBox,
    parent_content_width: f64,
    inherited: *const Style,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        table::layout(b, parent_content_width, inherited);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_table_intrinsic_width(
    b: *mut NsBox,
    inherited: *const Style,
    min: GBoolean,
) -> f64 {
    match unsafe { BoxRef::from_ptr(b) } {
        Some(b) => table::intrinsic_width(b, inherited, min != 0),
        None => 0.0,
    }
}

pub(crate) fn style_of(s: *const Style) -> Option<StyleRef<'static>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub(crate) fn length_resolve(v: ValueRef<'_>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(v.as_ptr(), basis, fallback) }
}

pub(crate) fn value_is_percent(v: ValueRef<'_>) -> bool {
    unsafe { ns_layout_value_is_percent(v.as_ptr()) != 0 }
}

pub(crate) fn edges_from_style(s: *const Style, basis: f64) -> (Edges, Edges, Edges) {
    let mut m = Edges::default();
    let mut p = Edges::default();
    let mut bd = Edges::default();
    unsafe { ns_layout_edges_from_style(s, basis, &mut m, &mut p, &mut bd) };
    (m, p, bd)
}

pub(crate) fn box_edges_from_style(b: BoxRef<'_>, basis: f64) {
    let (m, p, bd) = b.edges_mut();
    unsafe { ns_layout_edges_from_style(b.style(), basis, m, p, bd) };
}

pub(crate) fn resolve_used_height(
    b: BoxRef<'_>,
    hv: ValueRef<'_>,
    width_basis: f64,
    fallback: f64,
) -> f64 {
    unsafe { ns_layout_resolve_used_height(b.as_ptr(), hv.as_ptr(), width_basis, fallback) }
}

pub(crate) fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_width_of(b.as_mut_ptr(), parent_style) }
}

pub(crate) fn measure_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_measure_natural_width(b.as_mut_ptr(), parent_style) }
}

pub(crate) fn min_content_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_content_width_of(b.as_mut_ptr(), parent_style) }
}

pub(crate) fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_mut_ptr(), parent_content_width, inherited) };
}

pub(crate) fn layout_block(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_block(b.as_mut_ptr(), parent_content_width, inherited) };
}

pub(crate) fn legacy_align_block_child(
    c: BoxRef<'_>,
    avail_x: f64,
    avail_w: f64,
    inherited: *const Style,
) {
    unsafe { ns_layout_legacy_align_block_child(c.as_mut_ptr(), avail_x, avail_w, inherited) };
}

pub(crate) fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_mut_ptr(), dx, dy) };
}

pub(crate) fn translate_subtree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_translate_subtree(b.as_mut_ptr(), dx, dy) };
}
