//! Southstar — argument, property and promise helpers shared by the WebGPU bindings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{NativeFn, Scope, Value};

pub(crate) type JsResult = Result<Value, Value>;

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn defined(args: &[Value], index: usize) -> Option<&Value> {
    args.get(index).filter(|v| !v.is_undefined())
}

pub(crate) fn host<T: 'static>(value: &Value) -> Option<Rc<T>> {
    value.with_host(|h: &Rc<T>| h.clone())
}

pub(crate) fn prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn index(scope: &mut Scope<'_>, object: &Value, i: u32) -> Value {
    scope
        .get_index(object, i)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn string_of(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    if value.is_string() {
        scope.to_string(value).ok()
    } else {
        None
    }
}

pub(crate) fn str_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<String> {
    let value = prop(scope, object, key);
    string_of(scope, &value)
}

pub(crate) fn u32_of(scope: &mut Scope<'_>, value: &Value) -> u32 {
    scope.to_int32(value).unwrap_or(0) as u32
}

pub(crate) fn i32_of(scope: &mut Scope<'_>, value: &Value) -> i32 {
    scope.to_int32(value).unwrap_or(0)
}

pub(crate) fn i64_of(scope: &mut Scope<'_>, value: &Value) -> i64 {
    scope.to_int64(value).unwrap_or(0)
}

pub(crate) fn f64_of(scope: &mut Scope<'_>, value: &Value) -> f64 {
    scope.to_number(value).unwrap_or(f64::NAN)
}

pub(crate) fn u32_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> u32 {
    let value = prop(scope, object, key);
    u32_of(scope, &value)
}

pub(crate) fn i64_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> i64 {
    let value = prop(scope, object, key);
    i64_of(scope, &value)
}

pub(crate) fn defined_u32_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<u32> {
    let value = prop(scope, object, key);
    (!value.is_undefined()).then(|| u32_of(scope, &value))
}

pub(crate) fn array_len(scope: &mut Scope<'_>, array: &Value) -> u32 {
    u32_prop(scope, array, "length")
}

pub(crate) fn uint(value: u32) -> Value {
    Value::int64(i64::from(value))
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    scope.set(object, key, value).ok();
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn, arity: u32) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn resolved(scope: &mut Scope<'_>, value: Value) -> JsResult {
    let (promise, resolve, _reject) = scope.new_promise()?;
    scope.call(&resolve, &Value::undefined(), &[value]).ok();
    Ok(promise)
}

pub(crate) fn rejected(scope: &mut Scope<'_>, message: &str) -> JsResult {
    let (promise, _resolve, reject) = scope.new_promise()?;
    let error = scope.new_error();
    set_str(scope, &error, "message", message);
    scope.call(&reject, &Value::undefined(), &[error]).ok();
    Ok(promise)
}

pub(crate) fn feature_set(scope: &mut Scope<'_>) -> Value {
    let global = scope.global();
    let set = scope
        .get(&global, "Set")
        .and_then(|ctor| scope.construct(&ctor, &[]));
    set.unwrap_or_else(|_| scope.new_object())
}
