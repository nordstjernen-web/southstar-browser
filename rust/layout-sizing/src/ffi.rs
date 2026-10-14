//! Southstar — the sizing entry points layout.c calls and the layout.c, css, svg and grid calls sizing makes back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, Edges, MediaRef, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::{estimate, intrinsic, replaced};

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
    fn ns_layout_resolve_height_with_basis(
        hv: *const NsCssValue,
        width_basis: f64,
        height_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_containing_block_definite_height(b: *const NsBox) -> f64;
    fn ns_layout_height_keyword_stretches(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_style_is_absolute_or_fixed(s: *const Style) -> GBoolean;
    fn ns_layout_style_is_flex_container(s: *const Style) -> GBoolean;
    fn ns_layout_aspect_ratio_number(v: *const NsCssValue, with_auto: *mut GBoolean) -> f64;
    fn ns_layout_flex_box_is_border_box(c: *const NsBox) -> GBoolean;
    fn ns_layout_flex_grow_of(c: *const NsBox) -> f64;
    fn ns_layout_flex_shrink_of(c: *const NsBox) -> f64;
    fn ns_layout_flex_gap_of(s: *const Style, basis: f64) -> f64;
    fn ns_layout_flex_wraps(s: *const Style) -> GBoolean;
    fn ns_layout_flex_direction_of(s: *const Style) -> *const c_char;
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_box_is_abs_placeholder(b: *const NsBox) -> GBoolean;
    fn ns_layout_inline_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_inline_min_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_table_intrinsic_width(
        b: *const NsBox,
        inherited: *const Style,
        min: GBoolean,
    ) -> f64;
    fn ns_layout_grid_natural_width(b: *const NsBox, child_style: *const Style) -> f64;
    fn ns_layout_grid_flows_by_column(s: *const Style) -> GBoolean;
    fn ns_layout_grid_column_flow_width(
        b: *const NsBox,
        child_style: *const Style,
        min_content: GBoolean,
    ) -> f64;
    fn ns_layout_image_dimension_attr(n: *const NsNode, name: *const c_char) -> f64;
    fn ns_svg_intrinsic_size(svg: *const NsNode, out: *mut SvgSize);
    fn ns_css_length_or(v: *const NsCssValue, fallback: f64) -> f64;
}

#[repr(C)]
#[derive(Default)]
pub struct SvgSize {
    pub width: f64,
    pub height: f64,
    pub ratio: f64,
    has_width: GBoolean,
    has_height: GBoolean,
    has_ratio: GBoolean,
}

impl SvgSize {
    pub fn has_width(&self) -> bool {
        self.has_width != 0
    }

    pub fn has_ratio(&self) -> bool {
        self.has_ratio != 0
    }
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(ptr::null(), ValueRef::as_ptr)
}

pub fn style_of<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn dom_of<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn image_of<'a>(media: MediaRef<'a>) -> Option<ImageRef<'a>> {
    unsafe { ImageRef::from_ptr(media.image()) }
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(value_ptr(v), basis, fallback) }
}

pub fn value_is_percent(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_value_is_percent(value_ptr(v)) != 0 }
}

pub fn edges_of(s: *const Style, basis: f64) -> (Edges, Edges, Edges) {
    let (mut m, mut p, mut b) = (Edges::default(), Edges::default(), Edges::default());
    unsafe { ns_layout_edges_from_style(s, basis, &mut m, &mut p, &mut b) };
    (m, p, b)
}

pub fn edges_into_box(b: BoxRef<'_>, basis: f64) {
    let (margin, padding, border) = b.edges_mut();
    unsafe { ns_layout_edges_from_style(b.style(), basis, margin, padding, border) }
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

pub fn height_keyword_stretches(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_height_keyword_stretches(value_ptr(v)) != 0 }
}

pub fn style_is_absolute_or_fixed(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_absolute_or_fixed(s) != 0 }
}

pub fn style_is_flex_container(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_flex_container(s) != 0 }
}

pub fn aspect_ratio_number(v: Option<ValueRef<'_>>) -> (f64, bool) {
    let mut with_auto: GBoolean = 0;
    let ratio = unsafe { ns_layout_aspect_ratio_number(value_ptr(v), &mut with_auto) };
    (ratio, with_auto != 0)
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

pub fn flex_direction_of<'a>(s: *const Style) -> &'a [u8] {
    unsafe { CStr::from_ptr(ns_layout_flex_direction_of(s)) }.to_bytes()
}

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), parent_content_width, inherited) }
}

