//! Southstar — the layout.c, css, paint and cairo calls hit testing makes, and the hit-test entry points of layout.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, LinkRange, NsBox, Style};
use southstar_mat4::Mat4;
use southstar_style::{StyleRef, Transform};

#[repr(C)]
struct CairoMatrix {
    xx: f64,
    yx: f64,
    xy: f64,
    yy: f64,
    x0: f64,
    y0: f64,
}

unsafe extern "C" {
    fn ns_layout_inline_box_form_hit(
        b: *const NsBox,
        local_x: f64,
        local_y: f64,
        parent_style: *const Style,
    ) -> *const NsNode;
    fn ns_layout_box_clips_children(b: *const NsBox) -> GBoolean;
    fn ns_layout_style_creates_fixed_cb(s: *const Style) -> GBoolean;
    fn ns_css_style_effective_transform(
        style: *const Style,
        transform_override: *const Transform,
        out: *mut Transform,
    );
    fn ns_css_transform_to_mat4(tf: *const Transform, bw: f64, bh: f64, out: *mut Mat4);
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_box_sticky_offset(
        b: *const NsBox,
        vp_x0: f64,
        vp_y0: f64,
        vp_x1: f64,
        vp_y1: f64,
        out_dx: *mut f64,
        out_dy: *mut f64,
    );
    fn ns_box_image_map_area(b: *const NsBox, local_x: f64, local_y: f64) -> *const NsNode;
    fn ns_paint_3d_registered(b: *const NsBox) -> GBoolean;
    fn ns_paint_3d_pick(root3d: *const NsBox, x: f64, y: f64) -> *const NsBox;
    fn ns_paint_inline_xy_to_byte(
        b: *const NsBox,
        rel_x: f64,
        rel_y: f64,
        out_byte: *mut usize,
    ) -> GBoolean;
    fn ns_dom_active_modal() -> *const NsNode;
    fn cairo_matrix_invert(m: *mut CairoMatrix) -> c_int;
}

pub fn style_of<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn dom_of<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn node<'a>(p: *const core::ffi::c_void) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p.cast()) }
}

pub fn inline_box_form_hit<'a>(
    b: BoxRef<'a>,
    local_x: f64,
    local_y: f64,
    parent_style: *const Style,
) -> Option<Node<'a>> {
    unsafe {
        Node::from_ptr(ns_layout_inline_box_form_hit(
            b.as_ptr(),
            local_x,
            local_y,
            parent_style,
        ))
    }
}

pub fn clips_children(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_clips_children(b.as_ptr()) != 0 }
}

pub fn style_creates_fixed_cb(s: *const Style) -> bool {
    unsafe { ns_layout_style_creates_fixed_cb(s) != 0 }
}

pub fn effective_transform(s: StyleRef<'_>) -> Transform {
    let mut eff = Transform::default();
    unsafe { ns_css_style_effective_transform(s.as_ptr(), ptr::null(), &mut eff) };
    eff
}

pub fn transform_to_mat4(tf: &Transform, bw: f64, bh: f64) -> Mat4 {
    let mut out = Mat4::IDENTITY;
    unsafe { ns_css_transform_to_mat4(tf, bw, bh, &mut out) };
    out
}

pub fn invert_affine(m: [f64; 6]) -> Option<[f64; 6]> {
    let mut cm = CairoMatrix {
        xx: m[0],
        yx: m[1],
        xy: m[2],
        yy: m[3],
        x0: m[4],
        y0: m[5],
    };
    (unsafe { cairo_matrix_invert(&mut cm) } == 0)
        .then_some([cm.xx, cm.yx, cm.xy, cm.yy, cm.x0, cm.y0])
}

pub fn viewport_size() -> (f64, f64) {
    unsafe { (ns_css_viewport_w(), ns_css_viewport_h()) }
}

pub fn sticky_offset(b: BoxRef<'_>, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_sticky_offset(b.as_ptr(), x0, y0, x1, y1, &mut dx, &mut dy) };
    (dx, dy)
}

pub fn image_map_area<'a>(b: BoxRef<'a>, local_x: f64, local_y: f64) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_box_image_map_area(b.as_ptr(), local_x, local_y)) }
}

