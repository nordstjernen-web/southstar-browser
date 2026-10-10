//! Southstar — element metrics: getBoundingClientRect, getClientRects, the offset, client and scroll dimensions, offsetParent and the width/height reflection of images.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::FLAG_SVG_NS;
use southstar_js_engine::{Scope, Value};
use southstar_layout::{BoxRef, children};
use southstar_style::{Display, PropId, StyleRef, display_of};

use crate::boxes::{
    border_box, box_for_this, inline_rect_for_this, point_to_client, window_scroll,
};
use crate::{Element, JsResult, Rect, cmax, cmin, element_named, ffi, name_is, round_int};

const BOX_NORMAL: u8 = 0;
const INNER_TABLE: u8 = 2;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_TABLE_CELL: u8 = 5;
const POSITIONED: [&core::ffi::CStr; 4] = [c"relative", c"absolute", c"fixed", c"sticky"];
const STRING_DIMENSIONS: [&str; 10] = [
    "iframe", "embed", "object", "marquee", "table", "colgroup", "col", "td", "th", "hr",
];

pub(crate) fn get_bounding_client_rect(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let node = ffi::unwrap_node(this);
    let b = box_for_this(scope, this);
    let mut rect = Rect::default();
    let mut got_box = b.is_some();
    if b.is_none() {
        if let Some(r) = crate::svg::client_rect(scope, node) {
            rect = r;
            got_box = true;
        } else if let Some(r) = inline_rect_for_this(scope, this) {
            rect = r;
            got_box = true;
        }
    }
    if let Some(b) = b {
        rect = crate::boxes::visual_border_box(b);
    }
    if got_box {
        point_to_client(scope, node, &mut rect.x, &mut rect.y);
    }
    Ok(ffi::dom_rect(scope, rect))
}

pub(crate) fn get_client_rects(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let arr = scope.new_array();
    if box_for_this(scope, this).is_none()
        && crate::svg::client_rect(scope, ffi::unwrap_node(this)).is_none()
        && inline_rect_for_this(scope, this).is_none()
    {
        return Ok(arr);
    }
    let rect = get_bounding_client_rect(scope, this, &[])?;
    scope.set_index(&arr, 0, rect)?;
    Ok(arr)
}

fn offset_size(scope: &mut Scope<'_>, this: &Value, pick: fn(Rect) -> f64) -> JsResult {
    match box_for_this(scope, this) {
        Some(b) => Ok(round_int(pick(border_box(b)))),
        None => Ok(inline_rect_for_this(scope, this).map_or(Value::int(0), |r| round_int(pick(r)))),
    }
}

pub(crate) fn get_offset_width(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    offset_size(scope, this, |r| r.w)
}

pub(crate) fn get_offset_height(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    offset_size(scope, this, |r| r.h)
}

fn is_root_name(node: Option<Element>) -> bool {
    node.is_some_and(|n| name_is(n, "html") || name_is(n, "body"))
}

fn client_size(scope: &mut Scope<'_>, this: &Value, horizontal: bool) -> JsResult {
    let node = ffi::unwrap_node(this);
    let viewport = || {
        if horizontal {
            ffi::viewport_w()
        } else {
            ffi::viewport_h()
        }
    };
    let Some(b) = box_for_this(scope, this) else {
        return Ok(if is_root_name(node) {
            round_int(viewport())
        } else {
            Value::int(0)
        });
    };
    if node.is_some_and(|n| name_is(n, "html")) {
        return Ok(round_int(viewport()));
    }
    let padding = b.padding();
    let mut size = if horizontal {
        b.content_width() + padding.left + padding.right
    } else {
        b.content_height() + padding.top + padding.bottom
    };
    if size < 0.0 {
        size = 0.0;
    }
    Ok(round_int(size))
}

pub(crate) fn get_client_width(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    client_size(scope, this, true)
}

pub(crate) fn get_client_height(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    client_size(scope, this, false)
}

