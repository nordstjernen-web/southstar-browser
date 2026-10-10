//! Southstar — MessagePort, MessageChannel and BroadcastChannel, and the dedicated and service workers that carry messages between runtimes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod broadcast;
mod ffi;
mod ports;
mod scope;
mod service;
mod worker;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

pub(crate) type JsResult = Result<Value, Value>;

pub(crate) const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) const PLAIN: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub(crate) fn get_index(scope: &mut Scope<'_>, array: &Value, index: u32) -> Value {
    scope
        .get_index(array, index)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set_index(scope: &mut Scope<'_>, array: &Value, index: u32, value: Value) {
    let _ = scope.set_index(array, index, value);
}

pub(crate) fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope.to_number(&length).map_or(0, |n| n as u32)
}

pub(crate) fn push(scope: &mut Scope<'_>, array: &Value, value: Value) {
    let at = length(scope, array);
    set_index(scope, array, at, value);
}

pub(crate) fn flag(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn text(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    scope.to_string(value).ok()
}

pub(crate) fn proto_of(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let ctor = get(scope, global, name);
    if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn bind_if_not_callable(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: NativeFn,
) {
    let current = scope.get(object, name).ok();
    if !current.is_some_and(|current| scope.is_function(&current)) {
        bind(scope, object, name, arity, f);
    }
}

pub(crate) fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}
