//! Southstar — the box-tree query entry points and the length resolution and inline extents they call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, InlineAttr, NsBox, inline_kind};
use southstar_style::StyleRef;

use crate::{collect, extent};

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

fn store(out: *mut f64, v: f64) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = v;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_box_kind_name(kind: c_uint) -> *const c_char {
    collect::kind_name(kind).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_scroll_snap(scroller: *mut NsBox) {
    if let Some(b) = entry(scroller) {
        crate::scroll_snap_from(b, b.scroll_x(), b.scroll_y());
    }
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
