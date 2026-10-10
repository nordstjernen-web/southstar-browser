//! Southstar — the media bindings: HTMLMediaElement, media type support, the MSE natives, text tracks, the Web Audio API surface over rust/webaudio and the EME entry points.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod audio;
mod element;
mod eme;
mod ffi;
mod hooks;
mod mse;
mod support;
mod tracks;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

const PROTOTYPE_ONLY_WRITABLE: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
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

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope
        .to_number(&length)
        .map(|n| southstar_js_engine::int64_modulo(n) as u32)
        .unwrap_or(0)
}

pub(crate) fn make_ctor(scope: &mut Scope<'_>, name: &str, arity: u32, f: NativeFn) -> Value {
    let function = scope.constructor_or_function(name, arity, f);
    let proto = scope.new_object();
    let _ = scope.define(&proto, "constructor", function.clone(), Attributes::METHOD);
    let _ = scope.define_to_string_tag(&proto, name);
    let _ = scope.define(&function, "prototype", proto, PROTOTYPE_ONLY_WRITABLE);
    function
}

pub(crate) fn defined_error(
    scope: &mut Scope<'_>,
    name: &str,
    message: &str,
    code: Option<i32>,
) -> Value {
    let error = scope.new_error();
    let name = scope.string(name);
    let _ = scope.define(&error, "name", name, Attributes::METHOD);
    let message = scope.string(message);
    let _ = scope.define(&error, "message", message, Attributes::METHOD);
    if let Some(code) = code {
        let _ = scope.define(&error, "code", Value::int(code), Attributes::METHOD);
    }
    error
}

pub(crate) fn assigned_error(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    let error = scope.new_error();
    set_str(scope, &error, "name", name);
    set_str(scope, &error, "message", message);
    set(scope, &error, "code", Value::int(9));
    error
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    dom_exception_code(scope, name, message, 9)
}

pub(crate) fn dom_exception_code(
    scope: &mut Scope<'_>,
    name: &str,
    message: &str,
    code: i32,
) -> Value {
    let error = defined_error(scope, name, message, Some(code));
    let global = scope.global();
    let constructor = get(scope, &global, "DOMException");
    if constructor.is_object() {
        let proto = get(scope, &constructor, "prototype");
        if proto.is_object() {
            let _ = scope.set_prototype(&error, &proto);
        }
    }
    error
}

pub(crate) fn resolved(scope: &mut Scope<'_>, value: Value) -> JsResult {
    let (promise, resolve, _) = scope.new_promise()?;
    let _ = scope.call(&resolve, &Value::undefined(), &[value]);
    Ok(promise)
}

pub(crate) fn rejected(scope: &mut Scope<'_>, reason: Value) -> JsResult {
    scope.rejected_promise(&reason)
}
