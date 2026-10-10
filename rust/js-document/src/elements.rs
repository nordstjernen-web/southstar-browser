//! Southstar — the document's element getters: documentElement, body and its setter, head, scrollingElement, activeElement, currentScript, scripts and anchors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::VecDeque;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, FLAG_QUIRKS, JsResult, document_for, element_named};

const HIERARCHY_REQUEST_ERR: i32 = 3;
const SHADOW_ATTR: &core::ffi::CStr = c"data-nd-shadow-root";

fn html_element_named(node: Element, tag: &str) -> bool {
    element_named(node, tag) && node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
}

fn is_shadow_root(node: Element) -> bool {
    node.is_element() && node.attr(SHADOW_ATTR).is_some()
}

fn hidden_child(node: Element) -> bool {
    southstar_dom::serialize::is_embedded_doc(node) || is_shadow_root(node)
}

fn this_or_current(scope: &Scope<'_>, this: &Value) -> Option<Element> {
    ffi::unwrap_node(this).or_else(|| ffi::current_document(ffi::js_of(scope)))
}

fn root_node(scope: &Scope<'_>, this: &Value) -> Option<Element> {
    let doc = this_or_current(scope, this)?;
    if doc.is_element() {
        return Some(doc);
    }
    southstar_dom::children(doc).find(|c| c.is_element())
}

fn body_node(scope: &Scope<'_>, this: &Value) -> Option<Element> {
    let root = root_node(scope, this).filter(|r| html_element_named(*r, "html"))?;
    southstar_dom::children(root)
        .find(|c| html_element_named(*c, "body") || html_element_named(*c, "frameset"))
}

pub(crate) fn get_document_element(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let root = root_node(scope, this);
    Ok(ffi::wrap_node(scope, root))
}

pub(crate) fn get_body(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let body = body_node(scope, this);
    Ok(ffi::wrap_node(scope, body))
}

fn hierarchy_error(scope: &mut Scope<'_>, message: &str) -> Value {
    ffi::dom_exception(
        scope,
        "HierarchyRequestError",
        HIERARCHY_REQUEST_ERR,
        message,
    )
}

fn is_inclusive_ancestor(ancestor: Element, node: Element) -> bool {
    southstar_dom::ancestors_and_self(node).any(|n| n == ancestor)
}

fn detach_old_body(js: Js, root: Element, old_body: Element) -> Option<Element> {
    if old_body.parent() != Some(root) {
        return None;
    }
    let reference = old_body.next_sibling();
    let previous = old_body.prev_sibling();
    ffi::remove_node(old_body);
    if !js.is_null() {
        ffi::orphan_node(js, old_body);
        ffi::record_child_change(js, root, None, Some(old_body), previous, reference);
    }
    reference
}

pub(crate) fn set_body(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = &args[0];
    let Some(new_body) = ffi::unwrap_node(value) else {
        return Err(scope.type_error("Document.body must be an Element"));
    };
    if !html_element_named(new_body, "body") && !html_element_named(new_body, "frameset") {
        return Err(hierarchy_error(
            scope,
            "Document.body must be body or frameset",
        ));
    }
    let Some(root) = root_node(scope, this) else {
        return Err(hierarchy_error(scope, "Document has no document element"));
    };
    let old_body = body_node(scope, this);
    if old_body == Some(new_body) {
        return Ok(Value::undefined());
    }
    let js = ffi::js_of(scope);
    if !js.is_null()
        && let Some(parent) = new_body.parent()
    {
        ffi::record_child_change(
            js,
            parent,
            None,
            Some(new_body),
            new_body.prev_sibling(),
            new_body.next_sibling(),
        );
    }
    let new_body = match old_body.filter(|old| *old != root) {
        Some(old_body) => {
            if !js.is_null() {
                ffi::iters_pre_remove(js, old_body);
                ffi::ce_disconnect_subtree(js, old_body);
            }
            let Some(new_body) =
                ffi::unwrap_node(value).filter(|n| !is_inclusive_ancestor(*n, root))
            else {
                return Err(hierarchy_error(scope, "Document.body cannot be inserted"));
            };
            match detach_old_body(js, root, old_body) {
                Some(reference) => ffi::insert_before_single(js, root, new_body, reference),
                None => ffi::append_child(root, new_body),
            }
            new_body
        }
        None => {
            ffi::append_child(root, new_body);
            new_body
        }
    };
    if !js.is_null() {
        ffi::unorphan_node(js, new_body);
        ffi::mark_mutated(js);
        ffi::record_child_change(
            js,
            root,
            Some(new_body),
            None,
            new_body.prev_sibling(),
            new_body.next_sibling(),
        );
    }
    Ok(Value::undefined())
}