fn offset_position(scope: &mut Scope<'_>, this: &Value, vertical: bool) -> JsResult {
    let (origin_x, origin_y) = offset_parent_origin(scope, this);
    let origin = if vertical { origin_y } else { origin_x };
    let pos = match box_for_this(scope, this) {
        Some(b) => {
            if vertical {
                b.y() + b.margin().top
            } else {
                b.x() + b.margin().left
            }
        }
        None => match inline_rect_for_this(scope, this) {
            Some(r) => {
                if vertical {
                    r.y
                } else {
                    r.x
                }
            }
            None => return Ok(Value::int(0)),
        },
    };
    Ok(Value::int((pos - origin + 0.5).floor() as i32))
}

pub(crate) fn get_offset_top(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    offset_position(scope, this, true)
}

pub(crate) fn get_offset_left(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    offset_position(scope, this, false)
}

pub(crate) fn get_client_top(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(box_for_this(scope, this).map_or(Value::int(0), |b| round_int(b.border().top)))
}

pub(crate) fn get_client_left(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(box_for_this(scope, this).map_or(Value::int(0), |b| round_int(b.border().left)))
}

fn position_is(style: Option<StyleRef<'_>>, keyword: &core::ffi::CStr) -> bool {
    style
        .and_then(|s| s.get(PropId::Position))
        .is_some_and(|v| v.is_keyword(keyword))
}

fn is_table_wrapper(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == INNER_TABLE
}

fn is_table_cell(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal != INTERNAL_NONE && d.internal == INTERNAL_TABLE_CELL
}

fn offset_parent_candidate(js: ffi::Js, n: Element) -> bool {
    if !n.is_element() {
        return false;
    }
    if element_named(n, "body") || element_named(n, "html") {
        return true;
    }
    let style = ffi::style_table(js).get(n);
    if POSITIONED.iter().any(|kw| position_is(style, kw)) {
        return true;
    }
    let d = display_of(style);
    is_table_wrapper(d) || is_table_cell(d)
}

fn offset_parent_is_static_root(js: ffi::Js, p: Element) -> bool {
    if !element_named(p, "body") && !element_named(p, "html") {
        return false;
    }
    let pos = ffi::style_table(js)
        .get(p)
        .and_then(|s| s.get(PropId::Position));
    pos.is_none_or(|v| v.is_keyword(c"static"))
}

fn is_fixed(js: ffi::Js, n: Element) -> bool {
    position_is(ffi::style_table(js).get(n), c"fixed")
}

fn ancestors(n: Element) -> impl Iterator<Item = Element> {
    core::iter::successors(n.parent(), |p| p.parent())
}

fn offset_parent_origin(scope: &mut Scope<'_>, this: &Value) -> (f64, f64) {
    let js = ffi::js_of(scope);
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.is_element()) else {
        return (0.0, 0.0);
    };
    if js.is_null() {
        return (0.0, 0.0);
    }
    ffi::flush_layout(js);
    if is_fixed(js, n) {
        return (0.0, 0.0);
    }
    for p in ancestors(n) {
        if !p.is_element() || !offset_parent_candidate(js, p) {
            continue;
        }
        if offset_parent_is_static_root(js, p) {
            return (0.0, 0.0);
        }
        let Some(pb) = ffi::layout_root(js).and_then(|root| ffi::find_by_dom(root, p)) else {
            return (0.0, 0.0);
        };
        return (
            pb.x() + pb.margin().left + pb.border().left,
            pb.y() + pb.margin().top + pb.border().top,
        );
    }
    (0.0, 0.0)
}

pub(crate) fn get_offset_parent(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.is_element()) else {
        return Ok(Value::null());
    };
    if is_fixed(js, n) {
        return Ok(Value::null());
    }
    let mut fallback = None;
    for p in ancestors(n) {
        if !p.is_element() {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(p);
        }
        if offset_parent_candidate(js, p) {
            return Ok(ffi::wrap_node(scope, p));
        }
    }
    Ok(fallback.map_or(Value::null(), |p| ffi::wrap_node(scope, p)))
}

fn is_scrolling_root(this: &Value) -> bool {
    ffi::unwrap_node(this).is_some_and(|n| name_is(n, "html"))
}

