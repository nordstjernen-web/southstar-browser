//! Southstar — the DOMTokenList, DOMStringMap and Attr host classes: token lists and their indexed properties, dataset and its named properties, and Attr nodes with their shared state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyKey, Scope, SetFlags, Trace, Value,
};

use crate::ffi::Shared;
use crate::{Element, c_string, dataset, ffi, token_list};

pub(crate) static TOKEN_LIST_CLASS: HostClass = HostClass::new(
    c"DOMTokenList",
    ClassHooks {
        get_own_property: Some(get_own),
        ..ClassHooks::NONE
    },
);

struct TokenList {
    element: Value,
    attr: &'static CStr,
}

impl Trace for TokenList {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.element);
    }
}

pub(crate) fn register(scope: &mut Scope<'_>, prototype: Value) {
    scope.register_class(&TOKEN_LIST_CLASS);
    scope.set_class_prototype(&TOKEN_LIST_CLASS, prototype);
}

pub(crate) fn prototype(scope: &mut Scope<'_>) -> Value {
    scope.class_prototype(&TOKEN_LIST_CLASS)
}

pub(crate) fn node_of(value: &Value) -> (Option<Element>, &'static CStr) {
    value
        .with_instance(&TOKEN_LIST_CLASS, |list: &TokenList| {
            (ffi::unwrap_node(&list.element), list.attr)
        })
        .unwrap_or((None, c"class"))
}

pub(crate) fn is_token_list(value: &Value) -> bool {
    value
        .with_instance(&TOKEN_LIST_CLASS, |_: &TokenList| ())
        .is_some()
}

pub(crate) fn new_token_list(
    scope: &mut Scope<'_>,
    element: &Value,
    attr: &'static CStr,
) -> Result<Value, Value> {
    let list = TokenList {
        element: element.clone(),
        attr,
    };
    scope.new_traced_instance(&TOKEN_LIST_CLASS, None, list)
}

fn get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(None);
    };
    let (node, attr) = node_of(obj);
    let Some(token) = node.and_then(|node| token_list::indexed_token(node, attr, &name)) else {
        return Ok(None);
    };
    let end = token.iter().position(|&b| b == 0).unwrap_or(token.len());
    let value = scope.string_from_bytes(&token[..end]);
    Ok(Some(OwnProperty::data(value, false, true, true)))
}

pub(crate) static DATASET_CLASS: HostClass = HostClass::new(
    c"DOMStringMap",
    ClassHooks {
        get_own_property: Some(dataset_get_own),
        own_keys: Some(dataset_own_keys),
        set: Some(dataset_set),
        delete: Some(dataset_delete),
        ..ClassHooks::NONE
    },
);

pub(crate) static ATTR_CLASS: HostClass = HostClass::new(c"Attr", ClassHooks::NONE);

struct Dataset {
    element: Value,
}

impl Trace for Dataset {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.element);
    }
}

pub(crate) fn register_dataset(scope: &mut Scope<'_>, prototype: Value) {
    scope.register_class(&DATASET_CLASS);
    scope.set_class_prototype(&DATASET_CLASS, prototype);
}

pub(crate) fn dataset_prototype(scope: &mut Scope<'_>) -> Value {
    scope.class_prototype(&DATASET_CLASS)
}

pub(crate) fn dataset_node(value: &Value) -> Option<Element> {
    value
        .with_instance(&DATASET_CLASS, |data: &Dataset| {
            ffi::unwrap_node(&data.element)
        })
        .flatten()
}

pub(crate) fn element_dataset(scope: &mut Scope<'_>, element: &Value) -> Result<Value, Value> {
    if !ffi::unwrap_node(element).is_some_and(dataset::has_dataset) {
        return Ok(Value::undefined());
    }
    let data = Dataset {
        element: element.clone(),
    };
    scope.new_traced_instance(&DATASET_CLASS, None, data)
}

fn dataset_get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(None);
    };
    let Some(value) = dataset_node(obj).and_then(|node| dataset::named_value(node, &name)) else {
        return Ok(None);
    };
    let value = scope.string_from_bytes(value.to_bytes());
    Ok(Some(OwnProperty::data(value, true, true, true)))
}

fn dataset_own_keys(scope: &mut Scope<'_>, obj: &Value) -> Result<Vec<PropertyKey>, Value> {
    let names = dataset_node(obj).map(dataset::names).unwrap_or_default();
    Ok(names
        .iter()
        .map(|name| scope.property_key(c_string(name).as_bytes()))
        .collect())
}

fn dataset_set(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    value: &Value,
    _receiver: &Value,
    _flags: SetFlags,
) -> Result<bool, Value> {
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(false);
    };
    let Some(node) = dataset_node(obj) else {
        return Ok(false);
    };
    dataset::named_set(scope, node, &name, value).map(|()| true)
}

fn dataset_delete(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> Result<bool, Value> {
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(true);
    };
    if let Some(node) = dataset_node(obj) {
        dataset::named_delete(scope, node, &name);
    }
    Ok(true)
}

pub(crate) fn register_attr(scope: &mut Scope<'_>, prototype: Value) {
    scope.register_class(&ATTR_CLASS);
    scope.set_class_prototype(&ATTR_CLASS, prototype);
}

pub(crate) fn new_attr(scope: &mut Scope<'_>, state: Shared) -> Result<Value, Value> {
    scope.new_instance_with(&ATTR_CLASS, None, state)
}

pub(crate) fn attr_state(value: &Value) -> Option<Shared> {
    value.with_instance(&ATTR_CLASS, |state: &Shared| state.clone())
}
