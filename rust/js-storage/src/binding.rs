//! Southstar — the Storage interface: getItem/setItem/removeItem/clear/key/length, the named-property behaviour behind its exotic hooks and the window installer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::store::{Area, QuotaExceeded, Write};
use crate::{JsResult, events, ffi};

const BUILTIN_NAMES: [&str; 7] = [
    "length",
    "constructor",
    "getItem",
    "setItem",
    "removeItem",
    "clear",
    "key",
];

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn quota_exceeded(scope: &mut Scope<'_>) -> Value {
    let message = "Storage quota exceeded";
    let global = scope.global();
    if let Ok(ctor) = scope.get(&global, "QuotaExceededError")
        && scope.is_function(&ctor)
    {
        let text = scope.string(message);
        if let Ok(error) = scope.construct(&ctor, &[text]) {
            return error;
        }
    }
    let error = scope.new_error();
    let name = scope.string("QuotaExceededError");
    let text = scope.string(message);
    let _ = scope.define(&error, "name", name, Attributes::METHOD);
    let _ = scope.define(&error, "message", text, Attributes::METHOD);
    let _ = scope.define(&error, "code", Value::int(22), Attributes::METHOD);
    let _ = scope.define(&error, "requested", Value::null(), Attributes::METHOD);
    let _ = scope.define(&error, "quota", Value::null(), Attributes::METHOD);
    error
}

fn string_or_null(scope: &mut Scope<'_>, text: Option<String>) -> Value {
    text.map_or_else(Value::null, |text| scope.string(&text))
}

fn write(
    scope: &mut Scope<'_>,
    storage: &Value,
    area: Area,
    key: &str,
    value: &str,
) -> JsResult<()> {
    let js = ffi::js_of(scope);
    let Some(page) = crate::page(js) else {
        return Ok(());
    };
    let outcome = page.storage.borrow_mut().set(area, key, value);
    match outcome {
        Err(QuotaExceeded) => Err(quota_exceeded(scope)),
        Ok(Write::Unchanged) => Ok(()),
        Ok(Write::Changed(old)) => {
            events::fire(scope, storage, Some(key), old.as_deref(), Some(value));
            Ok(())
        }
    }
}

fn remove(scope: &mut Scope<'_>, storage: &Value, area: Area, key: &str) {
    let Some(page) = crate::page(ffi::js_of(scope)) else {
        return;
    };
    let old = page.storage.borrow_mut().remove(area, key);
    if let Some(old) = old {
        events::fire(scope, storage, Some(key), Some(&old), None);
    }
}

fn read<R>(
    scope: &Scope<'_>,
    this: &Value,
    f: impl FnOnce(&crate::store::Storage, Area) -> R,
) -> Option<R> {
    let area = ffi::area_of(this)?;
    let page = crate::page(ffi::js_of(scope))?;
    let storage = page.storage.borrow();
    Some(f(&storage, area))
}

fn get_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'getItem' on 'Storage': 1 argument required, but only 0 present.",
        ));
    }
    if ffi::area_of(this).is_none() {
        return Ok(Value::null());
    }
    let Ok(key) = scope.to_string(&args[0]) else {
        return Ok(Value::null());
    };
    let value = read(scope, this, |storage, area| storage.get(area, &key)).flatten();
    Ok(string_or_null(scope, value))
}

fn set_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Err(scope.type_error(&format!(
            "Failed to execute 'setItem' on 'Storage': 2 arguments required, but only {} present.",
            args.len()
        )));
    }
    let Some(area) = ffi::area_of(this) else {
        return Ok(Value::undefined());
    };
    let key = scope.to_string(&args[0])?;
    let value = scope.to_string(&args[1])?;
    write(scope, this, area, &key, &value)?;
    Ok(Value::undefined())
}

fn remove_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'removeItem' on 'Storage': 1 argument required, but only 0 present.",
        ));
    }
    let Some(area) = ffi::area_of(this) else {
        return Ok(Value::undefined());
    };
    if let Ok(key) = scope.to_string(&args[0]) {
        remove(scope, this, area, &key);
    }
    Ok(Value::undefined())
}

fn clear(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(area) = ffi::area_of(this) else {
        return Ok(Value::undefined());
    };
    let Some(page) = crate::page(ffi::js_of(scope)) else {
        return Ok(Value::undefined());
    };
    let cleared = page.storage.borrow_mut().clear(area);
    if cleared {
        events::fire(scope, this, None, None, None);
    }
    Ok(Value::undefined())
}

fn key(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'key' on 'Storage': 1 argument required, but only 0 present.",
        ));
    }
    if ffi::area_of(this).is_none() {
        return Ok(Value::null());
    }
    let index = scope.to_int32(&arg(args, 0)).unwrap_or(0);
    let Ok(index) = usize::try_from(index) else {
        return Ok(Value::null());
    };
    let name = read(scope, this, |storage, area| storage.key(area, index)).flatten();
    Ok(string_or_null(scope, name))
}

fn length(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let len = read(scope, this, |storage, area| storage.len(area)).unwrap_or(0);
    Ok(Value::int(i32::try_from(len).unwrap_or(i32::MAX)))
}

pub(crate) fn install_proto(scope: &mut Scope<'_>, proto: &Value) {
    let methods: [(&str, u32, southstar_js_engine::NativeFn); 5] = [
        ("getItem", 1, get_item),
        ("setItem", 2, set_item),
        ("removeItem", 1, remove_item),
        ("clear", 0, clear),
        ("key", 1, key),
    ];
    for (name, arity, f) in methods {
        let function = scope.function(name, arity, f);
        let _ = scope.define(proto, name, function, Attributes::METHOD);
    }
    let getter = scope.function("get length", 0, length);
    let _ = scope.define_accessor(
        proto,
        "length",
        Some(&getter),
        None,
        Attributes::CONFIGURABLE,
    );
}

pub(crate) fn install_window(scope: &mut Scope<'_>, global: &Value) {
    for (name, area) in [
        ("localStorage", Area::LOCAL_TAG),
        ("sessionStorage", Area::SESSION_TAG),
    ] {
        let storage = ffi::new_storage(scope, area);
        let _ = scope.set(global, name, storage);
    }
}

pub(crate) fn named_value(scope: &mut Scope<'_>, storage: &Value, name: &str) -> Option<String> {
    if BUILTIN_NAMES.contains(&name) {
        return None;
    }
    let value = read(scope, storage, |items, area| items.get(area, name)).flatten()?;
    let proto = scope.get_prototype(storage).ok()?;
    if proto.is_object() && scope.has_property(&proto, name).unwrap_or(false) {
        return None;
    }
    Some(value)
}

pub(crate) fn named_set(
    scope: &mut Scope<'_>,
    storage: &Value,
    name: &str,
    value: &Value,
) -> JsResult<()> {
    let Some(area) = ffi::area_of(storage) else {
        return Ok(());
    };
    let value = scope.to_string(value)?;
    write(scope, storage, area, name, &value)
}

pub(crate) fn named_delete(scope: &mut Scope<'_>, storage: &Value, name: &str) {
    if let Some(area) = ffi::area_of(storage) {
        remove(scope, storage, area, name);
    }
}

pub(crate) fn names(scope: &Scope<'_>, storage: &Value) -> Vec<String> {
    read(scope, storage, |items, area| items.keys(area)).unwrap_or_default()
}