pub fn box_is_abs_placeholder(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_is_abs_placeholder(b.as_ptr()) != 0 }
}

pub fn inline_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_inline_natural_width(b.as_ptr(), parent_style) }
}

pub fn inline_min_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_inline_min_width(b.as_ptr(), parent_style) }
}

pub fn table_intrinsic_width(b: BoxRef<'_>, inherited: *const Style, min: bool) -> f64 {
    unsafe { ns_layout_table_intrinsic_width(b.as_ptr(), inherited, boolean(min)) }
}

pub fn grid_natural_width(b: BoxRef<'_>, child_style: *const Style) -> f64 {
    unsafe { ns_layout_grid_natural_width(b.as_ptr(), child_style) }
}

pub fn grid_flows_by_column(s: *const Style) -> bool {
    unsafe { ns_layout_grid_flows_by_column(s) != 0 }
}

pub fn grid_column_flow_width(b: BoxRef<'_>, child_style: *const Style, min: bool) -> f64 {
    unsafe { ns_layout_grid_column_flow_width(b.as_ptr(), child_style, boolean(min)) }
}

pub fn image_dimension_attr(n: Option<Node<'_>>, name: &CStr) -> f64 {
    let n = n.map_or(ptr::null(), Node::as_ptr);
    unsafe { ns_layout_image_dimension_attr(n, name.as_ptr()) }
}

pub fn svg_intrinsic_size(svg: Option<Node<'_>>) -> SvgSize {
    let mut size = SvgSize::default();
    unsafe { ns_svg_intrinsic_size(svg.map_or(ptr::null(), Node::as_ptr), &mut size) };
    size
}

pub fn container_w() -> f64 {
    southstar_css::container_unit_dims().0
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn length_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    unsafe { ns_css_length_or(value_ptr(v), fallback) }
}

pub fn parse_int(s: &CStr, dflt: i32, min_v: i32, max_v: i32) -> i32 {
    southstar_dom::controls::parse_int(Some(s), dflt, min_v, max_v)
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_image(b: *mut NsBox, parent_content_width: f64) {
    if let Some(b) = box_ref(b) {
        replaced::layout_image(b, parent_content_width);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_size_keyword_is_intrinsic(v: *const NsCssValue) -> GBoolean {
    boolean(intrinsic::size_keyword_is_intrinsic(unsafe {
        ValueRef::from_ptr(v)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_measure_inline_atomics_begin(
    b: *mut NsBox,
    parent_style: *const Style,
    max_content: GBoolean,
) -> *mut c_void {
    box_ref(b)
        .and_then(|b| replaced::measure_inline_atomics_begin(b, parent_style, max_content != 0))
        .map_or(ptr::null_mut(), |saved| Box::into_raw(saved).cast())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_measure_inline_atomics_end(saved: *mut c_void) {
    if saved.is_null() {
        return;
    }
    let saved = unsafe { Box::from_raw(saved.cast::<replaced::SavedGeometry<'static>>()) };
    replaced::measure_inline_atomics_end(*saved);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_measure_natural_width(
    b: *mut NsBox,
    parent_style: *const Style,
) -> f64 {
    box_ref(b).map_or(0.0, |b| intrinsic::measure_natural_width(b, parent_style))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_min_width_of(b: *mut NsBox, parent_style: *const Style) -> f64 {
    intrinsic::min_width_of(box_ref(b), parent_style)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_min_content_width_of(
    b: *mut NsBox,
    parent_style: *const Style,
) -> f64 {
    match box_ref(b) {
        Some(b) => intrinsic::min_content_width_of(b, parent_style),
        None => 0.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_intrinsic_keyword_width(
    b: *mut NsBox,
    kw: *const c_char,
    mi: *const Style,
    avail: f64,
) -> f64 {
    match (box_ref(b), c_str(kw)) {
        (Some(b), Some(kw)) => intrinsic::intrinsic_keyword_width(b, kw, mi, avail),
        _ => -1.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_estimate_natural_width(b: *const NsBox, cap: f64) -> f64 {
    box_ref(b).map_or(0.0, |b| estimate::estimate_natural_width(b, cap))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_intrinsic_width(b: *const NsBox) -> f64 {
    box_ref(b).map_or(0.0, estimate::text_input_intrinsic_width)
}
