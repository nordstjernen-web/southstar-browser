//! Southstar — the event core: Event and its subclasses, EventTarget listeners and the dispatch algorithm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod attrs;
mod ctors;
mod event;
mod ffi;
mod install;

use southstar_dom::Node;
use southstar_js_engine::{Attributes, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};
pub(crate) const WRITABLE_CONFIGURABLE: Attributes = Attributes::METHOD;
pub(crate) const ENUMERABLE_CONFIGURABLE: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};
pub(crate) const ALL: Attributes = Attributes {
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

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn bool_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> (bool, bool) {
    let value = get(scope, object, key);
    let defined = !value.is_undefined();
    (defined && scope.to_bool(&value), defined)
}

pub(crate) fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: i32) -> i32 {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        return fallback;
    }
    scope.to_int32(&value).unwrap_or(0)
}

pub(crate) fn number_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: f64) -> f64 {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        return fallback;
    }
    scope.to_number(&value).unwrap_or(f64::NAN)
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn interface_prototype(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let ctor = get(scope, global, name);
    if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}
