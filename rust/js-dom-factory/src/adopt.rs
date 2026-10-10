//! Southstar — node documents across documents: cloneNode, importNode, adoptNode, the owner-document walk and the Attr copy.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, MAX_DEPTH, children, node};
use southstar_js_engine::{Scope, Value};

use crate::factories::attr_node;
use crate::ffi::{self, AttrName, RealmDocument};
use crate::names::NOT_SUPPORTED_ERR;
use crate::{Element, FLAG_XML_DOC, HIDDEN, JsResult, arg, c_text, is_document, is_nullish};

const ATTRIBUTE_NODE: i32 = 2;
const DOCUMENT_NODE: i32 = 9;

pub(crate) fn doc_is_xml(scope: &mut Scope<'_>, doc: &Value) -> bool {
    if !doc.is_object() {
        return false;
    }
    match scope.get(doc, "__ndXmlDoc") {
        Ok(xml) => scope.to_bool(&xml),
        Err(_) => false,
    }
}

pub(crate) fn doc_is_xhtml(scope: &mut Scope<'_>, doc: &Value) -> bool {
    if !doc.is_object() {
        return false;
    }
    let Ok(content_type) = scope.get(doc, "contentType") else {
        return false;
    };
    c_text(scope, &content_type).is_ok_and(|ct| ct.windows(5).any(|w| w == b"xhtml"))
}

fn node_type(scope: &mut Scope<'_>, value: &Value) -> i32 {
    scope
        .get(value, "nodeType")
        .and_then(|t| scope.to_int32(&t))
        .unwrap_or(0)
}

pub(crate) fn tag_owner_document(scope: &mut Scope<'_>, doc: &Value, node: &Value) {
    if !doc.is_object() || !node.is_object() {
        return;
    }
    match ffi::unwrap_node(doc) {
        Some(d) => {
            if !is_document(d) {
                return;
            }
            if ffi::js_of(scope).and_then(ffi::current_document) == Some(d) {
                return;
            }
        }
        None => {
            if node_type(scope, doc) != DOCUMENT_NODE {
                return;
            }
        }
    }
    let _ = scope.define(node, "__ndOwnerDoc", doc.clone(), HIDDEN);
}

fn owner_walk(scope: &mut Scope<'_>, doc: &Value, is_xml: bool, node: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if is_xml {
        node.add_flags(FLAG_XML_DOC);
    } else {
        node.remove_flags(FLAG_XML_DOC);
    }
    let wrapper = ffi::wrap(scope, node);
    if wrapper.is_object() {
        tag_owner_document(scope, doc, &wrapper);
        if let Ok(attrs) = scope.get(&wrapper, "attributes")
            && attrs.is_object()
        {
            let len = scope
                .get(&attrs, "length")
                .and_then(|l| scope.to_number(&l))
                .map_or(0, |l| {
                    if l.is_finite() && l > 0.0 {
                        l as u32
                    } else {
                        0
                    }
                });
            for i in 0..len {
                if let Ok(attr) = scope.get_index(&attrs, i)
                    && attr.is_object()
                {
                    tag_owner_document(scope, doc, &attr);
                }
            }
        }
    }
    for child in children(node) {
        owner_walk(scope, doc, is_xml, child, depth + 1);
    }
}

pub(crate) fn adopt_owner_walk(scope: &mut Scope<'_>, doc: &Value, node: Element) {
    let is_xml = doc_is_xml(scope, doc);
    owner_walk(scope, doc, is_xml, node, 0);
}

pub(crate) fn adopt_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::null());
    }
    let Some(node) = ffi::unwrap_node(&args[0]) else {
        if args[0].is_object() && node_type(scope, &args[0]) == ATTRIBUTE_NODE {
            tag_owner_document(scope, this, &args[0]);
        }
        return Ok(args[0].clone());
    };
    if is_document(node) {
        return Err(ffi::dom_exception(
            scope,
            c"NotSupportedError",
            NOT_SUPPORTED_ERR,
            c"adoptNode: a document cannot be adopted",
        ));
    }
    let js = ffi::js_of(scope);
    if let Some(parent) = node.parent() {
        let previous = node.prev_sibling();
        let next = node.next_sibling();
        node.detach();
        if let Some(js) = js {
            ffi::track_orphan(Some(js), node);
            ffi::record_removal(js, parent, node, previous, next);
        }
    }
    adopt_owner_walk(scope, this, node);
    if let Some(js) = js {
        ffi::mark_mutated(js);
    }
    Ok(args[0].clone())
}