pub fn paint_3d_registered(b: BoxRef<'_>) -> bool {
    unsafe { ns_paint_3d_registered(b.as_ptr()) != 0 }
}

pub fn paint_3d_pick<'a>(b: BoxRef<'a>, x: f64, y: f64) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(ns_paint_3d_pick(b.as_ptr(), x, y)) }
}

pub fn inline_xy_to_byte(b: BoxRef<'_>, rel_x: f64, rel_y: f64) -> Option<usize> {
    let mut byte = 0;
    (unsafe { ns_paint_inline_xy_to_byte(b.as_ptr(), rel_x, rel_y, &mut byte) } != 0)
        .then_some(byte)
}

pub fn active_modal<'a>() -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_dom_active_modal()) }
}

fn entry<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn box_out(b: Option<BoxRef<'_>>) -> *const NsBox {
    b.map_or(ptr::null(), BoxRef::as_ptr)
}

fn store(out: *mut f64, v: f64) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = v;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_style_blocks_hit_testing(s: *const Style) -> GBoolean {
    boolean(crate::stack::style_blocks_hit_testing(unsafe {
        StyleRef::from_ptr(s)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_node_is_form_hit_target(n: *const NsNode) -> GBoolean {
    boolean(crate::stack::node_is_form_hit_target(unsafe {
        Node::from_ptr(n)
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_box_set_hit_viewport(scroll_x: f64, scroll_y: f64) {
    crate::geometry::set_viewport(scroll_x, scroll_y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_is_fixed(b: *const NsBox) -> GBoolean {
    boolean(entry(b).is_some_and(crate::geometry::is_fixed))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_offset(b: *const NsBox, dx: *mut f64, dy: *mut f64) {
    let (x, y) = entry(b).map_or((0.0, 0.0), crate::geometry::hit_offset);
    store(dx, x);
    store(dy, y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_test(root: *const NsBox, x: f64, y: f64) -> *const NsBox {
    box_out(entry(root).and_then(|root| crate::tree::hit_test(root, x, y)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_test_local(
    root: *const NsBox,
    x: f64,
    y: f64,
    local_x: *mut f64,
    local_y: *mut f64,
) -> *const NsBox {
    let hit = entry(root).and_then(|root| crate::tree::hit_test(root, x, y));
    let (lx, ly) = crate::tree::last_local();
    store(local_x, lx);
    store(local_y, ly);
    box_out(hit)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_node(root: *const NsBox, x: f64, y: f64) -> *const NsNode {
    Node::ptr_or_null(entry(root).and_then(|root| crate::walks::hit_node(root, x, y)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_form_dom(root: *const NsBox, x: f64, y: f64) -> *const NsNode {
    Node::ptr_or_null(entry(root).and_then(|root| crate::walks::form_hit(root, x, y, ptr::null())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_inline_dom(
    root: *const NsBox,
    x: f64,
    y: f64,
) -> *const NsNode {
    Node::ptr_or_null(entry(root).and_then(|root| crate::walks::inline_dom(root, x, y)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_link_range(
    root: *const NsBox,
    x: f64,
    y: f64,
) -> *const LinkRange {
    entry(root)
        .and_then(|root| crate::walks::link_range(root, x, y))
        .map_or(ptr::null(), |r| r as *const LinkRange)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_link(root: *const NsBox, x: f64, y: f64) -> *const c_char {
    entry(root)
        .and_then(|root| crate::walks::link_range(root, x, y))
        .map_or(ptr::null(), LinkRange::href_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_scrollable(root: *mut NsBox, x: f64, y: f64) -> *mut NsBox {
    entry(root)
        .and_then(|root| crate::walks::scrollable(root, x, y))
        .map_or(ptr::null_mut(), |(b, _, _)| b.as_mut_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_hit_scrollbar(
    root: *mut NsBox,
    x: f64,
    y: f64,
    lx: *mut f64,
    ly: *mut f64,
) -> *mut NsBox {
    let Some((b, px, py)) = entry(root).and_then(|root| crate::walks::scrollable(root, x, y))
    else {
        return ptr::null_mut();
    };
    store(lx, px);
    store(ly, py);
    b.as_mut_ptr()
}
