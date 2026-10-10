//! Southstar — the CSS Font Loading bindings: FontFace, document.fonts and the promises that settle once pending web fonts have loaded.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod font_face;
mod font_set;
mod ready;

use southstar_js_engine::{NativeFn, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn c_string(scope: &mut Scope<'_>, value: &Value) -> Value {
    let mut bytes = scope.to_bytes(value).unwrap_or_default();
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    scope.string_from_bytes(&bytes)
}

pub(crate) fn undefined(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn always_true(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(true))
}
