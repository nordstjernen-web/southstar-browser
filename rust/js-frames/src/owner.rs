//! Southstar — frame owner elements: the document a frame holds, contentDocument, contentWindow, getSVGDocument and the names and window events a frame realm starts from.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::qjs::Ctx;
use crate::ffi::{self, Js};
use crate::realm;

type JsResult<T = Value> = Result<T, Value>;

const XML_DOC: u32 = 1 << 14;

const SHOWN_URL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

fn named(node: Node<'_>, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

fn is_frame_owner(node: Node<'_>) -> bool {
    named(node, b"iframe")
        || named(node, b"object")
        || named(node, b"frame")
        || named(node, b"embed")
}

pub(crate) fn document_node(iframe: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::children(iframe).find(|child| child.kind() == Kind::Document)
}

fn content_root(iframe: Node<'_>) -> Option<Node<'_>> {
    for child in southstar_dom::children(iframe) {
        match child.kind() {
            Kind::Document => {
                return southstar_dom::children(child).find(|e| e.kind() == Kind::Element);
            }
            Kind::Element => return Some(child),
            _ => {}
        }
    }
    None
}

pub(crate) fn ensure_content_root(iframe: Node<'_>) -> Node<'_> {
    if let Some(root) = content_root(iframe) {
        return root;
    }
    let root = ffi::new_element(c"html");
    ffi::append_child(root, ffi::new_element(c"head"));
    ffi::append_child(root, ffi::new_element(c"body"));
    let doc = ffi::new_document();
    ffi::append_child(doc, root);
    ffi::append_child(iframe, doc);
    root
}

fn has_browsing_context(js: Js, frame: Node<'_>) -> bool {
    if js.is_null() {
        return false;
    }
    let Some(page) = ffi::ce_main_doc(js).or_else(|| ffi::current_document(js)) else {
        return false;
    };
    let mut root = frame;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    root.as_ptr() == page.as_ptr()
}

fn non_empty(text: Option<&CStr>) -> Option<&CStr> {
    text.filter(|text| !text.is_empty())
}

pub(crate) fn show_url(scope: &mut Scope<'_>, doc: &Value, url: Option<&CStr>) {
    let Some(url) = non_empty(url) else {
        return;
    };
    if !doc.is_object() {
        return;
    }
    for key in ["URL", "documentURI"] {
        let text = scope.string_from_bytes(url.to_bytes());
        let _ = scope.define(doc, key, text, SHOWN_URL);
    }
}

fn build_content_document(scope: &mut Scope<'_>, iframe: Node<'_>) -> JsResult {
    ensure_content_root(iframe);
    let Some(doc) = document_node(iframe) else {
        return Ok(Value::null());
    };
    let ctx = ffi::scope_ctx(scope);
    if doc.has_js_wrapper() {
        return Ok(ffi::make_element(ctx, doc));
    }
    let js = Js::of_ctx(ctx);
    let charset = iframe.attr(c"data-nd-frame-charset").map(CStr::to_owned);
    let mut url = non_empty(iframe.attr(c"data-nd-frame-url")).map(CStr::to_owned);
    let mut shown = non_empty(iframe.attr(c"data-nd-frame-doc-url")).map(CStr::to_owned);
    if url.is_none() {
        url = ffi::node_doc_base(js, iframe).map(|base| CString::new(base).unwrap_or_default());
        shown = Some(c"about:blank".to_owned());
        ffi::set_doc_ready_state(js, doc, 2);
    }
    let is_xml = doc.flags() & XML_DOC != 0;
    let mime = if is_xml {
        c"application/xml"
    } else {
        c"text/html"
    };
    let document =
        ffi::make_realm_document(scope, doc, url.as_deref(), charset.as_deref(), mime, is_xml)?;
    show_url(scope, &document, shown.as_deref());
    if document.is_object() {
        let global = scope.global();
        let _ = scope.set(&document, "defaultView", global);
    }
    Ok(document)
}

pub(crate) fn content_document_of(
    scope: &mut Scope<'_>,
    element: &Value,
    node: Node<'_>,
) -> JsResult {
    let realm_doc = scope
        .get(element, "__ndRealmDoc")
        .unwrap_or_else(|_| Value::undefined());
    if realm_doc.is_object() {
        return Ok(realm_doc);
    }
    if let Some(doc) = document_node(node).filter(|doc| doc.has_js_wrapper()) {
        return Ok(ffi::make_element(ffi::scope_ctx(scope), doc));
    }
    if !named(node, b"iframe") {
        return Ok(Value::null());
    }
    build_content_document(scope, node)
}

pub(crate) fn content_document(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_element(this).filter(|node| is_frame_owner(*node)) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    if !named(node, b"iframe") && !named(node, b"object") {
        return Ok(Value::null());
    }
    let js = Js::of_ctx(ffi::scope_ctx(scope));
    if !has_browsing_context(js, node) || ffi::is_cross_origin(js, node) {
        return Ok(Value::null());
    }
    content_document_of(scope, this, node)
}

pub(crate) fn content_window(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_element(this).filter(|node| is_frame_owner(*node)) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    if !named(node, b"iframe") {
        return Ok(Value::null());
    }
    let ctx = ffi::scope_ctx(scope);
    let js = Js::of_ctx(ctx);
    if !has_browsing_context(js, node) {
        return Ok(Value::null());
    }
    ensure_content_root(node);
    let window = realm::realm_window(ctx, this, node);
    if !window.is_object() || !ffi::is_cross_origin(js, node) {
        return Ok(window);
    }
    Ok(realm::cross_origin_window(ctx, window))
}

pub(crate) fn svg_document(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_element(this) else {
        return Ok(Value::null());
    };
    if !named(node, b"iframe") && !named(node, b"object") && !named(node, b"embed") {
        return Ok(Value::null());
    }
    let doc = content_document(scope, this)?;
    if !doc.is_object() {
        return Ok(doc);
    }
    let root = scope.get(&doc, "documentElement")?;
    let name = if root.is_object() {
        scope.get(&root, "localName")?
    } else {
        Value::null()
    };
    let is_svg = scope
        .to_bytes(&name)
        .is_ok_and(|name| name.eq_ignore_ascii_case(b"svg"));
    Ok(if is_svg { doc } else { Value::null() })
}

pub(crate) fn child_frame_window(
    scope: &mut Scope<'_>,
    doc: Option<Node<'_>>,
    index: u32,
    name: Option<&CStr>,
    raw: bool,
) -> JsResult {
    let Some(frame) = ffi::window_child_frame(doc, index, name) else {
        return Ok(Value::undefined());
    };
    let ctx = ffi::scope_ctx(scope);
    let element = ffi::make_element(ctx, frame);
    if !raw {
        return content_window(scope, &element);
    }
    if named(frame, b"iframe") {
        ensure_content_root(frame);
        return Ok(realm::realm_window(ctx, &element, frame));
    }
    Ok(Value::undefined())
}

pub(crate) fn platform_names(js: Js, ctx: Ctx) -> Value {
    let names = ffi::platform_global_names(js);
    ctx.enter(|scope| {
        let object = scope.new_object_with_proto(&Value::null());
        for name in names {
            let _ = scope.set(&object, &name, Value::boolean(true));
        }
        object
    })
}

pub(crate) fn frame_window_events(ctx: Ctx) -> Value {
    ctx.enter(|scope| {
        let events = scope.new_object_with_proto(&Value::null());
        for (key, function) in ffi::window_event_functions(scope) {
            let _ = scope.set(&events, key, function);
        }
        events
    })
}
