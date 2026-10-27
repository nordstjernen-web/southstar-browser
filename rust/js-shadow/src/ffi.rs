//! Southstar — the C ABI and Rust-ABI entry points of the shadow DOM and slot bindings, and the calls they make into the rest of the engine.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::Element;

#[derive(Clone, Copy)]
pub(crate) struct Js(Page);

use southstar_js_geometry::nsr_document_element_from_point;
use southstar_js_geometry::nsr_document_elements_from_point;

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    Page::current(scope).map(Js)
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn not_supported(scope: &mut Scope<'_>, message: &str) -> Value {
    api::dom_exception(scope, "NotSupportedError", 9, message)
}

pub(crate) fn custom_element_class(_scope: &mut Scope<'_>, js: Js, node: Element) -> Value {
    southstar_js_custom_elements::class_for_node(Some(js.0), node)
}

pub(crate) fn clear_children(js: Option<Js>, node: Element) {
    southstar_js_mutation::clear_children(js.map(|js| js.0), node);
}

pub(crate) fn append_child(parent: Element, child: Element) {
    parent.append(child);
}

pub(crate) fn arm_js_invalidate(node: Element) {
    api::arm_js_invalidate(node);
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(Some(js.0))
}

pub(crate) fn focused_node(js: Js) -> Option<Element> {
    southstar_js_focus::focused_node(Some(js.0))
}

pub(crate) fn element_from_point_fns(scope: &mut Scope<'_>) -> [(&'static str, Value); 2] {
    [
        (
            "elementFromPoint",
            scope.function("elementFromPoint", 2, nsr_document_element_from_point),
        ),
        (
            "elementsFromPoint",
            scope.function("elementsFromPoint", 2, nsr_document_elements_from_point),
        ),
    ]
}

#[allow(non_snake_case)]
pub fn nsr_element_get_shadowRoot(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    crate::root::get_shadow_root(scope, this, &[])
}
