//! Southstar — the DOMException values the bindings throw or reject with, built on the realm's DOMException interface.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

const ABORT_MESSAGE: &str = "The operation was aborted.";

const MEMBER: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

fn dom_exception_interface(scope: &mut Scope<'_>) -> Option<Value> {
    let global = scope.global();
    scope
        .get(&global, "DOMException")
        .ok()
        .filter(Value::is_object)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    let error = scope.new_error();
    let name = scope.string_from_bytes(name);
    let _ = scope.define(&error, "name", name, MEMBER);
    let message = scope.string_from_bytes(message);
    let _ = scope.define(&error, "message", message, MEMBER);
    let _ = scope.define(&error, "code", Value::int(code), MEMBER);
    if let Some(interface) = dom_exception_interface(scope)
        && let Ok(prototype) = scope.get(&interface, "prototype")
        && prototype.is_object()
    {
        let _ = scope.set_prototype(&error, &prototype);
    }
    error
}

pub(crate) fn abort_error(scope: &mut Scope<'_>) -> Value {
    if let Some(interface) = dom_exception_interface(scope) {
        let args = [scope.string(ABORT_MESSAGE), scope.string("AbortError")];
        if let Ok(error) = scope.construct(&interface, &args)
            && error.is_object()
        {
            return error;
        }
    }
    let error = scope.new_error();
    let name = scope.string("AbortError");
    let _ = scope.set(&error, "name", name);
    let message = scope.string(ABORT_MESSAGE);
    let _ = scope.set(&error, "message", message);
    error
}
