//! Southstar — the document layout entry point, which installs the layout hooks before building, and the positioned-layout entry points the painters call, with the box builder and sizing calls positioned layout makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use std::sync::Once;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_layout::dispatch::Dispatch;
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_layout_build::{BuildInput, Passes};
use southstar_layout_sizing::Measures;
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::sticky::{self, StickyY};
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

pub fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
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

pub fn pending_entry<'a>(i: usize) -> Option<Option<Entry<'a>>> {
    let (dom, pseudo, fixed) = southstar_layout_build::abs_pending_entry(i)?;
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

fn install_layout_hooks() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        southstar_layout::dispatch::install(Dispatch {
            layout_box: southstar_layout_block::layout_box,
            layout_block: southstar_layout_block::layout_block,
            shift_box_tree: southstar_layout_block::shift_box_tree,
            legacy_align_block_child: southstar_layout_block::legacy_align_block_child,
            max_bottom: southstar_layout_float::max_bottom,
            clips_out_point: southstar_layout_float::clips_out_point,
        });
        southstar_layout_build::install_passes(Passes {
            reset_container_queries: southstar_layout_block::reset_container_queries,
            apply_position_offsets: relative::apply_position_offsets,
            process_absolute_boxes: abs::process_absolute_boxes,
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build(
    doc: *const NsNode,
    table: *mut GHashTable,
    viewport_width: f64,
    focused_input: *const NsNode,
    focused_caret_byte: usize,
    focused_sel_anchor_byte: usize,
    image_cache: *mut c_void,
    base_url: *const c_char,
) -> *mut NsBox {
    install_layout_hooks();
    // SAFETY: the caller passes the document as null or a live node of this thread's DOM.
    let doc = unsafe { Node::from_ptr(doc) };
    // SAFETY: the caller passes the computed-style table of that document, or null.
    let styles = unsafe { StyleTable::from_ptr(table) };
    // SAFETY: the focused input is null or a live node of the same document.
    let focused_input = unsafe { Node::from_ptr(focused_input) };
    southstar_layout_build::layout_build(BuildInput {
        doc,
        styles,
        viewport_width,
        focused_input,
        caret_byte: focused_caret_byte,
        anchor_byte: focused_sel_anchor_byte,
        image_cache,
        base_url,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_sticky_offset(
    b: *const NsBox,
    vp_x0: f64,
    vp_y0: f64,
    vp_x1: f64,
    vp_y1: f64,
    dx: *mut f64,
    dy: *mut f64,
) {
    let (ox, oy) = unsafe { BoxRef::from_ptr(b) }.map_or((0.0, 0.0), |b| {
        sticky::offset(b, vp_x0, vp_y0, vp_x1, vp_y1)
    });
    unsafe {
        *dx = ox;
        *dy = oy;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_in_scroller(b: *const NsBox) -> GBoolean {
    let b = unsafe { BoxRef::from_ptr(b) };
    glib::boolean(b.and_then(sticky::scrollport_for).is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_sticky_y_model(
    b: *const NsBox,
    viewport_h: f64,
    out: *mut StickyY,
) -> GBoolean {
    let (model, ok) = match unsafe { BoxRef::from_ptr(b) } {
        Some(b) => sticky::y_model(b, viewport_h),
        None => (StickyY::default(), false),
    };
    unsafe { *out = model };
    glib::boolean(ok)
}
