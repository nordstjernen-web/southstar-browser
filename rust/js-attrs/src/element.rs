//! Southstar — the element members backed by attribute objects: classList, relList, sandbox and sizes token lists, their put-forwards setters and the attributes NamedNodeMap.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CStr;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, c_string, ffi};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

fn named(node: Element, tag: &str) -> bool {
    node.element_name() == Some(tag.as_bytes())
}

fn foreign(node: Element) -> bool {
    node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) != 0
}

pub(crate) fn token_list(scope: &mut Scope<'_>, element: &Value, attr: &'static CStr) -> JsResult {
    if ffi::unwrap_node(element).is_none() {
        return Ok(Value::null());
    }
    let key = format!("__nsTokenList_{}", attr.to_string_lossy());
    let cached = scope
        .get(element, &key)
        .unwrap_or_else(|_| Value::undefined());
    if ffi::is_token_list(&cached) {
        return Ok(cached);
    }
    let list = ffi::new_token_list(scope, element, attr)?;
    let _ = scope.define(element, &key, list.clone(), HIDDEN);
    Ok(list)
}

pub fn class_list(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    token_list(scope, this, c"class")
}

pub fn rel_list(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let html_link = !foreign(node)
        && ["a", "area", "form", "link"]
            .iter()
            .any(|tag| named(node, tag));
    let svg_link = node.flags() & FLAG_SVG_NS != 0 && node.name() == Some(c"a");
    if !html_link && !svg_link {
        return Ok(Value::undefined());
    }
    token_list(scope, this, c"rel")
}

pub fn sandbox_list(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    match ffi::unwrap_node(this) {
        Some(node) if !foreign(node) && named(node, "iframe") => {
            token_list(scope, this, c"sandbox")
        }
        _ => Ok(Value::undefined()),
    }
}

pub fn sizes_list(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|&n| !foreign(n)) else {
        return Ok(Value::undefined());
    };
    if named(node, "link") {
        return token_list(scope, this, c"sizes");
    }
    if named(node, "img") || named(node, "source") {
        return ffi::reflect_str_get(scope, this, c"sizes");
    }
    Ok(Value::undefined())
}

fn put_forwards(scope: &mut Scope<'_>, this: &Value, args: &[Value], attr: &CStr) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    let text = c_string(&scope.to_bytes(&value)?);
    ffi::set_attr(ffi::js_of(scope), node, attr, &text);
    Ok(Value::undefined())
}

pub fn set_style(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    put_forwards(scope, this, args, c"style")
}

pub fn set_class_list(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    put_forwards(scope, this, args, c"class")
}

pub fn set_rel_list(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    put_forwards(scope, this, args, c"rel")
}

pub fn attributes(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|n| n.is_element()) else {
        return Ok(scope.new_object());
    };
    let js = ffi::js_of(scope);
    if let Some(cached) = ffi::attribute_map_lookup(scope, js, node) {
        return Ok(cached);
    }
    let map = ffi::make_attributes_live(scope, this)?;
    let global = scope.global();
    let prototype = scope
        .get(&global, "NamedNodeMap")
        .and_then(|constructor| scope.get(&constructor, "prototype"));
    if let Ok(prototype) = prototype
        && prototype.is_object()
    {
        let _ = scope.set_prototype(&map, &prototype);
    }
    ffi::attribute_map_remember(scope, js, node, &map);
    Ok(map)
}
