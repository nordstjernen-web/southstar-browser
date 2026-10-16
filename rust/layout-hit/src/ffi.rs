//! Southstar — the layout.c, css, paint and cairo calls hit testing makes, and the hit-test entry points.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox};
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
    fn ns_box_sticky_offset(
        b: *const NsBox,
        vp_x0: f64,
        vp_y0: f64,
        vp_x1: f64,
        vp_y1: f64,
        out_dx: *mut f64,
        out_dy: *mut f64,
    );
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

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn node<'a>(p: *const core::ffi::c_void) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p.cast()) }
}

pub fn inline_box_form_hit<'a>(
    b: BoxRef<'a>,
    local_x: f64,
    local_y: f64,
    parent_style: Option<StyleRef<'_>>,
) -> Option<Node<'a>> {
    southstar_layout_inline::inline_box_form_hit(b, local_x, local_y, parent_style)
}

pub use southstar_layout_float::box_clips_children as clips_children;

pub fn effective_transform(s: StyleRef<'_>) -> Transform {
    southstar_style::effective_transform(Some(s), None)
}

pub fn transform_to_mat4(tf: &Transform, bw: f64, bh: f64) -> Mat4 {
    southstar_style::transform_to_mat4(tf, bw, bh)
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
    southstar_css::viewport()
}

pub fn sticky_offset(b: BoxRef<'_>, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_sticky_offset(b.as_ptr(), x0, y0, x1, y1, &mut dx, &mut dy) };
    (dx, dy)
}

pub use southstar_layout::image_map_area;

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

pub fn text<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: `p` is null or a NUL-terminated string the box tree owns for as long as its boxes are borrowed.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
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
