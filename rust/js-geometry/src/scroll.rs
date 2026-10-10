//! Southstar — scrolling: the scrollTop/scrollLeft setters, element and window scroll, scrollTo and scrollBy, scrollByLines/Pages and scrollIntoView.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_layout::BoxRef;
use southstar_style::PropId;

use crate::boxes::{border_box, box_for_this, window_scroll};
use crate::metrics::{scroll_offset, scrolling_root};
use crate::{Element, JsResult, arg, cmax, ffi};

fn set_scroll(scope: &mut Scope<'_>, this: &Value, v: f64, vertical: bool) {
    let js = ffi::js_of(scope);
    if scrolling_root(this) {
        if vertical {
            let x = window_scroll(scope, "scrollX");
            ffi::scroll_viewport(js, x, v);
        } else {
            let y = window_scroll(scope, "scrollY");
            ffi::scroll_viewport(js, v, y);
        }
        return;
    }
    let Some(b) = box_for_this(scope, this) else {
        return;
    };
    let (max, current) = if vertical {
        (b.scroll_max_y(), b.scroll_y())
    } else {
        (b.scroll_max_x(), b.scroll_x())
    };
    let max = if max > 0.0 { max } else { 0.0 };
    let mut v = v;
    if v < 0.0 {
        v = 0.0;
    }
    if v > max {
        v = max;
    }
    if v == current {
        return;
    }
    if vertical {
        b.set_scroll(b.scroll_x(), v);
    } else {
        b.set_scroll(v, b.scroll_y());
    }
    ffi::scroll_snap(b);
    if js.is_null() {
        return;
    }
    ffi::request_repaint(js);
    if let Some(el) = ffi::unwrap_node(this) {
        ffi::dispatch_scroll(js, el);
    }
    ffi::observer_schedule_tick(js);
}

pub(crate) fn set_scroll_top(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let v = scope.to_number(&arg(args, 0))?;
    set_scroll(scope, this, v, true);
    Ok(Value::undefined())
}

pub(crate) fn set_scroll_left(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let v = scope.to_number(&arg(args, 0))?;
    set_scroll(scope, this, v, false);
    Ok(Value::undefined())
}

fn is_options(v: &Value) -> bool {
    v.is_object() && !v.is_number()
}

fn optional_number(scope: &mut Scope<'_>, v: &Value) -> JsResult<Option<f64>> {
    if v.is_undefined() {
        return Ok(None);
    }
    scope.to_number(v).map(Some)
}

fn element_scroll(scope: &mut Scope<'_>, this: &Value, args: &[Value], relative: bool) -> JsResult {
    let first = arg(args, 0);
    let (x, y) = if is_options(&first) {
        let left = scope.get(&first, "left")?;
        let top = scope.get(&first, "top")?;
        (
            optional_number(scope, &left)?,
            optional_number(scope, &top)?,
        )
    } else {
        let second = arg(args, 1);
        (
            optional_number(scope, &first)?,
            optional_number(scope, &second)?,
        )
    };
    let (mut x, mut y) = (x, y);
    if relative {
        let cx = scroll_offset(scope, this, false).unwrap_or(0.0);
        let cy = scroll_offset(scope, this, true).unwrap_or(0.0);
        x = x.map(|v| v + cx);
        y = y.map(|v| v + cy);
    }
    if let Some(x) = x {
        set_scroll(scope, this, x, false);
    }
    if let Some(y) = y {
        set_scroll(scope, this, y, true);
    }
    Ok(Value::undefined())
}

pub(crate) fn element_scroll_to(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    element_scroll(scope, this, args, false)
}

pub(crate) fn element_scroll_by(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    element_scroll(scope, this, args, true)
}

