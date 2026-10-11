//! Southstar — the Storage host class: its named properties (get, set, delete, define and enumeration) over the storage areas.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyDefinition, PropertyKey, Scope, Value,
};

use crate::store::Area;
use crate::{JsResult, binding};

pub(crate) static STORAGE_CLASS: HostClass = HostClass::new(
    c"Storage",
    ClassHooks {
        get_own_property: Some(get_own),
        own_keys: Some(own_keys),
        set: Some(set),
        delete: Some(delete),
        define_own_property: Some(define_own),
        ..ClassHooks::NONE
    },
);

pub(crate) fn c_text(text: &str) -> &[u8] {
    let bytes = text.as_bytes();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

pub(crate) fn area_tag(value: &Value) -> i32 {
    value
        .with_instance(&STORAGE_CLASS, |area: &i32| *area)
        .unwrap_or(0)
}

pub(crate) fn area_of(value: &Value) -> Option<Area> {
    Area::from_tag(area_tag(value))
}

pub(crate) fn new_storage(scope: &mut Scope<'_>, area: i32) -> JsResult {
    scope.new_instance_with(&STORAGE_CLASS, None, area)
}

pub(crate) fn register(scope: &mut Scope<'_>, prototype: Value) {
    scope.register_class(&STORAGE_CLASS);
    scope.set_class_prototype(&STORAGE_CLASS, prototype);
}

fn key_name(scope: &mut Scope<'_>, key: &PropertyKey) -> Option<String> {
    key.to_c_bytes(scope)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn get_own(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> JsResult<Option<OwnProperty>> {
    if area_tag(obj) == 0 {
        return Ok(None);
    }
    let Some(name) = key_name(scope, key) else {
        return Ok(None);
    };
    let Some(value) = binding::named_value(scope, obj, &name) else {
        return Ok(None);
    };
    let value = scope.string_from_bytes(c_text(&value));
    Ok(Some(OwnProperty::data(value, true, true, true)))
}

fn set(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    value: &Value,
    _receiver: &Value,
) -> JsResult<bool> {
    if area_tag(obj) == 0 {
        return Ok(false);
    }
    let Some(name) = key_name(scope, key) else {
        return Ok(false);
    };
    binding::named_set(scope, obj, &name, value).map(|()| true)
}

fn delete(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> JsResult<bool> {
    if area_tag(obj) == 0 {
        return Ok(false);
    }
    let Some(name) = key_name(scope, key) else {
        return Ok(false);
    };
    binding::named_delete(scope, obj, &name);
    Ok(true)
}

fn define_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    definition: &PropertyDefinition,
) -> JsResult<bool> {
    let is_symbol = key.is_symbol(scope);
    if area_tag(obj) == 0
        || is_symbol
        || definition.getter.is_object()
        || definition.setter.is_object()
        || definition.value.is_undefined()
    {
        return scope.define_ordinary(obj, key, definition);
    }
    let Some(name) = key_name(scope, key) else {
        return Err(scope.type_error("out of memory"));
    };
    binding::named_set(scope, obj, &name, &definition.value).map(|()| true)
}

fn own_keys(scope: &mut Scope<'_>, obj: &Value) -> JsResult<Vec<PropertyKey>> {
    if area_tag(obj) == 0 {
        return Ok(Vec::new());
    }
    let names = binding::names(scope, obj);
    Ok(names
        .iter()
        .map(|name| scope.property_key(c_text(name)))
        .collect())
}
