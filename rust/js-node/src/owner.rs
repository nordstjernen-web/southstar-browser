//! Southstar — the getters that reach past the node itself: ownerDocument, tagName, baseURI, template content, children and childNodes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::names;
use crate::{Element, JsResult, ffi, is_document};

const LIVE_CHILDREN: i32 = 0;
const LIVE_CHILDNODES: i32 = 1;
const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

fn object_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Value> {
    scope.get(object, key).ok().filter(Value::is_object)
}

fn document_ancestor(node: Element) -> Option<Element> {
    southstar_dom::ancestors(node).find(|&p| is_document(p))
}

fn is_frame_host(node: Element) -> bool {
    matches!(node.element_name(), Some(b"iframe" | b"object"))
}

pub(crate) fn owner_document(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(js) = ffi::js_of(scope) else {
        return Ok(Value::null());
    };
    let node = ffi::unwrap_node(this);
    if node.is_some_and(is_document) {
        return Ok(Value::null());
    }
    if let Some(owner) = ffi::attr_owner(this) {
        let owner = ffi::make_node(scope, Some(owner));
        return owner_document(scope, &owner);
    }
    let current = ffi::current_document(js);
    if let Some(doc) = node.and_then(document_ancestor) {
        if Some(doc) == current {
            return Ok(ffi::make_node(scope, current));
        }
        if let Some(host) = doc
            .parent()
            .filter(|&h| h.has_js_wrapper() && is_frame_host(h))
        {
            let wrapper = ffi::make_node(scope, Some(host));
            if let Some(realm) = object_property(scope, &wrapper, "__ndRealmDoc") {
                return Ok(realm);
            }
        }
        return Ok(ffi::make_node(scope, Some(doc)));
    }
    if let Some(own) = object_property(scope, this, "__ndOwnerDoc") {
        return Ok(own);
    }
    let no_owner = scope
        .get(this, "__ndNoOwnerDoc")
        .is_ok_and(|v| scope.to_bool(&v));
    if no_owner {
        return Ok(Value::null());
    }
    Ok(ffi::make_node(scope, current))
}

pub(crate) fn tag_name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let node = ffi::unwrap_node(this);
    let text = names::tag_name(node, || {
        owner_document(scope, this).is_ok_and(|doc| doc.is_object() && ffi::doc_is_xml(scope, &doc))
    });
    Ok(names::to_value(scope, text))
}

pub(crate) fn base_uri(scope: &mut Scope<'_>, _this: &Value) -> JsResult {
    let base = ffi::js_of(scope).and_then(ffi::doc_base_url);
    let base = base
        .as_deref()
        .filter(|b| !b.is_empty())
        .unwrap_or(b"about:blank");
    Ok(scope.string_from_bytes(base))
}

pub(crate) fn template_content(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(scope.string(""));
    };
    let Some(name) = node.name() else {
        return Ok(scope.string(""));
    };
    if !name.to_bytes().eq_ignore_ascii_case(b"template") {
        return ffi::reflect_str_get(scope, this, c"content");
    }
    let cached = scope.get(this, "__nd_template_content")?;
    if !cached.is_undefined() {
        return Ok(cached);
    }
    let content = southstar_dom::node::template_content(node);
    let wrapped = ffi::make_node(scope, Some(content));
    scope.define(
        this,
        "__nd_template_content",
        wrapped.clone(),
        Attributes::METHOD,
    )?;
    Ok(wrapped)
}

pub(crate) fn children(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    if ffi::unwrap_node(this).is_none() {
        return Ok(scope.new_array());
    }
    ffi::make_live(scope, this, LIVE_CHILDREN)
}

pub(crate) fn child_nodes(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    if ffi::unwrap_node(this).is_none() {
        return Ok(scope.new_array());
    }
    if let Some(cached) = object_property(scope, this, "__ndChildNodes") {
        return Ok(cached);
    }
    let list = ffi::make_live(scope, this, LIVE_CHILDNODES)?;
    let _ = scope.define(this, "__ndChildNodes", list.clone(), HIDDEN);
    Ok(list)
}