fn read_scroll_xy(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<(f64, f64)> {
    let first = arg(args, 0);
    if is_options(&first) {
        let left = scope.get(&first, "left")?;
        let top = scope.get(&first, "top")?;
        return Ok((scope.to_number(&left)?, scope.to_number(&top)?));
    }
    let x = if args.is_empty() {
        0.0
    } else {
        scope.to_number(&first)?
    };
    let y = if args.len() < 2 {
        0.0
    } else {
        scope.to_number(&args[1])?
    };
    Ok((x, y))
}

pub(crate) fn window_scroll_to(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let (x, y) = read_scroll_xy(scope, args)?;
    ffi::scroll_viewport(ffi::js_of(scope), x, y);
    Ok(Value::undefined())
}

fn scroll_viewport_by(scope: &mut Scope<'_>, dx: f64, dy: f64) {
    let x = window_scroll(scope, "scrollX") + dx;
    let y = window_scroll(scope, "scrollY") + dy;
    ffi::scroll_viewport(ffi::js_of(scope), x, y);
}

pub(crate) fn window_scroll_by(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let (dx, dy) = read_scroll_xy(scope, args)?;
    scroll_viewport_by(scope, dx, dy);
    Ok(Value::undefined())
}

fn int_arg(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<i32> {
    match args.first() {
        Some(v) => scope.to_int32(v),
        None => Ok(0),
    }
}

pub(crate) fn window_scroll_by_lines(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> JsResult {
    let lines = int_arg(scope, args)?;
    scroll_viewport_by(scope, 0.0, f64::from(lines) * 16.0);
    Ok(Value::undefined())
}

pub(crate) fn window_scroll_by_pages(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let pages = int_arg(scope, args)?;
    let inner_height = scope.get(this, "innerHeight")?;
    let vh = scope.to_number(&inner_height)?;
    scroll_viewport_by(scope, 0.0, f64::from(pages) * vh);
    Ok(Value::undefined())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Start,
    Center,
    End,
    Nearest,
    Other,
}

impl Align {
    fn parse(text: &str) -> Align {
        match text {
            "start" => Align::Start,
            "center" => Align::Center,
            "end" => Align::End,
            "nearest" => Align::Nearest,
            _ => Align::Other,
        }
    }
}

fn string_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> JsResult<Option<String>> {
    let v = scope.get(object, key)?;
    if !v.is_string() {
        return Ok(None);
    }
    scope.to_string(&v).map(Some)
}

fn clamp(v: f64, low: f64, high: f64) -> f64 {
    if v > high {
        high
    } else if v < low {
        low
    } else {
        v
    }
}

fn inline_target(mode: Align, start: f64, extent: f64, size: f64, current: f64) -> f64 {
    let end = start + extent;
    match mode {
        Align::Start => start,
        Align::Center => start - (size - extent) / 2.0,
        Align::End => end - size,
        _ if start < current => start,
        _ if end > current + size => end - size,
        _ => current,
    }
}

fn block_target(mode: Align, start: f64, extent: f64, size: f64, current: f64) -> f64 {
    let end = start + extent;
    match mode {
        Align::Center => start - (size - extent) / 2.0,
        Align::End => end - size,
        Align::Nearest if start < current => start,
        Align::Nearest if end > current + size => end - size,
        Align::Nearest => current,
        _ => start,
    }
}

fn ancestors(b: BoxRef<'static>) -> impl Iterator<Item = BoxRef<'static>> {
    core::iter::successors(b.parent(), |p| p.parent())
}

fn scroll_containers(target: BoxRef<'static>, block: Align, inline: Align) -> (bool, Vec<Element>) {
    let t = border_box(target);
    let mut changed = false;
    let mut scrolled = Vec::new();
    for p in ancestors(target) {
        if !p.scrolls() || (p.scroll_max_x() <= 0.0 && p.scroll_max_y() <= 0.0) {
            continue;
        }
        let mut x = t.x;
        let mut y = t.y;
        for q in ancestors(target).take_while(|q| !q.same(p)) {
            x -= q.scroll_x();
            y -= q.scroll_y();
        }
        let margin = p.margin();
        let border = p.border();
        let padding = p.padding();
        let left = p.x() + margin.left + border.left;
        let top = p.y() + margin.top + border.top;
        let width = p.content_width() + padding.left + padding.right;
        let height = p.content_height() + padding.top + padding.bottom;
        let next_x = inline_target(inline, x - left, t.w, width, p.scroll_x());
        let next_y = block_target(block, y - top, t.h, height, p.scroll_y());
        let next_x = clamp(next_x, 0.0, cmax(0.0, p.scroll_max_x()));
        let next_y = clamp(next_y, 0.0, cmax(0.0, p.scroll_max_y()));
        if next_x != p.scroll_x() || next_y != p.scroll_y() {
            p.set_scroll(next_x, next_y);
            changed = true;
            if let Some(dom) = ffi::box_dom(p) {
                scrolled.push(dom);
            }
        }
    }
    (changed, scrolled)
}

fn in_fixed_box(target: BoxRef<'static>) -> bool {
    core::iter::successors(Some(target), |p| p.parent())
        .any(|p| ffi::box_style(p).and_then(|s| s.keyword_of(PropId::Position)) == Some(c"fixed"))
}

pub(crate) fn scroll_into_view(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let first = arg(args, 0);
    let (mut block, mut inline) = (None, None);
    if first.is_object() {
        block = string_prop(scope, &first, "block")?;
        inline = string_prop(scope, &first, "inline")?;
    }
    let mut block_mode = Align::parse(block.as_deref().unwrap_or("start"));
    let inline_mode = Align::parse(inline.as_deref().unwrap_or("nearest"));
    if first.is_bool() && !scope.to_bool(&first) {
        block_mode = Align::End;
    }
    ffi::flush_layout(js);
    let Some(target) = ffi::layout_root(js).and_then(|root| ffi::find_by_dom(root, el)) else {
        return Ok(Value::undefined());
    };
    let t = border_box(target);
    let fixed = in_fixed_box(target);
    let (mut changed, scrolled) = scroll_containers(target, block_mode, inline_mode);
    let mut view_x = t.x;
    let mut view_y = t.y;
    for p in ancestors(target) {
        view_x -= p.scroll_x();
        view_y -= p.scroll_y();
    }
    for dom in scrolled {
        ffi::dispatch_scroll(js, dom);
    }
    if !fixed {
        let vw = ffi::viewport_w();
        let vh = ffi::viewport_h();
        let sx = window_scroll(scope, "scrollX");
        let sy = window_scroll(scope, "scrollY");
        let next_x = cmax(0.0, inline_target(inline_mode, view_x, t.w, vw, sx));
        let next_y = cmax(0.0, block_target(block_mode, view_y, t.h, vh, sy));
        if next_x != sx || next_y != sy {
            ffi::note_viewport_scroll(js, next_x, next_y);
            changed = true;
        }
    }
    if changed {
        ffi::request_repaint(js);
    }
    ffi::notify_scroll_to(js, el);
    Ok(Value::undefined())
}
