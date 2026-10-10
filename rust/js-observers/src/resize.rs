//! Southstar — ResizeObserver: the targets and last sizes kept on each observer object and the tick that reports boxes whose size changed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi::{self, Js, Rect};
use crate::{bind, bind_if_not_callable, get, page, page_or_new, set, set_index};

const BOXES: [&[u8]; 3] = [b"content-box", b"border-box", b"device-pixel-content-box"];

fn array_or_new(scope: &mut Scope<'_>, observer: &Value, key: &str) -> (Value, bool) {
    let value = get(scope, observer, key);
    if value.is_array() {
        return (value, false);
    }
    let array = scope.new_array();
    set(scope, observer, key, array.clone());
    (array, true)
}

fn register(scope: &mut Scope<'_>, js: Js, observer: &Value, target: &Value, box_name: &[u8]) {
    let page = page_or_new(js);
    let (targets, created) = array_or_new(scope, observer, "__targets");
    if created {
        page.resize.borrow_mut().push(observer.clone());
    }
    let (widths, _) = array_or_new(scope, observer, "__lastWidths");
    let (heights, _) = array_or_new(scope, observer, "__lastHeights");
    let (boxes, _) = array_or_new(scope, observer, "__boxes");
    let length = crate::array_length(scope, &targets);
    let wanted = ffi::node_address(target);
    let mut index = length;
    for i in 0..length {
        let existing = scope
            .get_index(&targets, i)
            .unwrap_or_else(|_| Value::undefined());
        if ffi::node_address(&existing) == wanted {
            index = i;
            break;
        }
    }
    if index == length {
        set_index(scope, &targets, length, target.clone());
    }
    if index != length {
        let name = scope.string_from_bytes(box_name);
        set_index(scope, &boxes, index, name);
    }
    set_index(scope, &widths, index, Value::undefined());
    set_index(scope, &heights, index, Value::undefined());
    if index == length {
        let name = scope.string_from_bytes(box_name);
        set_index(scope, &boxes, index, name);
    }
}

fn element_target(scope: &mut Scope<'_>, args: &[Value]) -> Result<(), Value> {
    let is_element = args
        .first()
        .and_then(ffi::unwrap_element)
        .is_some_and(|node| node.kind() == Kind::Element);
    if is_element {
        Ok(())
    } else {
        Err(scope.type_error("ResizeObserver target must be an Element"))
    }
}

pub(crate) fn observe(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    element_target(scope, args)?;
    let callback = get(scope, this, "__cb");
    if !scope.is_function(&callback) {
        return Err(scope.type_error("incompatible ResizeObserver receiver"));
    }
    drop(callback);
    let mut box_name = b"content-box".to_vec();
    if let Some(options) = args.get(1).filter(|o| !o.is_undefined() && !o.is_null()) {
        let value = scope.get(options, "box")?;
        if !value.is_undefined() {
            let converted = crate::text_of(scope, &value)?;
            if !BOXES.contains(&converted.as_slice()) {
                return Err(scope.type_error("invalid ResizeObserver box option"));
            }
            box_name = converted;
        }
    }
    let js = ffi::js_of(scope);
    if !js.is_null() {
        register(scope, js, this, &args[0], &box_name);
        ffi::schedule_tick(js);
    }
    Ok(Value::undefined())
}

fn size_list(scope: &mut Scope<'_>, inline_size: f64, block_size: f64) -> Value {
    let size = scope.new_object();
    set(scope, &size, "inlineSize", Value::number(inline_size));
    set(scope, &size, "blockSize", Value::number(block_size));
    let list = scope.new_array();
    set_index(scope, &list, 0, size);
    list
}

fn build_entry(scope: &mut Scope<'_>, target: &Value, observed_box: &[u8]) -> (f64, f64, Value) {
    let (mut content_w, mut content_h, mut content_x, mut content_y) = (0.0, 0.0, 0.0, 0.0);
    let (mut border_left, mut border_top) = (0.0, 0.0);
    let mut border = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    if let Some(b) = ffi::box_for(scope, target) {
        content_w = b.content_width();
        content_h = b.content_height();
        content_x = b.padding().left;
        content_y = b.padding().top;
        border_left = b.border().left;
        border_top = b.border().top;
        border = ffi::border_box(b);
    }
    let dppx = crate::global_number(scope, "devicePixelRatio")
        .filter(|&ratio| ratio > 0.0 && ratio.is_finite())
        .unwrap_or(1.0);
    let device_x = (border.x + border_left + content_x) * dppx;
    let device_y = (border.y + border_top + content_y) * dppx;
    let device_w = (device_x + content_w * dppx).round() - device_x.round();
    let device_h = (device_y + content_h * dppx).round() - device_y.round();
    let (observed_w, observed_h) = match observed_box {
        b"border-box" => (border.w, border.h),
        b"device-pixel-content-box" => (device_w, device_h),
        _ => (content_w, content_h),
    };
    let content_size = size_list(scope, content_w, content_h);
    let border_size = size_list(scope, border.w, border.h);
    let device_size = size_list(scope, device_w, device_h);
    let entry = scope.new_object();
    set(scope, &entry, "target", target.clone());
    let content_rect = ffi::dom_rect(
        scope,
        Rect {
            x: content_x,
            y: content_y,
            w: content_w,
            h: content_h,
        },
    );
    set(scope, &entry, "contentRect", content_rect);
    set(scope, &entry, "borderBoxSize", border_size);
    set(scope, &entry, "contentBoxSize", content_size);
    set(scope, &entry, "devicePixelContentBoxSize", device_size);
    (observed_w, observed_h, entry)
}

