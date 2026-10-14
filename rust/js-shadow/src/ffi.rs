//! Southstar — the C ABI and Rust-ABI entry points of the shadow DOM and slot bindings, and the calls they make into the rest of the engine.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::Element;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_node_arm_js_invalidate(node: *mut NsNode);
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
}

unsafe extern "Rust" {
    unsafe fn nsr_ce_class_for_node(js: *const NsJs, node: *const NsNode) -> Value;
    safe fn nsr_document_element_from_point(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_document_elements_from_point(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

fn node_of(raw: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(raw) }
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = scope.context_opaque().cast();
    (!js.is_null()).then_some(Js(js))
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
    unsafe { nsr_ce_class_for_node(js.0, node.as_ptr()) }
}

pub(crate) fn clear_children(js: Option<Js>, node: Element) {
    let js = js.map_or(ptr::null_mut(), |js| js.0);
    unsafe { ns_js_clear_children(js, node.as_mut_ptr()) };
}

pub(crate) fn append_child(parent: Element, child: Element) {
    parent.append(child);
}

pub(crate) fn arm_js_invalidate(node: Element) {
    unsafe { ns_node_arm_js_invalidate(node.as_mut_ptr()) };
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_current_document(js.0) })
}

pub(crate) fn focused_node(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_focused_node(js.0) })
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean {
    GBoolean::from(crate::is_shadow_root(node_of(n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_find_shadow_child(host: *const NsNode) -> *mut NsNode {
    raw(node_of(host).and_then(crate::shadow_child))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_assigned_slot_node(n: *const NsNode) -> *mut NsNode {
    raw(node_of(n).and_then(crate::slot::assigned_slot_node))
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_element_get_shadowRoot(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    crate::root::get_shadow_root(scope, this, &[])
}
