//! Southstar — the live collection host class: HTMLCollection, NodeList and NamedNodeMap instances, their indexed and named properties and enumeration.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;
use std::rc::Rc;

use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyDefinition, PropertyKey, Scope, Value,
};

use crate::ffi;
use crate::live::{self, Key, Live};

pub(crate) static LIVE_CLASS: HostClass = HostClass::new(
    c"HTMLCollection",
    ClassHooks {
        get_own_property: Some(get_own),
        own_keys: Some(own_keys),
        delete: Some(delete),
        define_own_property: Some(define_own),
        ..ClassHooks::NONE
    },
);

pub fn register(scope: &mut Scope<'_>) {
    scope.register_class(&LIVE_CLASS);
    live::install_protos(scope);
    let prototype = live::proto(ffi::js_of(scope), 0).unwrap_or_else(Value::undefined);
    scope.set_class_prototype(&LIVE_CLASS, prototype);
}

pub(crate) fn new_live(scope: &mut Scope<'_>, back: Live) -> Result<Value, Value> {
    scope.new_traced_instance(&LIVE_CLASS, None, Rc::new(back))
}

fn name_of(scope: &mut Scope<'_>, key: &PropertyKey) -> Option<CString> {
    key.to_c_bytes(scope)
        .map(|bytes| CString::new(bytes).unwrap_or_default())
}

fn get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(back) = ffi::live_of(obj) else {
        return Ok(None);
    };
    let index = key.array_index(scope);
    let name = match index {
        Some(_) => None,
        None => {
            if !back.named_access() {
                return Ok(None);
            }
            let Some(name) = name_of(scope, key) else {
                return Ok(None);
            };
            let mut proto_has = name.as_bytes() == b"length";
            if !proto_has {
                let proto = scope
                    .get_prototype(obj)
                    .unwrap_or_else(|_| Value::undefined());
                proto_has = proto.is_object() && scope.has_key(&proto, key).unwrap_or(false);
            }
            if proto_has {
                return Ok(None);
            }
            Some(name)
        }
    };
    let found = match (index, &name) {
        (Some(index), _) => live::get_own(scope, &back, Key::Index(index)),
        (None, Some(name)) => live::get_own(scope, &back, Key::Name(name)),
        (None, None) => None,
    };
    Ok(found.map(|value| OwnProperty::data(value, false, index.is_some(), true)))
}

fn own_keys(scope: &mut Scope<'_>, obj: &Value) -> Result<Vec<PropertyKey>, Value> {
    let Some(back) = ffi::live_of(obj) else {
        return Ok(Vec::new());
    };
    let mut named: Vec<Vec<u8>> = Vec::new();
    let length = live::own_names(scope, &back, obj, |name| {
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        named.push(name[..end].to_vec());
    });
    let mut keys: Vec<PropertyKey> = (0..length).map(|i| scope.index_key(i)).collect();
    keys.extend(named.iter().map(|name| scope.property_key(name)));
    Ok(keys)
}

fn delete(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> Result<bool, Value> {
    let Some(back) = ffi::live_of(obj) else {
        return Ok(true);
    };
    if let Some(index) = key.array_index(scope) {
        return Ok(live::deletable(scope, &back, Key::Index(index)));
    }
    let Some(name) = name_of(scope, key) else {
        return Ok(true);
    };
    Ok(live::deletable(scope, &back, Key::Name(&name)))
}

fn define_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    definition: &PropertyDefinition,
) -> Result<bool, Value> {
    if let Some(back) = ffi::live_of(obj) {
        if key.array_index(scope).is_some() {
            return Ok(false);
        }
        if let Some(name) = name_of(scope, key)
            && live::named_exists(scope, &back, Key::Name(&name))
        {
            if definition.throws() {
                return Err(scope.type_error("Cannot define property on this object"));
            }
            return Ok(false);
        }
    }
    scope.define_ordinary(obj, key, definition)
}
