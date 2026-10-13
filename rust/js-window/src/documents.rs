//! Southstar — the document a window object stands for, and window.find() over the page's text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value};

use crate::ffi;

fn as_document(node: Option<Node<'static>>) -> Option<Node<'static>> {
    node.filter(|n| n.kind() == Kind::Document)
}

pub(crate) fn document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Node<'static>> {
    if window.is_object() {
        let doc = scope
            .get(window, "document")
            .unwrap_or_else(|_| Value::undefined());
        if let Some(doc) = as_document(ffi::unwrap(&doc)) {
            return Some(doc);
        }
    }
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    if let Some(doc) = as_document(ffi::iframe_document(js)) {
        return Some(doc);
    }
    ffi::current_document(js)
}

fn frame_document(
    scope: &mut Scope<'_>,
    window: &Value,
    stale: Node<'static>,
) -> Option<Node<'static>> {
    let frame_element = scope
        .get(window, "frameElement")
        .unwrap_or_else(|_| Value::undefined());
    Some(ffi::frame_document(ffi::unwrap(&frame_element)).unwrap_or(stale))
}

pub(crate) fn current_document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Node<'static>> {
    let js = ffi::js_of(scope);
    let forwarded = crate::message::forward_of(js, window);
    let window = if forwarded.is_object() {
        &forwarded
    } else {
        window
    };
    let doc = document_for(scope, window)?;
    if doc.parent().is_none()
        && !js.is_null()
        && Some(doc) != ffi::ce_main_document(js)
        && !scope.global().same_object(window)
    {
        return frame_document(scope, window, doc);
    }
    Some(doc)
}

fn c_string_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

pub(crate) fn find(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(needle) = args.first() else {
        return Ok(Value::boolean(false));
    };
    let Ok(needle) = scope.to_bytes(needle) else {
        return Ok(Value::boolean(false));
    };
    let needle = c_string_prefix(&needle);
    if needle.is_empty() {
        return Ok(Value::boolean(false));
    }
    let case_sensitive = args.get(1).is_some_and(|flag| scope.to_bool(flag));
    let js = ffi::js_of(scope);
    let doc = if js.is_null() {
        None
    } else {
        ffi::current_document(js)
    };
    let found = doc.is_some_and(|doc| {
        let text = doc.collect_text();
        let haystack = c_string_prefix(&text);
        if case_sensitive {
            contains(haystack, needle)
        } else {
            contains(&ffi::casefold(haystack), &ffi::casefold(needle))
        }
    });
    Ok(Value::boolean(found))
}
