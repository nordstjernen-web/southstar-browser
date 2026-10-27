//! Southstar — the svg intrinsic-size call sizing makes, the measures the formatting-context crates install, and the helpers sizing borrows from the other layout crates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, MediaRef};
use southstar_style::StyleRef;

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn image_of<'a>(media: MediaRef<'a>) -> Option<ImageRef<'a>> {
    media.image().and_then(ImageRef::from_media)
}

pub use southstar_layout_geom::flex::{
    box_is_border_box as flex_box_is_border_box, gap_of as flex_gap_of, grow_of as flex_grow_of,
    shrink_of as flex_shrink_of, wraps as flex_wraps,
};

pub use southstar_layout_geom::dispatch::layout_box;

pub struct Measures {
    pub inline_natural_width: fn(BoxRef<'_>, Option<StyleRef<'_>>) -> f64,
    pub inline_min_width: fn(BoxRef<'_>, Option<StyleRef<'_>>) -> f64,
    pub table_intrinsic_width: fn(BoxRef<'_>, Option<StyleRef<'_>>, bool) -> f64,
    pub grid_natural_width: fn(BoxRef<'_>, Option<StyleRef<'_>>) -> f64,
    pub grid_flows_by_column: fn(Option<StyleRef<'_>>) -> bool,
    pub grid_column_flow_width: fn(BoxRef<'_>, Option<StyleRef<'_>>, bool) -> f64,
}

static MEASURES: OnceLock<Measures> = OnceLock::new();

pub fn install(measures: Measures) {
    let _ = MEASURES.set(measures);
}

fn measures() -> &'static Measures {
    MEASURES
        .get()
        .expect("the formatting-context measures are installed before the first layout")
}

pub use southstar_layout_build::{box_is_abs_placeholder, image_dimension_attr};

pub fn inline_natural_width(b: BoxRef<'_>, parent_style: Option<StyleRef<'_>>) -> f64 {
    (measures().inline_natural_width)(b, parent_style)
}

pub fn inline_min_width(b: BoxRef<'_>, parent_style: Option<StyleRef<'_>>) -> f64 {
    (measures().inline_min_width)(b, parent_style)
}

pub fn table_intrinsic_width(b: BoxRef<'_>, inherited: Option<StyleRef<'_>>, min: bool) -> f64 {
    (measures().table_intrinsic_width)(b, inherited, min)
}

pub fn grid_natural_width(b: BoxRef<'_>, child_style: Option<StyleRef<'_>>) -> f64 {
    (measures().grid_natural_width)(b, child_style)
}

pub fn grid_flows_by_column(s: Option<StyleRef<'_>>) -> bool {
    (measures().grid_flows_by_column)(s)
}

pub fn grid_column_flow_width(b: BoxRef<'_>, child_style: Option<StyleRef<'_>>, min: bool) -> f64 {
    (measures().grid_column_flow_width)(b, child_style, min)
}

pub use southstar_svg::intrinsic_size as svg_intrinsic_size;

pub fn container_w() -> f64 {
    southstar_css::container_unit_dims().0
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn parse_int(s: &CStr, dflt: i32, min_v: i32, max_v: i32) -> i32 {
    southstar_dom::controls::parse_int(Some(s), dflt, min_v, max_v)
}
