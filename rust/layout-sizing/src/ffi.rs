//! Southstar — the sizing entry points layout.c calls and the layout.c, css, svg and grid calls sizing makes back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, MediaRef, NsBox, Style};
use southstar_style::StyleRef;

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
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

pub use southstar_layout_build::{box_is_abs_placeholder, image_dimension_attr};

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
