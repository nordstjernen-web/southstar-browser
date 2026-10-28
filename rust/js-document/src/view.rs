//! Southstar — document.defaultView: the window of the document's browsing context, the page's or a frame's, and null for a document without one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult, element_named};

fn page_window(scope: &mut Scope<'_>, js: Js) -> Value {
    if !js.is_null()
        && let Some(global) = ffi::with_main_context(js, |main| main.global())
    {
        return global;
    }
    scope.global()
}

fn is_frame_element(node: Element) -> bool {
    node.is_element()
        && (element_named(node, "iframe")
            || element_named(node, "frame")
            || element_named(node, "object"))
}

pub fn default_view(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let doc = ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Document);
    let Some(doc) = doc.filter(|&d| !js.is_null() && Some(d) != ffi::current_document(js)) else {
        return Ok(page_window(scope, js));
    };
    match doc.parent().filter(|&owner| is_frame_element(owner)) {
        Some(owner) => Ok(ffi::iframe_realm_window(scope, owner)),
        None => Ok(Value::null()),
    }
}
