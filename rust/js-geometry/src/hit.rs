//! Southstar — document hit testing: elementFromPoint and elementsFromPoint, with frames' documents translated into their content boxes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Scope, Value};
use southstar_layout::BoxRef;

use crate::boxes::{visual_border_box, window_scroll};
use crate::{Element, JsResult, element_named, ffi};

struct Hit {
    node: Element,
    doc: Element,
    exact_box: Option<BoxRef<'static>>,
    local_x: f64,
    local_y: f64,
}

fn point_in_hit_bounds(root: BoxRef<'_>, x: f64, y: f64) -> bool {
    let mut w = ffi::viewport_w();
    let mut h = ffi::viewport_h();
    if root.content_width() > w {
        w = root.content_width();
    }
    if root.content_height() > h {
        h = root.content_height();
    }
    x <= w && y <= h
}

fn point_usable(x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0
}

fn frame_of(doc: Option<Element>) -> Option<Element> {
    let doc = doc.filter(|d| d.kind() == Kind::Document)?;
    doc.parent().filter(|p| element_named(*p, "iframe"))
}

fn frame_point(root: BoxRef<'_>, frame: Element, x: &mut f64, y: &mut f64) -> bool {
    let Some(fb) = ffi::find_by_dom(root, frame) else {
        return false;
    };
    let r = visual_border_box(fb);
    let border = fb.border();
    let padding = fb.padding();
    let cw = r.w - border.left - border.right - padding.left - padding.right;
    let ch = r.h - border.top - border.bottom - padding.top - padding.bottom;
    if *x >= cw || *y >= ch {
        return false;
    }
    *x += r.x + border.left + padding.left;
    *y += r.y + border.top + padding.top;
    true
}

fn layout_point(
    scope: &mut Scope<'_>,
    js: ffi::Js,
    root: BoxRef<'_>,
    doc: Option<Element>,
    x: &mut f64,
    y: &mut f64,
) -> Option<Element> {
    if let Some(frame) = frame_of(doc) {
        return if frame_point(root, frame, x, y) {
            doc
        } else {
            None
        };
    }
    if !point_in_hit_bounds(root, *x, *y) {
        return None;
    }
    *x += window_scroll(scope, "scrollX");
    *y += window_scroll(scope, "scrollY");
    ffi::current_doc(js)
}

fn owner_node(hit: Element, doc: Element) -> Option<Element> {
    let mut node = hit;
    let mut p = Some(hit);
    while let Some(n) = p {
        if n == doc {
            return Some(node);
        }
        if n.kind() == Kind::Document
            && let Some(parent) = n.parent()
        {
            node = parent;
        }
        p = n.parent();
    }
    None
}

fn hit_node(scope: &mut Scope<'_>, this: &Value, mut x: f64, mut y: f64) -> Option<Hit> {
    if !point_usable(x, y) {
        return None;
    }
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    ffi::current_doc(js)?;
    ffi::flush_layout(js);
    let root = ffi::layout_root(js)?;
    let doc = layout_point(scope, js, root, ffi::unwrap_node(this), &mut x, &mut y)?;
    let (hit, local_x, local_y) = ffi::hit_test_local(root, x, y)?;
    let hit_dom = ffi::box_dom(hit)?;
    let node = owner_node(hit_dom, doc)?;
    Some(Hit {
        node,
        doc,
        exact_box: (node == hit_dom).then_some(hit),
        local_x,
        local_y,
    })
}

fn point_args(scope: &mut Scope<'_>, args: &[Value]) -> Option<(f64, f64)> {
    if args.len() < 2 {
        return None;
    }
    let x = scope.to_number(&args[0]).unwrap_or(f64::NAN);
    let y = scope.to_number(&args[1]).unwrap_or(f64::NAN);
    Some((x, y))
}

pub(crate) fn element_from_point(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some((x, y)) = point_args(scope, args) else {
        return Ok(Value::null());
    };
    let Some(hit) = hit_node(scope, this, x, y) else {
        return Ok(Value::null());
    };
    let area = hit
        .exact_box
        .and_then(|b| ffi::image_map_area(b, hit.local_x, hit.local_y));
    Ok(ffi::wrap_node(scope, area.unwrap_or(hit.node)))
}

pub(crate) fn elements_from_point(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let arr = scope.new_array();
    let Some((x, y)) = point_args(scope, args) else {
        return Ok(arr);
    };
    let Some(hit) = hit_node(scope, this, x, y) else {
        return Ok(arr);
    };
    let mut index = 0;
    let mut cur = Some(hit.node);
    while let Some(n) = cur.filter(|n| *n != hit.doc) {
        if n.is_element() {
            let wrapped = ffi::wrap_node(scope, n);
            scope.set_index(&arr, index, wrapped)?;
            index += 1;
        }
        cur = n.parent();
    }
    Ok(arr)
}
