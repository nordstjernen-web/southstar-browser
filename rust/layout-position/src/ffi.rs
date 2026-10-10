//! Southstar — the positioned-layout entry points layout.c and the painters call, and the layout.c calls positioned layout makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_layout::{BoxRef, Edges, NsBox, Style};
use southstar_style::{NsCssValue, PropId, StyleRef, StyleTable, ValueRef};

use crate::sticky::{self, StickyY};
use crate::{abs, relative};

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
    fn ns_layout_resolve_height_with_basis(
        hv: *const NsCssValue,
        width_basis: f64,
        height_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_containing_block_definite_height(b: *const NsBox) -> f64;
    fn ns_layout_size_keyword_is_intrinsic(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_height_keyword_stretches(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_measure_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_width_of(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_layout_style_is_absolute_or_fixed(s: *const Style) -> GBoolean;
    fn ns_layout_style_is_flex_container(s: *const Style) -> GBoolean;
    fn ns_layout_keyword_or(s: *const Style, p: c_int, fallback: *const c_char) -> *const c_char;
    fn ns_layout_flex_item_align(c: *const NsBox, container_align: *const c_char) -> *const c_char;
    fn ns_layout_flex_align_is_baseline(align: *const c_char) -> GBoolean;
    fn ns_layout_flex_box_is_border_box(c: *const NsBox) -> GBoolean;
    fn ns_layout_flex_direction_of(s: *const Style) -> *const c_char;
    fn ns_layout_estimate_natural_width(b: *const NsBox, cap: f64) -> f64;
    fn ns_layout_style_creates_fixed_cb(s: *const Style) -> GBoolean;
    fn ns_layout_box_clips_children(b: *const NsBox) -> GBoolean;
    fn ns_layout_box_append_child(parent: *mut NsBox, child: *mut NsBox);
    fn ns_layout_flat_parent(n: *const NsNode) -> *const NsNode;
    fn ns_layout_abs_pending_len() -> c_uint;
    fn ns_layout_abs_pending_entry(
        i: c_uint,
        dom: *mut *const NsNode,
        pseudo: *mut *const Style,
        fixed: *mut GBoolean,
    ) -> GBoolean;
    fn ns_layout_abs_pending_clear();
    fn ns_layout_abs_static_run(dom: *const NsNode, rel_x: *mut f64, rel_y: *mut f64)
    -> *mut NsBox;
    fn ns_layout_abs_build_box(
        dom: *const NsNode,
        pseudo: *const Style,
        styles: *mut GHashTable,
    ) -> *mut NsBox;
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
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
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

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(ptr::null(), ValueRef::as_ptr)
}

fn style_ptr(s: Option<StyleRef<'_>>) -> *const Style {
    s.map_or(ptr::null(), StyleRef::as_ptr)
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

pub fn prop(b: BoxRef<'_>, p: PropId) -> Option<ValueRef<'_>> {
    style_of(b).and_then(|s| s.get(p))
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(value_ptr(v), basis, fallback) }
}

pub fn value_is_percent(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_value_is_percent(value_ptr(v)) != 0 }
}

pub fn edges_from_style(s: *const Style, basis: f64) -> (Edges, Edges, Edges) {
    let mut m = Edges::default();
    let mut p = Edges::default();
    let mut b = Edges::default();
    unsafe { ns_layout_edges_from_style(s, basis, &mut m, &mut p, &mut b) };
    (m, p, b)
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

pub fn measure_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_measure_natural_width(b.as_ptr(), parent_style) }
}

pub fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_width_of(b.as_ptr(), parent_style) }
}

