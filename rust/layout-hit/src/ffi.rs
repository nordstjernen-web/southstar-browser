//! Southstar — the layout.c, css, paint and cairo calls hit testing makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_cairo::Matrix;
use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef};
use southstar_mat4::Mat4;
pub use southstar_style::box_style as style_of;
use southstar_style::{StyleRef, StyleTable, Transform};

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
    let mut cm = Matrix::from(m);
    cm.invert().ok().map(|()| cm.to_array())
}

pub fn viewport_size() -> (f64, f64) {
    southstar_css::viewport()
}

pub use southstar_layout_position::sticky_offset;

pub use southstar_layout::image_map_area;

pub fn svg_shape_at(b: BoxRef<'_>, local_x: f64, local_y: f64) -> Option<Node<'static>> {
    if b.kind() != BoxKind::Svg {
        return None;
    }
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let styles = southstar_style::svg_styles(b);
    southstar_svg::hit_node(
        b.dom()?,
        b.content_width(),
        b.content_height(),
        StyleTable::new(styles.as_ref()),
        style_of(b),
        local_x - (b.x() + m.left + bd.left + p.left),
        local_y - (b.y() + m.top + bd.top + p.top),
    )
}

pub use southstar_paint::{
    inline_xy_to_byte, pick_3d as paint_3d_pick, registered_3d as paint_3d_registered,
};

pub fn active_modal<'a>() -> Option<Node<'a>> {
    southstar_dom::tree::active_modal()
}
