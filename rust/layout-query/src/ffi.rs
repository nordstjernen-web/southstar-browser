//! Southstar — the box-tree query entry points and the length resolution and inline extents they call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GPtrArray, boolean, g_ptr_array_add};
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style, inline_kind};
use southstar_style::StyleRef;

use crate::{collect, extent, find, snap};

unsafe extern "C" {
    fn ns_paint_inline_range_extents(
        b: *const NsBox,
        start: usize,
        len: usize,
        element: *const InlineAttr,
        out_x: *mut f64,
        out_y: *mut f64,
        out_w: *mut f64,
        out_h: *mut f64,
    ) -> GBoolean;
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn inline_range_extents(b: BoxRef<'_>, r: &InlineAttr) -> Option<(f64, f64, f64, f64)> {
    let element: *const InlineAttr = if r.kind == inline_kind::ELEMENT {
        r
    } else {
        ptr::null()
    };
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    (unsafe {
        ns_paint_inline_range_extents(
            b.as_ptr(),
            r.start,
            r.len,
            element,
            &mut x,
            &mut y,
            &mut w,
            &mut h,
        )
    } != 0)
        .then_some((x, y, w, h))
}

fn entry<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn needle<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_bytes()).filter(|n| !n.is_empty())
}

fn store(out: *mut f64, v: f64) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = v;
    }
}

fn push_all(out: *mut GPtrArray, boxes: Vec<BoxRef<'_>>) {
    for b in boxes {
        unsafe { g_ptr_array_add(out, b.as_mut_ptr().cast()) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_box_kind_name(kind: c_uint) -> *const c_char {
    collect::kind_name(kind).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_count_matches(
    root: *const NsBox,
    needle_text: *const c_char,
    case_sensitive: GBoolean,
) -> c_uint {
    match (entry(root), needle(needle_text)) {
        (Some(root), Some(n)) => find::count_matches(root, n, case_sensitive != 0),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_first_match_below(
    root: *const NsBox,
    needle_text: *const c_char,
    y_threshold: f64,
    case_sensitive: GBoolean,
) -> *const NsBox {
    match (entry(root), needle(needle_text)) {
        (Some(root), Some(n)) => find::first_match_below(root, n, y_threshold, case_sensitive != 0)
            .map_or(ptr::null(), BoxRef::as_ptr),
        _ => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_first_match_above(
    root: *const NsBox,
    needle_text: *const c_char,
    y_threshold: f64,
    case_sensitive: GBoolean,
) -> *const NsBox {
    match (entry(root), needle(needle_text)) {
        (Some(root), Some(n)) => find::first_match_above(root, n, y_threshold, case_sensitive != 0)
            .map_or(ptr::null(), BoxRef::as_ptr),
        _ => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_match_ordinal(
    root: *const NsBox,
    needle_text: *const c_char,
    target: *const NsBox,
    case_sensitive: GBoolean,
) -> c_uint {
    match (entry(root), needle(needle_text), entry(target)) {
        (Some(root), Some(n), Some(target)) => {
            find::match_ordinal(root, n, target, case_sensitive != 0)
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_scroll_snap_from(scroller: *mut NsBox, prev_x: f64, prev_y: f64) {
    let Some(scroller) = entry(scroller) else {
        return;
    };
    let Some(style) = style_of(scroller) else {
        return;
    };
    let Some(kind) = snap::snap_type(style) else {
        return;
    };
    let port = snap::scroller_port(scroller, style, prev_x, prev_y);
    if let Some((x, y)) = snap::solve(scroller, kind, &port) {
        scroller.set_scroll(x.unwrap_or(port.cur_x), y.unwrap_or(port.cur_y));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_scroll_snap(scroller: *mut NsBox) {
    if let Some(b) = entry(scroller) {
        unsafe { ns_box_scroll_snap_from(scroller, b.scroll_x(), b.scroll_y()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_scroll_snap_viewport(
    root: *mut NsBox,
    s: *const Style,
    viewport_w: f64,
    viewport_h: f64,
    max_x: f64,
    max_y: f64,
    prev_x: f64,
    prev_y: f64,
    x: *mut f64,
    y: *mut f64,
) -> GBoolean {
    let (Some(root), Some(style)) = (entry(root), unsafe { StyleRef::from_ptr(s) }) else {
        return 0;
    };
    let (Some(x), Some(y)) = (unsafe { x.as_mut() }, unsafe { y.as_mut() }) else {
        return 0;
    };
    if viewport_w.is_nan() || viewport_w <= 0.0 || viewport_h.is_nan() || viewport_h <= 0.0 {
        return 0;
    }
    let Some(kind) = snap::snap_type(style) else {
        return 0;
    };
    let port = snap::viewport_port(
        style,
        viewport_w,
        viewport_h,
        (*x, *y),
        (prev_x, prev_y),
        (max_x, max_y),
    );
    let Some((sx, sy)) = snap::solve(root, kind, &port) else {
        return 0;
    };
    if let Some(sx) = sx {
        *x = sx;
    }
    if let Some(sy) = sy {
        *y = sy;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_compute_paint_bounds(root: *mut NsBox) {
    if let Some(root) = entry(root) {
        extent::compute_paint_bounds(root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_subtree_extent_y(
    b: *const NsBox,
    top: *mut f64,
    bottom: *mut f64,
) -> GBoolean {
    let ext = extent::subtree_extent_y(entry(b));
    store(top, ext.top);
    store(bottom, ext.bottom);
    boolean(ext.exact)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_inline_rect_for_dom(
    root: *const NsBox,
    target: *const NsNode,
    x: *mut f64,
    y: *mut f64,
    w: *mut f64,
    h: *mut f64,
) -> GBoolean {
    let (Some(root), Some(target)) = (entry(root), unsafe { Node::from_ptr(target) }) else {
        return 0;
    };
    let Some(r) = extent::inline_rect_for_dom(root, target) else {
        return 0;
    };
    store(x, r.x0);
    store(y, r.y0);
    store(w, r.x1 - r.x0);
    store(h, r.y1 - r.y0);
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_collect_images(root: *const NsBox, out: *mut GPtrArray) {
    let mut boxes = Vec::new();
    if let Some(root) = entry(root) {
        collect::images(root, &mut boxes);
    }
    push_all(out, boxes);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_collect_videos(root: *const NsBox, out: *mut GPtrArray) {
    let mut boxes = Vec::new();
    if let Some(root) = entry(root) {
        collect::videos(root, &mut boxes);
    }
    push_all(out, boxes);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_record_frame_viewports(root: *const NsBox) {
    collect::record_frame_viewports(entry(root));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_frame_viewport(
    frame: *const NsNode,
    w: *mut f64,
    h: *mut f64,
) -> GBoolean {
    let Some((fw, fh)) = unsafe { Node::from_ptr(frame) }.and_then(collect::frame_viewport) else {
        return 0;
    };
    store(w, fw);
    store(h, fh);
    1
}