pub(crate) fn get_scrolling_element(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let Some(doc) = document_for(scope, this) else {
        return Ok(Value::null());
    };
    if doc.flags() & FLAG_QUIRKS != 0 {
        return get_body(scope, this, a);
    }
    get_document_element(scope, this, a)
}

pub(crate) fn get_head(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(doc) = document_for(scope, this) else {
        return Ok(Value::null());
    };
    let head = southstar_dom::index::find_first_element(doc, c"head");
    Ok(ffi::wrap_node(scope, head))
}

fn top_document(doc: Option<Element>) -> Option<Element> {
    let mut doc = doc;
    while let Some(parent) = doc.and_then(|d| d.parent()) {
        doc = southstar_dom::ancestors_and_self(parent).find(|n| n.kind() == Kind::Document);
    }
    doc
}

fn ancestor_or_self(desc: Element, root: Option<Element>) -> bool {
    let Some(root) = root else {
        return false;
    };
    desc == root
        || southstar_dom::ancestors(desc)
            .take(MAX_DEPTH as usize)
            .any(|p| p == root)
}

fn active_element_in(js: Js, doc: Element) -> Option<Element> {
    let mut n = ffi::focused_node(js).or_else(|| {
        ffi::focused_doc(js)
            .filter(|fdoc| *fdoc != doc)
            .and_then(|fdoc| fdoc.parent())
    });
    while let Some(node) = n {
        let mut found = node;
        let mut p = node.parent();
        while let Some(parent) = p.filter(|p| p.kind() != Kind::Document) {
            if is_shadow_root(parent)
                && let Some(host) = parent.parent()
            {
                found = host;
            }
            p = parent.parent();
        }
        let p = p?;
        if p == doc {
            return Some(found);
        }
        n = p.parent();
    }
    None
}

pub(crate) fn get_active_element(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let doc = document_for(scope, this);
    let js = ffi::js_of(scope);
    let Some(doc) = doc.filter(|_| !js.is_null()) else {
        return Ok(Value::null());
    };
    if let Some(focused) = ffi::focused_node(js)
        && !ancestor_or_self(focused, top_document(ffi::current_document(js)))
    {
        ffi::clear_focused_node(js);
    }
    let active = active_element_in(js, doc)
        .or_else(|| southstar_dom::index::find_first_element(doc, c"body"));
    Ok(ffi::wrap_node(scope, active))
}

pub(crate) fn get_current_script(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::null());
    }
    let script = ffi::current_script(js);
    Ok(ffi::wrap_node(scope, script))
}

fn breadth_first(doc: Element, matches: impl Fn(Element) -> bool) -> Vec<Element> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([doc]);
    while let Some(n) = queue.pop_front() {
        for c in southstar_dom::children(n) {
            if hidden_child(c) {
                continue;
            }
            if matches(c) {
                found.push(c);
            }
            queue.push_back(c);
        }
    }
    found
}

fn node_array(scope: &mut Scope<'_>, nodes: Vec<Element>) -> JsResult {
    let array = scope.new_array();
    for (index, node) in nodes.into_iter().enumerate() {
        let item = ffi::wrap_node(scope, Some(node));
        scope.set_index(&array, index as u32, item)?;
    }
    Ok(array)
}

pub(crate) fn get_scripts(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(doc) = document_for(scope, this) else {
        return Ok(scope.new_array());
    };
    let scripts = if doc.tag_table().is_some() {
        southstar_dom::index::tag_lookup(doc, c"script", |list| {
            (0..list.len()).map(|i| list.get(i)).collect()
        })
        .unwrap_or_default()
    } else {
        breadth_first(doc, |c| {
            c.name().is_some_and(|name| {
                c.is_element() && name.to_bytes().eq_ignore_ascii_case(b"script")
            })
        })
    };
    node_array(scope, scripts)
}

pub(crate) fn get_anchors(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(doc) = document_for(scope, this) else {
        return Ok(scope.new_array());
    };
    let anchors = breadth_first(doc, |c| element_named(c, "a") && c.attr(c"name").is_some());
    node_array(scope, anchors)
}

pub(crate) fn empty_list(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.new_array())
}
