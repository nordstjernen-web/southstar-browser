//! Southstar — CSSOM View geometry and scrolling: client rects, offset, client and scroll metrics, element and window scrolling, and SVG geometry.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod boxes;
mod ffi;
mod metrics;
mod scroll;
mod svg;

use southstar_dom::Node;
use southstar_js_engine::Value;

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

#[derive(Clone, Copy, Default)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn name_is(node: Node<'_>, tag: &str) -> bool {
    node.name()
        .is_some_and(|name| name.to_bytes() == tag.as_bytes())
}

pub(crate) fn element_named(node: Node<'_>, tag: &str) -> bool {
    node.element_name() == Some(tag.as_bytes())
}

pub(crate) fn cmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub(crate) fn cmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub(crate) fn round_int(v: f64) -> Value {
    Value::int((v + 0.5) as i32)
}
