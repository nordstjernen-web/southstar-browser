//! Southstar — the C ABI of finding the image map <area> under a point in an image box.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, NsNode};

use super::{BoxKind, BoxRef, NsBox};
use crate::image_map;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_image_map_area(
    b: *const NsBox,
    local_x: f64,
    local_y: f64,
) -> *const NsNode {
    let Some(b) = (unsafe { BoxRef::from_ptr(b) }).filter(|b| b.kind() == BoxKind::Image) else {
        return core::ptr::null();
    };
    let Some(img) = (unsafe { Node::from_ptr(b.dom_ptr().cast()) }) else {
        return core::ptr::null();
    };
    let (margin, border, padding) = (b.margin(), b.border(), b.padding());
    let x = local_x - (b.x() + margin.left + border.left + padding.left);
    let y = local_y - (b.y() + margin.top + border.top + padding.top);
    Node::ptr_or_null(image_map::area_at(
        img,
        x,
        y,
        b.content_width(),
        b.content_height(),
    ))
}
