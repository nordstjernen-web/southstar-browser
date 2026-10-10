//! Southstar — the window's own browsing-context members: window, self, frames, length, top and parent, the window state values, the viewport metrics and the print/stop/focus actions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::{bind, named, set};

const WINDOW_ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

fn ignore(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn get_global(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(scope.global())
}

fn get_length(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let doc = ffi::current_document_for(scope, this);
    let count = named::child_frame_count(doc);
    Ok(Value::int64(i64::from(count)))
}

fn accessor(scope: &mut Scope<'_>, object: &Value, name: &str, getter: NativeFn) {
    let get = scope.function(&format!("get {name}"), 0, getter);
    let set = scope.function(&format!("set {name}"), 1, ignore);
    let _ = scope.define_accessor(object, name, Some(&get), Some(&set), WINDOW_ACCESSOR);
}

pub(crate) fn install_browsing_context(scope: &mut Scope<'_>, global: &Value) {
    accessor(scope, global, "window", get_global);
    for name in ["self", "top", "parent", "globalThis"] {
        set(scope, global, name, global.clone());
    }
    let _ = scope.define_to_string_tag(global, "Window");
    accessor(scope, global, "frames", get_global);
    accessor(scope, global, "length", get_length);
}

pub(crate) fn install_state(scope: &mut Scope<'_>, js: Js, global: &Value) {
    for name in ["screenX", "screenY", "screenLeft", "screenTop"] {
        set(scope, global, name, Value::int(0));
    }
    let url = if js.is_null() {
        Vec::new()
    } else {
        ffi::current_url(js)
    };
    set(
        scope,
        global,
        "isSecureContext",
        Value::boolean(url.starts_with(b"https:")),
    );
    let origin = scope.string_from_bytes(&url);
    set(scope, global, "origin", origin);
    for name in ["name", "status"] {
        let empty = scope.string("");
        set(scope, global, name, empty);
    }
    set(scope, global, "closed", Value::boolean(false));
    set(scope, global, "opener", Value::null());
    set(scope, global, "event", Value::undefined());
}

fn action(scope: &mut Scope<'_>, name: &str) -> Result<Value, Value> {
    ffi::window_action(ffi::js_of(scope), name);
    Ok(Value::undefined())
}

fn print(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    action(scope, "print")
}

fn stop(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    action(scope, "stop")
}

fn focus(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    action(scope, "focus")
}

pub(crate) fn install_actions(scope: &mut Scope<'_>, global: &Value) {
    for (name, arity) in [
        ("close", 0),
        ("blur", 0),
        ("moveTo", 2),
        ("moveBy", 2),
        ("resizeTo", 2),
        ("resizeBy", 2),
    ] {
        bind(scope, global, name, arity, ignore);
    }
    bind(scope, global, "print", 0, print);
    bind(scope, global, "stop", 0, stop);
    bind(scope, global, "focus", 0, focus);
}

fn viewport_metric(value: f64, fallback: i32) -> i32 {
    if !value.is_finite() || value <= 0.0 {
        return fallback;
    }
    (value + 0.5) as i32
}

pub(crate) fn sync_metrics(scope: &mut Scope<'_>) {
    let width = viewport_metric(ffi::viewport_width(), 1000);
    let height = viewport_metric(ffi::viewport_height(), 800);
    let (screen_width, screen_height) = ffi::screen_size();
    let global = scope.global();
    set(scope, &global, "innerWidth", Value::int(width));
    set(scope, &global, "innerHeight", Value::int(height));
    set(
        scope,
        &global,
        "outerWidth",
        Value::int((width + 16).min(screen_width)),
    );
    set(
        scope,
        &global,
        "outerHeight",
        Value::int((height + 95).min(screen_height)),
    );
    set(
        scope,
        &global,
        "devicePixelRatio",
        Value::number(ffi::device_pixel_ratio()),
    );
}
