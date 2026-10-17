//! Southstar — the box-tree query entry points and the length resolution and inline extents they call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, InlineAttr, NsBox, inline_kind};
use southstar_style::StyleRef;

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