pub(crate) fn scroll_offset(scope: &mut Scope<'_>, this: &Value, vertical: bool) -> Option<f64> {
    if is_scrolling_root(this) {
        return Some(window_scroll(
            scope,
            if vertical { "scrollY" } else { "scrollX" },
        ));
    }
    let b = box_for_this(scope, this)?;
    Some(if vertical { b.scroll_y() } else { b.scroll_x() })
}

pub(crate) fn get_scroll_top(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(scroll_offset(scope, this, true).map_or(Value::int(0), Value::number))
}

pub(crate) fn get_scroll_left(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(scroll_offset(scope, this, false).map_or(Value::int(0), Value::number))
}

pub(crate) fn scrolling_root(this: &Value) -> bool {
    is_scrolling_root(this)
}

struct Extent {
    max_r: f64,
    max_btm: f64,
    min_l: f64,
    min_t: f64,
}

fn clips_overflow(style: Option<StyleRef<'_>>) -> bool {
    let Some(style) = style else {
        return false;
    };
    [PropId::Overflow, PropId::OverflowX, PropId::OverflowY]
        .into_iter()
        .any(|prop| {
            style
                .get(prop)
                .and_then(|v| v.keyword_text())
                .is_some_and(|kw| kw != c"visible")
        })
}

fn scrollable_overflow_walk(b: BoxRef<'_>, ext: &mut Extent, depth: i32) {
    if depth > 512 {
        return;
    }
    for c in children(b) {
        let style = ffi::box_style(c);
        if position_is(style, c"fixed") {
            continue;
        }
        let r = border_box(c);
        if r.w > 0.0 || r.h > 0.0 {
            if r.x + r.w > ext.max_r {
                ext.max_r = r.x + r.w;
            }
            if r.y + r.h > ext.max_btm {
                ext.max_btm = r.y + r.h;
            }
            if r.x < ext.min_l {
                ext.min_l = r.x;
            }
            if r.y < ext.min_t {
                ext.min_t = r.y;
            }
        }
        if !clips_overflow(style) {
            scrollable_overflow_walk(c, ext, depth + 1);
        }
    }
}

fn keyword_is(style: Option<StyleRef<'_>>, prop: PropId, keywords: &[&core::ffi::CStr]) -> bool {
    style
        .and_then(|s| s.get(prop))
        .is_some_and(|v| keywords.iter().any(|kw| v.is_keyword(kw)))
}

