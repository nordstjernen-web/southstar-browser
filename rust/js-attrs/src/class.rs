//! Southstar — the DOMTokenList host class: one token list per element and attribute, and its indexed properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{ClassHooks, HostClass, OwnProperty, PropertyKey, Scope, Trace, Value};

use crate::{Element, ffi, token_list};

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
