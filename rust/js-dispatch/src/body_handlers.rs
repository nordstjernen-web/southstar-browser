//! Southstar — the window event handlers <body> and <frameset> reflect: their accessors on the element and the content attributes forwarded to the window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, is_named};

const WINDOW_REFLECTED: [&str; 22] = [
    "onafterprint",
    "onbeforeprint",
    "onbeforeunload",
    "onblur",
    "onerror",
    "onfocus",
    "onhashchange",
    "onlanguagechange",
    "onload",
    "onmessage",
    "onmessageerror",
    "onoffline",
    "ononline",
    "onpagehide",
    "onpageshow",
    "onpopstate",
    "onrejectionhandled",
    "onresize",
    "onscroll",
    "onstorage",
    "onunhandledrejection",
    "onunload",
];

const ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

fn has_browsing_context(scope: &mut Scope<'_>, this: &Value) -> bool {
    !scope
        .get(this, "__ndOwnerDoc")
        .is_ok_and(|owner| owner.is_object())
}

fn handler_name(scope: &mut Scope<'_>, data: &[Value]) -> Option<String> {
    let name = data.first()?;
    scope.to_string(name).ok()
}

fn reflected_get(scope: &mut Scope<'_>, this: &Value, _args: &[Value], data: &[Value]) -> JsResult {
    if !has_browsing_context(scope, this) {
        return Ok(Value::null());
    }
    let Some(name) = handler_name(scope, data) else {
        return Ok(Value::null());
    };
    let global = scope.global();
    scope.get(&global, &name)
}

fn reflected_set(scope: &mut Scope<'_>, this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    if !has_browsing_context(scope, this) {
        return Ok(Value::undefined());
    }
    if let Some(name) = handler_name(scope, data) {
        let handler = match args.first() {
            Some(value) if scope.is_function(value) => value.clone(),
            _ => Value::null(),
        };
        let global = scope.global();
        let _ = scope.set(&global, &name, handler);
    }
    Ok(Value::undefined())
}

pub(crate) fn install(scope: &mut Scope<'_>, wrapper: &Value) {
    for name in WINDOW_REFLECTED {
        let data = [scope.string(name)];
        let getter = scope.bound_function("", 0, reflected_get, &data);
        let setter = scope.bound_function("", 1, reflected_set, &data);
        let _ = scope.define_accessor(wrapper, name, Some(&getter), Some(&setter), ACCESSOR);
    }
}

fn is_window_reflected(name: &str) -> bool {
    WINDOW_REFLECTED
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

pub(crate) fn forward_content_handler(
    scope: &mut Scope<'_>,
    node: Element,
    name: &str,
    code: Option<&str>,
) {
    if !node.is_element() || !(is_named(Some(node), b"body") || is_named(Some(node), b"frameset")) {
        return;
    }
    if !is_window_reflected(name) {
        return;
    }
    let handler = match code.filter(|c| !c.is_empty()) {
        Some(code) => {
            crate::handlers::compile_inline(scope, code).unwrap_or_else(|_| Value::null())
        }
        None => Value::null(),
    };
    let global = scope.global();
    let _ = scope.set(&global, name, handler);
}
