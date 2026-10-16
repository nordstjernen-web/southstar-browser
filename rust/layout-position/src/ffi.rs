//! Southstar — the positioned-layout entry points layout.c and the painters call, and the layout.c calls positioned layout makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::sticky::{self, StickyY};
use crate::{abs, relative};

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_layout_grid_abs_containing_block(
        cb: *const NsBox,
        st: *const Style,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    ) -> GBoolean;
    fn ns_layout_grid_static_position(
        abox: *mut NsBox,
        cb: *const NsBox,
        area_x: f64,
        area_y: f64,
        area_w: f64,
        area_h: f64,
        static_x: GBoolean,
        static_y: GBoolean,
    );
    fn ns_layout_grid_static_align_offset(
        align: *const c_char,
        free_space: f64,
        flip: GBoolean,
    ) -> f64;
}

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

pub fn layout_box(b: BoxRef<'_>, width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), width, inherited) }
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

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

pub fn grid_abs_containing_block(cb: BoxRef<'_>, st: *const Style) -> Option<Area> {
    let mut a = Area {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    let found = unsafe {
        ns_layout_grid_abs_containing_block(cb.as_ptr(), st, &mut a.x, &mut a.y, &mut a.w, &mut a.h)
    };
    (found != 0).then_some(a)
}

pub fn grid_static_position(
    abox: BoxRef<'_>,
    cb: BoxRef<'_>,
    area: Area,
    static_x: bool,
    static_y: bool,
) {
    unsafe {
        ns_layout_grid_static_position(
            abox.as_mut_ptr(),
            cb.as_ptr(),
            area.x,
            area.y,
            area.w,
            area.h,
            glib::boolean(static_x),
            glib::boolean(static_y),
        )
    }
}

pub fn grid_static_align_offset(align: &CStr, free_space: f64) -> f64 {
    unsafe { ns_layout_grid_static_align_offset(align.as_ptr(), free_space, glib::FALSE) }
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_apply_position_offsets(
    b: *mut NsBox,
    parent_w: f64,
    parent_h: f64,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        relative::apply_position_offsets(b, parent_w, parent_h);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_process_absolute_boxes(
    root: *mut NsBox,
    styles: *mut GHashTable,
    viewport_width: f64,
) {
    if let Some(root) = unsafe { BoxRef::from_ptr(root) } {
        abs::process_absolute_boxes(
            root,
            unsafe { StyleTable::from_ptr(styles) },
            viewport_width,
        );
    }
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
