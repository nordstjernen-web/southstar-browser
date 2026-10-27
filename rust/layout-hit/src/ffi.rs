//! Southstar — the layout.c, css, paint and cairo calls hit testing makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_dom::Node;
use southstar_layout::BoxRef;
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
    fn cairo_matrix_invert(m: *mut CairoMatrix) -> c_int;
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
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

pub use southstar_layout_position::sticky_offset;

pub use southstar_layout::image_map_area;

pub use southstar_paint::{
    inline_xy_to_byte, pick_3d as paint_3d_pick, registered_3d as paint_3d_registered,
};

pub fn active_modal<'a>() -> Option<Node<'a>> {
    southstar_dom::tree::active_modal()
}