pub(crate) fn import_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::null());
    }
    let Some(src) = ffi::unwrap_node(&args[0]) else {
        if args[0].is_object() && node_type(scope, &args[0]) == ATTRIBUTE_NODE {
            let copy = clone_attr(scope, &args[0], &[])?;
            if copy.is_object() {
                tag_owner_document(scope, this, &copy);
            }
            return Ok(copy);
        }
        return Ok(Value::null());
    };
    let deep = args.len() >= 2 && scope.to_bool(&args[1]);
    let Some(copy) = node::clone(src, deep, 0) else {
        return Ok(Value::null());
    };
    adopt_owner_walk(scope, this, copy);
    if let Some(js) = ffi::js_of(scope) {
        ffi::track_orphan(Some(js), copy);
        ffi::ce_upgrade_subtree_detached(js, copy);
    }
    Ok(ffi::wrap(scope, copy))
}

fn text_or_null(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.to_bytes(value).ok()
}

fn clone_document(scope: &mut Scope<'_>, this: &Value, src: Element, deep: bool) -> JsResult {
    let Some(copy) = node::clone(src, deep, 0) else {
        return Ok(Value::null());
    };
    ffi::track_orphan(ffi::js_of(scope), copy);
    let is_xml = scope
        .get(this, "__ndXmlDoc")
        .is_ok_and(|xml| scope.to_bool(&xml));
    let content_type = scope.get(this, "contentType")?;
    let charset = scope.get(this, "characterSet")?;
    let url = scope.get(this, "URL")?;
    let content_type = text_or_null(scope, &content_type);
    let charset = text_or_null(scope, &charset);
    let url = text_or_null(scope, &url);
    let wrapper = ffi::realm_document(
        scope,
        copy,
        RealmDocument {
            url: url.as_deref(),
            charset: charset.as_deref(),
            content_type: content_type.as_deref(),
            is_xml,
        },
    );
    if wrapper.is_object()
        && let Ok(proto) = scope.get_prototype(this)
        && proto.is_object()
    {
        let _ = scope.set_prototype(&wrapper, &proto);
    }
    Ok(wrapper)
}

pub(crate) fn clone_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(src) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let deep = !args.is_empty() && scope.to_bool(&args[0]);
    if is_document(src) {
        return clone_document(scope, this, src, deep);
    }
    let Some(copy) = node::clone(src, deep, 0) else {
        return Ok(Value::null());
    };
    if let Some(js) = ffi::js_of(scope) {
        ffi::track_orphan(Some(js), copy);
        if !ffi::in_template_content(src) {
            ffi::ce_upgrade_subtree_detached(js, copy);
        }
    }
    Ok(ffi::wrap(scope, copy))
}

fn optional_c_text(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    if is_nullish(value) {
        return None;
    }
    c_text(scope, value).ok()
}

pub(crate) fn clone_attr(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let namespace_uri = scope.get(this, "namespaceURI")?;
    let prefix = scope.get(this, "prefix")?;
    let local = scope.get(this, "localName")?;
    let name = scope.get(this, "name")?;
    let value = scope.get(this, "value")?;
    let namespace_uri = optional_c_text(scope, &namespace_uri);
    let prefix = optional_c_text(scope, &prefix);
    let local = c_text(scope, &local).ok();
    let name = c_text(scope, &name).ok();
    let local = local.unwrap_or_default();
    let name = name.unwrap_or_else(|| local.clone());
    let copy = attr_node(
        scope,
        AttrName {
            namespace_uri: namespace_uri.as_deref(),
            prefix: prefix.as_deref(),
            local_name: &local,
            name: &name,
        },
    )?;
    for key in ["value", "nodeValue", "textContent"] {
        scope.set(&copy, key, value.clone())?;
    }
    Ok(copy)
}

pub(crate) fn doctype_arg(args: &[Value], index: usize) -> Option<Element> {
    ffi::unwrap_node(&arg(args, index)).filter(|n| n.kind() == Kind::Doctype)
}
