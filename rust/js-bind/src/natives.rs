//! Southstar — the stock natives shared by many bindings: no-ops, empty answers and already-settled promises.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

pub fn resolved(scope: &mut Scope<'_>, value: Value) -> Result<Value, Value> {
    let (promise, resolve, _reject) = scope.new_promise()?;
    let _ = scope.call(&resolve, &Value::undefined(), &[value]);
    Ok(promise)
}

pub fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

pub fn null(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::null())
}

pub fn empty_array(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(scope.new_array())
}

pub fn resolved_undefined(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    resolved(scope, Value::undefined())
}

pub fn resolved_false(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    resolved(scope, Value::boolean(false))
}

pub fn resolved_empty_array(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let array = scope.new_array();
    resolved(scope, array)
}
