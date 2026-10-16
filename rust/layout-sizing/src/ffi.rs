//! Southstar — the sizing entry points layout.c calls and the layout.c, css, svg and grid calls sizing makes back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, MediaRef, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::{estimate, intrinsic, replaced};

unsafe extern "C" {
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

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn image_of<'a>(media: MediaRef<'a>) -> Option<ImageRef<'a>> {
    unsafe { ImageRef::from_ptr(media.image()) }
}

pub use southstar_layout_geom::flex::{
    box_is_border_box as flex_box_is_border_box, gap_of as flex_gap_of, grow_of as flex_grow_of,
    shrink_of as flex_shrink_of, wraps as flex_wraps,
};

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
