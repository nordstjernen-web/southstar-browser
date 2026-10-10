//! Southstar — ClipboardItem: a typed bag of clipboard representations whose getType resolves to the stored value.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::{make_ctor, resolved};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

fn rejected(scope: &mut Scope<'_>, name: Option<&str>, message: &str) -> Result<Value, Value> {
    let error = scope.new_error();
    if let Some(name) = name {
        let name = scope.string(name);
        let _ = scope.define(&error, "name", name, Attributes::METHOD);
        let message = scope.string(message);
        let _ = scope.define(&error, "message", message, Attributes::METHOD);
    } else {
        let message = scope.string(message);
        let _ = scope.set(&error, "message", message);
    }
    scope.rejected_promise(&error)
}

fn stored(scope: &mut Scope<'_>, item: &Value, kind: &Value) -> Option<Value> {
    let store = scope.get(item, "__data").ok()?;
    if !store.is_object() {
        return None;
    }
    let kind = scope.to_string(kind).ok()?;
    scope
        .get(&store, &kind)
        .ok()
        .filter(|value| !value.is_undefined())
}

fn get_type(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(kind) = args.first() else {
        return rejected(scope, None, "not supported");
    };
    match stored(scope, this, kind) {
        Some(value) => resolved(scope, value),
        None => rejected(scope, Some("NotFoundError"), "The type was not found"),
    }
}

fn construct(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let item = scope.new_object();
    let types = scope.new_array();
    let data = scope.new_object();
    if let Some(items) = args.first().filter(|items| items.is_object()) {
        let keys = scope.own_enumerable_keys(items).unwrap_or_default();
        for (index, key) in keys.into_iter().enumerate() {
            let value = scope.get_key(items, &key)?;
            scope.set_key(&data, &key, value)?;
            scope.set_index(&types, index as u32, key)?;
        }
    }
    scope.set(&item, "types", types)?;
    let style = scope.string("unspecified");
    scope.set(&item, "presentationStyle", style)?;
    scope.define(&item, "__data", data, HIDDEN)?;
    let get_type = scope.function("getType", 1, get_type);
    scope.set(&item, "getType", get_type)?;
    Ok(item)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let ctor = make_ctor(scope, "ClipboardItem", 1, construct);
    let _ = scope.set(global, "ClipboardItem", ctor);
}