fn last_size(scope: &mut Scope<'_>, widths: &Value, heights: &Value, i: u32) -> Option<(f64, f64)> {
    let width = scope.get_index(widths, i).ok()?;
    let height = scope.get_index(heights, i).ok()?;
    if !width.is_number() || !height.is_number() {
        return None;
    }
    let width = scope.to_number(&width).ok()?;
    let height = scope.to_number(&height).ok()?;
    Some((width, height))
}

fn deliver(scope: &mut Scope<'_>, observer: &Value) {
    let callback = get(scope, observer, "__cb");
    let targets = get(scope, observer, "__targets");
    let widths = get(scope, observer, "__lastWidths");
    let heights = get(scope, observer, "__lastHeights");
    let boxes = get(scope, observer, "__boxes");
    if !scope.is_function(&callback)
        || !targets.is_array()
        || !widths.is_array()
        || !heights.is_array()
        || !boxes.is_array()
    {
        return;
    }
    let length = crate::array_length(scope, &targets);
    let mut entries: Option<Value> = None;
    let mut count = 0u32;
    for i in 0..length {
        let target = scope
            .get_index(&targets, i)
            .unwrap_or_else(|_| Value::undefined());
        let box_value = scope
            .get_index(&boxes, i)
            .unwrap_or_else(|_| Value::undefined());
        let box_name =
            crate::text_of(scope, &box_value).unwrap_or_else(|_| b"content-box".to_vec());
        let (w, h, entry) = build_entry(scope, &target, &box_name);
        let changed = last_size(scope, &widths, &heights, i)
            .is_none_or(|(last_w, last_h)| w != last_w || h != last_h);
        if changed {
            set_index(scope, &widths, i, Value::number(w));
            set_index(scope, &heights, i, Value::number(h));
            let array = entries.get_or_insert_with(|| scope.new_array()).clone();
            set_index(scope, &array, count, entry);
            count += 1;
        }
    }
    if let (Some(entries), true) = (entries, count > 0) {
        let args = [entries, observer.clone()];
        let _ = ffi::call_observer(
            scope,
            &callback,
            observer,
            &args,
            Some(c"ResizeObserver"),
            false,
        );
    }
}

pub(crate) fn tick(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    if !ffi::has_main_context(js) || page.ticking.get() {
        return;
    }
    page.ticking.set(true);
    ffi::with_main_context(js, |scope| {
        let mut index = 0;
        loop {
            let Some(observer) = page.resize.borrow().get(index).cloned() else {
                break;
            };
            index += 1;
            deliver(scope, &observer);
        }
    });
    page.ticking.set(false);
}

pub(crate) fn unobserve(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    element_target(scope, args)?;
    let targets = get(scope, this, "__targets");
    let widths = get(scope, this, "__lastWidths");
    let heights = get(scope, this, "__lastHeights");
    let boxes = get(scope, this, "__boxes");
    if targets.is_array() {
        let length = crate::array_length(scope, &targets);
        let kept = scope.new_array();
        let kept_widths = scope.new_array();
        let kept_heights = scope.new_array();
        let kept_boxes = scope.new_array();
        let mut k = 0u32;
        let wanted = ffi::node_address(&args[0]);
        let item = |scope: &mut Scope<'_>, array: &Value, i: u32| {
            if array.is_array() {
                scope
                    .get_index(array, i)
                    .unwrap_or_else(|_| Value::undefined())
            } else {
                Value::undefined()
            }
        };
        for i in 0..length {
            let target = scope
                .get_index(&targets, i)
                .unwrap_or_else(|_| Value::undefined());
            let w = item(scope, &widths, i);
            let h = item(scope, &heights, i);
            let b = item(scope, &boxes, i);
            if ffi::node_address(&target) != wanted {
                set_index(scope, &kept, k, target);
                set_index(scope, &kept_widths, k, w);
                set_index(scope, &kept_heights, k, h);
                set_index(scope, &kept_boxes, k, b);
                k += 1;
            }
        }
        set(scope, this, "__targets", kept);
        set(scope, this, "__lastWidths", kept_widths);
        set(scope, this, "__lastHeights", kept_heights);
        set(scope, this, "__boxes", kept_boxes);
    }
    Ok(Value::undefined())
}

pub(crate) fn disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    for key in ["__targets", "__lastWidths", "__lastHeights", "__boxes"] {
        set(scope, this, key, Value::undefined());
    }
    if let Some(page) = page(ffi::js_of(scope)) {
        let removed = {
            let mut list = page.resize.borrow_mut();
            list.iter()
                .position(|o| o.same_object(this))
                .map(|index| list.remove(index))
        };
        drop(removed);
    }
    Ok(Value::undefined())
}

const METHODS: [(&str, u32, NativeFn); 3] = [
    ("observe", 2, observe),
    ("unobserve", 1, unobserve),
    ("disconnect", 0, disconnect),
];

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let Some(callback) = args.first().filter(|cb| scope.is_function(cb)).cloned() else {
        return Err(scope.type_error("ResizeObserver callback must be callable"));
    };
    let proto = scope
        .get(new_target, "prototype")
        .ok()
        .filter(Value::is_object);
    let object = match &proto {
        Some(proto) => {
            let object = scope.new_object_with_proto(proto);
            if !matches!(scope.has_property(proto, "observe"), Ok(true)) {
                for (name, arity, f) in METHODS {
                    bind(scope, proto, name, arity, f);
                }
            }
            object
        }
        None => {
            let object = scope.new_object();
            for (name, arity, f) in METHODS {
                bind(scope, &object, name, arity, f);
            }
            object
        }
    };
    set(scope, &object, "__cb", callback);
    for (name, arity, f) in METHODS {
        bind_if_not_callable(scope, &object, name, arity, f);
    }
    Ok(object)
}
