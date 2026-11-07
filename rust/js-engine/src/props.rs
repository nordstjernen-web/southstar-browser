//! Southstar — the forgiving property, argument and binding helpers the bindings share: reads that fall back to undefined and writes that ignore errors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::{NativeFn, Scope, Value};

pub fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub fn number_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> f64 {
    args.get(index)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

pub fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

pub fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub fn get_index(scope: &mut Scope<'_>, object: &Value, index: u32) -> Value {
    scope
        .get_index(object, index)
        .unwrap_or_else(|_| Value::undefined())
}

pub fn global_get(scope: &mut Scope<'_>, key: &str) -> Value {
    let global = scope.global();
    get(scope, &global, key)
}

pub fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> i32 {
    let value = get(scope, object, key);
    scope.to_int32(&value).unwrap_or(0)
}

pub fn string_value(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    if value.is_string() {
        scope.to_string(value).ok()
    } else {
        None
    }
}

pub fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub fn set_index(scope: &mut Scope<'_>, object: &Value, index: u32, value: Value) {
    let _ = scope.set_index(object, index, value);
}

pub fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub fn set_bytes(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    set(scope, object, key, value);
}

pub fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope.to_int32(&length).map_or(0, |n| n as u32)
}

pub fn number_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope.to_number(&length).map_or(0, |n| n as u32)
}

pub fn interface_prototype(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let constructor = get(scope, global, name);
    if constructor.is_object() {
        get(scope, &constructor, "prototype")
    } else {
        Value::undefined()
    }
}

pub fn dom_exception(scope: &mut Scope<'_>, name: &CStr, code: i32, message: &CStr) -> Value {
    scope.dom_exception_code(name.to_bytes(), code, message.to_bytes())
}

pub fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub fn bind_if_not_callable(
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