pub fn layout_box(b: BoxRef<'_>, width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), width, inherited) }
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub fn style_is_absolute_or_fixed(s: *const Style) -> bool {
    unsafe { ns_layout_style_is_absolute_or_fixed(s) != 0 }
}

pub fn style_is_flex_container(s: StyleRef<'_>) -> bool {
    unsafe { ns_layout_style_is_flex_container(s.as_ptr()) != 0 }
}

pub fn keyword_or<'a>(s: StyleRef<'a>, p: PropId, fallback: &'static CStr) -> &'a CStr {
    c_str(unsafe { ns_layout_keyword_or(s.as_ptr(), p as c_int, fallback.as_ptr()) })
}

pub fn flex_item_align<'a>(c: BoxRef<'a>, container_align: &'a CStr) -> &'a CStr {
    c_str(unsafe { ns_layout_flex_item_align(c.as_ptr(), container_align.as_ptr()) })
}

pub fn flex_align_is_baseline(align: &CStr) -> bool {
    unsafe { ns_layout_flex_align_is_baseline(align.as_ptr()) != 0 }
}

pub fn flex_box_is_border_box(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_flex_box_is_border_box(b.as_ptr()) != 0 }
}

pub fn flex_direction_of(s: StyleRef<'_>) -> &CStr {
    c_str(unsafe { ns_layout_flex_direction_of(s.as_ptr()) })
}

pub fn estimate_natural_width(b: BoxRef<'_>, cap: f64) -> f64 {
    unsafe { ns_layout_estimate_natural_width(b.as_ptr(), cap) }
}

pub fn style_creates_fixed_cb(s: Option<StyleRef<'_>>) -> bool {
    unsafe { ns_layout_style_creates_fixed_cb(style_ptr(s)) != 0 }
}

pub fn box_clips_children(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_clips_children(b.as_ptr()) != 0 }
}

pub fn append_child(parent: BoxRef<'_>, child: BoxRef<'_>) {
    unsafe { ns_layout_box_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) }
}

pub fn flat_parent(n: Node<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_layout_flat_parent(n.as_ptr())) }
}

pub fn pending_len() -> usize {
    unsafe { ns_layout_abs_pending_len() as usize }
}

pub fn pending_entry<'a>(i: usize) -> Option<Option<Entry<'a>>> {
    let i = c_uint::try_from(i).ok()?;
    let mut dom = ptr::null();
    let mut pseudo = ptr::null();
    let mut fixed = glib::FALSE;
    if unsafe { ns_layout_abs_pending_entry(i, &mut dom, &mut pseudo, &mut fixed) } == 0 {
        return None;
    }
    Some(unsafe { Node::from_ptr(dom) }.map(|dom| Entry {
        dom,
        pseudo: unsafe { StyleRef::from_ptr(pseudo) },
        fixed: fixed != 0,
    }))
}

pub fn pending_clear() {
    unsafe { ns_layout_abs_pending_clear() }
}

pub fn static_run<'a>(e: Entry<'_>) -> Option<(BoxRef<'a>, f64, f64)> {
    if e.pseudo.is_some() {
        return None;
    }
    let mut rel_x = 0.0;
    let mut rel_y = 0.0;
    let run = unsafe { ns_layout_abs_static_run(e.dom.as_ptr(), &mut rel_x, &mut rel_y) };
    unsafe { BoxRef::from_ptr(run) }.map(|run| (run, rel_x, rel_y))
}

pub fn build_abs_box<'a>(e: Entry<'_>, styles: StyleTable) -> Option<BoxRef<'a>> {
    unsafe {
        BoxRef::from_ptr(ns_layout_abs_build_box(
            e.dom.as_ptr(),
            style_ptr(e.pseudo),
            styles.as_ptr(),
        ))
    }
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
    unsafe { ns_css_viewport_w() }
}

pub fn viewport_h() -> f64 {
    unsafe { ns_css_viewport_h() }
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
pub unsafe extern "C" fn ns_box_sticky_offset_in(
    b: *const NsBox,
    sp_x0: f64,
    sp_y0: f64,
    sp_x1: f64,
    sp_y1: f64,
    out_dx: *mut f64,
    out_dy: *mut f64,
) {
    let (dx, dy) = unsafe { BoxRef::from_ptr(b) }.map_or((0.0, 0.0), |b| {
        sticky::offset_in(b, sp_x0, sp_y0, sp_x1, sp_y1)
    });
    unsafe {
        *out_dx = dx;
        *out_dy = dy;
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sticky_y_offset(m: *const StickyY, scroll_y: f64) -> f64 {
    sticky::y_offset(unsafe { &*m }, scroll_y)
}
