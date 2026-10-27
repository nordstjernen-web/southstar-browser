//! Southstar — the document layout entry point, which installs the layout hooks and the painter's links before building, with the box builder and sizing calls positioned layout makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::Once;

use southstar_css::StyleData;
use southstar_dom::Node;
use southstar_layout::dispatch::Dispatch;
use southstar_layout::paint_links::PaintLinks;
use southstar_layout::{BoxRef, NsBox};
use southstar_layout_build::{BuildInput, Passes};
use southstar_layout_sizing::Measures;
use southstar_paint::{LayoutLinks, LegendGap};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::sticky;
use crate::{abs, relative};

#[derive(Clone, Copy)]
pub struct Entry<'a> {
    pub dom: Node<'a>,
    pub pseudo: Option<StyleRef<'a>>,
    pub fixed: bool,
}

#[derive(Clone, Copy)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn prop(b: BoxRef<'_>, p: PropId) -> Option<ValueRef<'_>> {
    style_of(b).and_then(|s| s.get(p))
}

pub use southstar_layout_sizing::{
    estimate_natural_width, measure_natural_width, size_keyword_is_intrinsic,
};

pub fn min_width_of(b: BoxRef<'_>, parent_style: Option<StyleRef<'_>>) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub use southstar_layout_block::{layout_box, shift_box_tree};

pub use southstar_layout_geom::flex::{
    align_is_baseline as flex_align_is_baseline, box_is_border_box as flex_box_is_border_box,
    item_align as flex_item_align,
};

pub use southstar_layout_float::box_clips_children;

pub use southstar_layout_tree::append_child;

pub fn flat_parent(n: Node<'_>) -> Option<Node<'_>> {
    southstar_layout_build::flat_parent(Some(n))
}

pub use southstar_layout_build::{
    abs_pending_clear as pending_clear, abs_pending_len as pending_len,
};

pub fn pending_entry(i: usize, styles: StyleTable) -> Option<Option<Entry<'static>>> {
    let (dom, pseudo, fixed) = southstar_layout_build::abs_pending_entry(i, styles)?;
    Some(dom.map(|dom| Entry { dom, pseudo, fixed }))
}

pub fn static_run<'a>(e: Entry<'_>) -> Option<(BoxRef<'a>, f64, f64)> {
    let key = e
        .pseudo
        .map_or(e.dom.as_ptr() as usize, |ps| ps.as_ptr() as usize);
    southstar_layout_build::abs_static_run(key)
}

pub fn build_abs_box(e: Entry<'_>, styles: StyleTable) -> Option<BoxRef<'static>> {
    southstar_layout_build::abs_build_box(e.dom, e.pseudo, styles)
}

pub fn inline_containing_block(root: BoxRef<'_>, target: Node<'_>) -> Option<Area> {
    southstar_layout_query::inline_rect_for_dom(root, target).map(|(x, y, w, h)| Area {
        x,
        y,
        w,
        h,
    })
}

pub fn grid_abs_containing_block(cb: BoxRef<'_>, st: Option<StyleRef<'_>>) -> Option<Area> {
    southstar_layout_grid::abs_containing_block(cb, st?).map(|a| Area {
        x: a.x,
        y: a.y,
        w: a.w,
        h: a.h,
    })
}

pub fn grid_static_position(
    abox: BoxRef<'_>,
    cb: BoxRef<'_>,
    area: Area,
    static_x: bool,
    static_y: bool,
) {
    let area = southstar_layout_grid::Area {
        x: area.x,
        y: area.y,
        w: area.w,
        h: area.h,
    };
    southstar_layout_grid::abs_static_position(abox, cb, &area, static_x, static_y);
}

pub fn grid_static_align_offset(align: &CStr, free_space: f64) -> f64 {
    southstar_layout_grid::abs_static_align_offset(Some(align), free_space, false)
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

fn fieldset_legend_gap(b: BoxRef<'_>) -> Option<LegendGap> {
    let gap = southstar_layout_block::fieldset_legend_gap(b)?;
    Some(LegendGap {
        inset: gap.inset,
        x0: gap.x0,
        x1: gap.x1,
        y0: gap.y0,
        y1: gap.y1,
    })
}

fn in_scroller(b: BoxRef<'_>) -> bool {
    sticky::scrollport_for(b).is_some()
}

fn dispatch_layout_box(b: BoxRef<'_>, width: f64, inherited: Option<&StyleData>) {
    southstar_layout_block::layout_box(b, width, inherited.map(StyleRef::from_data));
}

fn dispatch_layout_block(b: BoxRef<'_>, width: f64, inherited: Option<&StyleData>) {
    southstar_layout_block::layout_block(b, width, inherited.map(StyleRef::from_data));
}

fn dispatch_legacy_align(c: BoxRef<'_>, x: f64, w: f64, inherited: Option<&StyleData>) {
    southstar_layout_block::legacy_align_block_child(c, x, w, inherited.map(StyleRef::from_data));
}

fn install_layout_hooks() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        southstar_layout::dispatch::install(Dispatch {
            layout_box: dispatch_layout_box,
            layout_block: dispatch_layout_block,
            shift_box_tree: southstar_layout_block::shift_box_tree,
            legacy_align_block_child: dispatch_legacy_align,
            max_bottom: southstar_layout_float::max_bottom,
            clips_out_point: southstar_layout_float::clips_out_point,
        });
        southstar_layout_build::install_passes(Passes {
            reset_container_queries: southstar_layout_block::reset_container_queries,
            apply_position_offsets: relative::apply_position_offsets,
            process_absolute_boxes: abs::process_absolute_boxes,
        });
        southstar_layout::paint_links::install(PaintLinks {
            inline_xy_to_byte: southstar_paint::inline_xy_to_byte,
            inline_word_range: southstar_paint::inline_word_range,
            inline_range_extents: southstar_paint::inline_range_extents,
        });
        southstar_paint::install_layout_links(LayoutLinks {
            fieldset_legend_gap,
            inline_apply_atomic_shapes: southstar_layout_inline::apply_atomic_shapes,
            inline_layout_set_attrs: southstar_layout_inline::layout_set_attrs,
            control_css_extra_w: southstar_layout_content::inline_attr_css_extra_w,
            math_paint: southstar_mathml::paint_math,
            sticky_offset: sticky::offset,
            in_scroller,
        });
        southstar_layout_sizing::install(Measures {
            inline_natural_width: southstar_layout_inline::inline_natural_width,
            inline_min_width: southstar_layout_inline::inline_min_width,
            table_intrinsic_width: southstar_layout_table::table_intrinsic_width,
            grid_natural_width: southstar_layout_grid::grid_natural_width,
            grid_flows_by_column: southstar_layout_grid::grid_flows_by_column,
            grid_column_flow_width: southstar_layout_grid::grid_column_flow_width,
        });
    });
}

pub fn layout_build(input: BuildInput<'_>) -> *mut NsBox {
    install_layout_hooks();
    southstar_layout_build::layout_build(input).map_or(core::ptr::null_mut(), BoxRef::as_mut_ptr)
}
