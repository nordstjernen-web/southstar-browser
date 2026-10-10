//! Southstar — box geometry: border and padding boxes, the accumulated CSS transform, scroll and sticky offsets, and mapping page points to the client viewport.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_layout::BoxRef;
use southstar_mat4::Mat4;
use southstar_style::{PropId, StyleRef};

use crate::{Element, Rect, element_named, ffi};

pub(crate) fn border_box(b: BoxRef<'_>) -> Rect {
    let margin = b.margin();
    let padding = b.padding();
    let border = b.border();
    Rect {
        x: b.x() + margin.left,
        y: b.y() + margin.top,
        w: b.content_width() + padding.left + padding.right + border.left + border.right,
        h: b.content_height() + padding.top + padding.bottom + border.top + border.bottom,
    }
}

fn transform_origin(style: StyleRef<'_>, r: Rect) -> (f64, f64, f64) {
    let mut origin = (r.x + r.w / 2.0, r.y + r.h / 2.0, 0.0);
    if let Some(tf) = style
        .get(PropId::TransformOrigin)
        .and_then(|v| v.transform())
        && tf.n_ops > 0
    {
        let o = &tf.ops[0];
        origin.0 = r.x
            + if o.a_is_percent != 0 {
                o.a / 100.0 * r.w
            } else {
                o.a
            };
        origin.1 = r.y
            + if o.b_is_percent != 0 {
                o.b / 100.0 * r.h
            } else {
                o.b
            };
        origin.2 = o.c;
    }
    origin
}

fn has_transform(style: StyleRef<'_>) -> bool {
    [
        PropId::Transform,
        PropId::Translate,
        PropId::Rotate,
        PropId::Scale,
    ]
    .into_iter()
    .any(|prop| style.get(prop).is_some())
}

fn local_transform(b: BoxRef<'_>) -> Option<Mat4> {
    let style = ffi::box_style(b).filter(|s| has_transform(*s))?;
    let r = border_box(b);
    let tm = ffi::effective_transform_matrix(style, r.w, r.h)?;
    let (ox, oy, oz) = transform_origin(style, r);
    let mut m = Mat4::IDENTITY;
    m.translate(ox, oy, oz);
    m = m.multiply(&tm);
    m.translate(-ox, -oy, -oz);
    Some(m)
}

pub(crate) fn accumulate_transform(target: BoxRef<'_>) -> Option<Mat4> {
    let mut out = Mat4::IDENTITY;
    let mut any = false;
    let mut cur = Some(target);
    while let Some(b) = cur {
        cur = b.parent();
        let mut local = false;
        let mut m = match local_transform(b) {
            Some(m) => {
                local = true;
                m
            }
            None => Mat4::IDENTITY,
        };
        if !b.same(target) && (b.scroll_x() != 0.0 || b.scroll_y() != 0.0) {
            m.translate(-b.scroll_x(), -b.scroll_y(), 0.0);
            local = true;
        }
        let (sticky_x, sticky_y) = ffi::hit_offset(b);
        if sticky_x != 0.0 || sticky_y != 0.0 {
            let mut sticky = Mat4::IDENTITY;
            sticky.translate(sticky_x, sticky_y, 0.0);
            m = sticky.multiply(&m);
            local = true;
        }
        if !local {
            continue;
        }
        out = m.multiply(&out);
        any = true;
    }
    any.then_some(out)
}

fn apply_visual_transform(b: BoxRef<'_>, r: Rect) -> Rect {
    let Some(transform) = accumulate_transform(b) else {
        return r;
    };
    let cx = [r.x, r.x + r.w, r.x, r.x + r.w];
    let cy = [r.y, r.y, r.y + r.h, r.y + r.h];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (1e18, 1e18, -1e18, -1e18);
    for i in 0..4 {
        let [mut px, mut py, _, pw] = transform.apply(cx[i], cy[i], 0.0);
        if pw.abs() > 1e-9 {
            px /= pw;
            py /= pw;
        }
        if px < min_x {
            min_x = px;
        }
        if px > max_x {
            max_x = px;
        }
        if py < min_y {
            min_y = py;
        }
        if py > max_y {
            max_y = py;
        }
    }
    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

pub(crate) fn visual_border_box(b: BoxRef<'_>) -> Rect {
    apply_visual_transform(b, border_box(b))
}

pub(crate) fn visual_padding_box(b: BoxRef<'_>) -> Rect {
    let mut r = border_box(b);
    let border = b.border();
    r.x += border.left;
    r.y += border.top;
    r.w -= border.left + border.right;
    r.h -= border.top + border.bottom;
    if r.w < 0.0 {
        r.w = 0.0;
    }
    if r.h < 0.0 {
        r.h = 0.0;
    }
    apply_visual_transform(b, r)
}

pub(crate) fn window_scroll(scope: &mut Scope<'_>, prop: &str) -> f64 {
    let global = scope.global();
    match scope.get(&global, prop) {
        Ok(v) if v.is_number() => scope.to_number(&v).unwrap_or(0.0),
        _ => 0.0,
    }
}

fn owner_iframe(node: Option<Element>) -> Option<Element> {
    let mut p = node?.parent();
    while let Some(n) = p {
        if element_named(n, "iframe") {
            return Some(n);
        }
        p = n.parent();
    }
    None
}

pub(crate) fn point_to_client(
    scope: &mut Scope<'_>,
    node: Option<Element>,
    x: &mut f64,
    y: &mut f64,
) {
    *x -= window_scroll(scope, "scrollX");
    *y -= window_scroll(scope, "scrollY");
    let js = ffi::js_of(scope);
    let Some(frame_box) = owner_iframe(node)
        .and_then(|iframe| ffi::layout_root(js).and_then(|root| ffi::find_by_dom(root, iframe)))
    else {
        return;
    };
    let frame = visual_border_box(frame_box);
    let border = frame_box.border();
    let padding = frame_box.padding();
    let mut frame_x = frame.x + border.left + padding.left;
    let mut frame_y = frame.y + border.top + padding.top;
    frame_x -= window_scroll(scope, "scrollX");
    frame_y -= window_scroll(scope, "scrollY");
    *x -= frame_x;
    *y -= frame_y;
}

pub(crate) fn box_for_this(scope: &mut Scope<'_>, this: &Value) -> Option<BoxRef<'static>> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    ffi::flush_layout(js);
    let root = ffi::layout_root(js)?;
    let node = ffi::unwrap_node(this)?;
    ffi::find_by_dom(root, node)
}

pub(crate) fn inline_rect_for_this(scope: &mut Scope<'_>, this: &Value) -> Option<Rect> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    ffi::flush_layout(js);
    let root = ffi::layout_root(js)?;
    let node = ffi::unwrap_node(this).filter(|n| n.is_element())?;
    ffi::inline_rect_for_dom(root, node)
}