fn scrollable_overflow_size(b: BoxRef<'_>) -> (f64, f64) {
    let r = border_box(b);
    let padding = b.padding();
    let border = b.border();
    let mut pad_left = r.x + border.left;
    let mut pad_top = r.y + border.top;
    let pad_w = b.content_width() + padding.left + padding.right;
    let pad_h = b.content_height() + padding.top + padding.bottom;
    let mut ext = Extent {
        max_r: pad_left,
        max_btm: pad_top,
        min_l: pad_left + pad_w,
        min_t: pad_top + pad_h,
    };
    scrollable_overflow_walk(b, &mut ext, 0);
    let (mut flow_r, mut flow_btm, mut flow_l) = (pad_left, pad_top, pad_left + pad_w);
    let mut flow_t = pad_top + pad_h;
    let content_r = pad_left + padding.left + b.content_width();
    let content_btm = pad_top + padding.top + b.content_height();
    for c in children(b) {
        if keyword_is(
            ffi::box_style(c),
            PropId::Position,
            &[c"absolute", c"fixed"],
        ) {
            continue;
        }
        let cr = border_box(c);
        if cr.w <= 0.0 && cr.h <= 0.0 {
            continue;
        }
        let margin = c.margin();
        let mr = cmin(cr.x + cr.w + margin.right, cmax(cr.x + cr.w, content_r));
        let mb = cmin(cr.y + cr.h + margin.bottom, cmax(cr.y + cr.h, content_btm));
        if mr > flow_r {
            flow_r = mr;
        }
        if mb > flow_btm {
            flow_btm = mb;
        }
        if cr.x - margin.left < flow_l {
            flow_l = cr.x - margin.left;
        }
        if cr.y - margin.top < flow_t {
            flow_t = cr.y - margin.top;
        }
    }
    if flow_r + padding.right > ext.max_r {
        ext.max_r = flow_r + padding.right;
    }
    if flow_btm + padding.bottom > ext.max_btm {
        ext.max_btm = flow_btm + padding.bottom;
    }
    if ext.max_r < pad_left + pad_w {
        ext.max_r = pad_left + pad_w;
    }
    if ext.max_btm < pad_top + pad_h {
        ext.max_btm = pad_top + pad_h;
    }
    let style = ffi::box_style(b);
    let mut flip_x = keyword_is(style, PropId::Direction, &[c"rtl"]);
    let mut flip_y = false;
    if keyword_is(style, PropId::Display, &[c"flex", c"inline-flex"]) {
        let column = keyword_is(
            style,
            PropId::FlexDirection,
            &[c"column", c"column-reverse"],
        );
        let wrap_reverse = keyword_is(style, PropId::FlexWrap, &[c"wrap-reverse"]);
        if wrap_reverse {
            if column {
                flip_x = !flip_x;
            } else {
                flip_y = !flip_y;
            }
        }
    }
    if flip_x {
        ext.max_r = pad_left + pad_w;
        if flow_l - padding.left < ext.min_l {
            ext.min_l = flow_l - padding.left;
        }
        if ext.min_l < pad_left {
            pad_left = ext.min_l;
        }
    }
    if flip_y {
        ext.max_btm = pad_top + pad_h;
        if flow_t - padding.top < ext.min_t {
            ext.min_t = flow_t - padding.top;
        }
        if ext.min_t < pad_top {
            pad_top = ext.min_t;
        }
    }
    let w = ext.max_r - pad_left;
    let h = ext.max_btm - pad_top;
    let legacy_w = pad_w
        + if b.scroll_max_x() > 0.0 {
            b.scroll_max_x()
        } else {
            0.0
        };
    let legacy_h = pad_h
        + if b.scroll_max_y() > 0.0 {
            b.scroll_max_y()
        } else {
            0.0
        };
    (
        if w > pad_w { w } else { legacy_w },
        if h > pad_h { h } else { legacy_h },
    )
}

pub(crate) fn get_scroll_height(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let is_root = is_root_name(ffi::unwrap_node(this));
    let Some(b) = box_for_this(scope, this) else {
        return Ok(if is_root {
            round_int(ffi::viewport_h())
        } else {
            Value::int(0)
        });
    };
    let (_, mut h) = scrollable_overflow_size(b);
    if is_root {
        if let Some(root) = ffi::layout_root(ffi::js_of(scope)) {
            h = root.max_bottom(h);
        }
        let vh = ffi::viewport_h();
        if h < vh {
            h = vh;
        }
    }
    if h < 0.0 {
        h = 0.0;
    }
    Ok(round_int(h))
}

pub(crate) fn get_scroll_width(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(b) = box_for_this(scope, this) else {
        return Ok(Value::int(0));
    };
    let (mut w, _) = scrollable_overflow_size(b);
    if w < 0.0 {
        w = 0.0;
    }
    Ok(round_int(w))
}

pub(crate) fn dimension(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let width = magic == 8;
    let attr = if width { c"width" } else { c"height" };
    let node = ffi::unwrap_node(this);
    if let Some(n) = node.filter(|n| n.flags() & FLAG_SVG_NS != 0) {
        return Ok(ffi::svg_animated_length(scope, n, attr));
    }
    if let Some(n) = node.filter(|n| STRING_DIMENSIONS.iter().any(|tag| name_is(*n, tag))) {
        let value = ffi::attr_bytes(n, attr).unwrap_or_default();
        return Ok(scope.string_from_bytes(value));
    }
    if let Some(n) = node.filter(|n| name_is(*n, "img")) {
        if let Some(b) = box_for_this(scope, this) {
            let r = border_box(b);
            return Ok(round_int(if width { r.w } else { r.h }));
        }
        if n.attr(attr).is_some_and(|v| !v.is_empty()) {
            return ffi::int_attr_getter(scope, this, magic);
        }
        return ffi::img_natural_size(scope, this, width);
    }
    ffi::int_attr_getter(scope, this, magic)
}
