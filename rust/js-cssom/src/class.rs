//! Southstar — the CSSStyleDeclaration host class behind element.style: its named properties read and write the style attribute.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyKey, Scope, SetFlags, Trace, Value,
};

use crate::{declaration, ffi};

pub(crate) static STYLE_CLASS: HostClass = HostClass::new(
    c"CSSStyleDeclaration",
    ClassHooks {
        get_own_property: Some(get_own),
        set: Some(set),
        ..ClassHooks::NONE
    },
);

struct StyleDecl {
    element: Value,
}

impl Trace for StyleDecl {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.element);
    }
}

pub fn register(scope: &mut Scope<'_>, prototype: Value) {
    scope.register_class(&STYLE_CLASS);
    scope.set_class_prototype(&STYLE_CLASS, prototype);
}

pub fn prototype(scope: &mut Scope<'_>) -> Value {
    scope.class_prototype(&STYLE_CLASS)
}

pub(crate) fn node_of(value: &Value) -> Option<Node<'static>> {
    value
        .with_instance(&STYLE_CLASS, |decl: &StyleDecl| {
            ffi::unwrap_element(&decl.element)
        })
        .flatten()
}

pub(crate) fn is_style_decl(value: &Value) -> bool {
    value
        .with_instance(&STYLE_CLASS, |_: &StyleDecl| ())
        .is_some()
}

pub(crate) fn new_style_decl(scope: &mut Scope<'_>, element: &Value) -> Result<Value, Value> {
    let decl = StyleDecl {
        element: element.clone(),
    };
    scope.new_traced_instance(&STYLE_CLASS, None, decl)
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(node) = node_of(obj) else {
        return Ok(None);
    };
    if key.is_symbol(scope) {
        return Ok(None);
    }
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(None);
    };
    let name = String::from_utf8_lossy(&name);
    let Some((value, writable)) = declaration::own_value(node, &name) else {
        return Ok(None);
    };
    let value = scope.string_from_bytes(until_nul(value.as_bytes()));
    Ok(Some(OwnProperty::data(value, writable, true, true)))
}

fn set(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    value: &Value,
    _receiver: &Value,
    _flags: SetFlags,
) -> Result<bool, Value> {
    let Some(node) = node_of(obj) else {
        return Ok(false);
    };
    if ffi::write_ignored(scope, node) {
        return Ok(true);
    }
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(false);
    };
    let name = String::from_utf8_lossy(&name).into_owned();
    declaration::set_named(scope, node, &name, value);
    Ok(true)
}
