//! Southstar — ElementInternals from attachInternals: the form-associated custom element surface and its CustomStateSet.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{JsResult, ffi};

fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

fn nothing(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

fn valid(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(true))
}

pub(crate) fn attach_internals(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let internals = scope.new_object();
    let shadow_root = ffi::shadow_root(scope, this);
    set(scope, &internals, "shadowRoot", shadow_root);
    set(scope, &internals, "form", Value::null());
    set(scope, &internals, "willValidate", Value::boolean(true));
    let message = scope.string("");
    set(scope, &internals, "validationMessage", message);
    let labels = scope.new_array();
    set(scope, &internals, "labels", labels);

    let validity = scope.new_object();
    set(scope, &validity, "valid", Value::boolean(true));
    set(scope, &internals, "validity", validity);

    let states = scope.new_object();
    bind(scope, &states, "add", 1, nothing);
    bind(scope, &states, "delete", 1, nothing);
    bind(scope, &states, "has", 1, nothing);
    bind(scope, &states, "clear", 0, nothing);
    bind(scope, &states, "forEach", 1, nothing);
    set(scope, &internals, "states", states);

    bind(scope, &internals, "setFormValue", 2, nothing);
    bind(scope, &internals, "setValidity", 3, nothing);
    bind(scope, &internals, "associateForm", 1, nothing);
    bind(scope, &internals, "_connect", 1, nothing);
    bind(scope, &internals, "_disconnect", 0, nothing);
    bind(scope, &internals, "checkValidity", 0, valid);
    bind(scope, &internals, "reportValidity", 0, valid);
    Ok(internals)
}

pub(crate) fn internals(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    attach_internals(scope, this, &[])
}

pub(crate) fn custom_state_set(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let global = scope.global();
    let set_ctor = scope.get(&global, "Set")?;
    scope.construct(&set_ctor, &[])
}
