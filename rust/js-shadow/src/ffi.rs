//! Southstar — the C ABI of the shadow DOM and slot bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::Element;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_ce_class_for_node(js: *mut NsJs, node: *const NsNode) -> JSValue;
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_node_arm_js_invalidate(node: *mut NsNode);
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_document_element_from_point(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_document_elements_from_point(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
}

fn node_of(raw: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(raw) }
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = quickjs::context_opaque(scope).cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    node_of(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn not_supported(scope: &mut Scope<'_>, message: &str) -> Value {
    let message = CString::new(message).unwrap_or_default();
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            c"NotSupportedError".as_ptr(),
            9,
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn custom_element_class(scope: &mut Scope<'_>, js: Js, node: Element) -> Value {
    let raw = unsafe { ns_ce_class_for_node(js.0, node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn clear_children(js: Option<Js>, node: Element) {
    let js = js.map_or(ptr::null_mut(), |js| js.0);
    unsafe { ns_js_clear_children(js, node.as_mut_ptr()) };
}

pub(crate) fn append_child(parent: Element, child: Element) {
    unsafe { ns_node_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) };
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
            quickjs::c_function(scope, "elementFromPoint", 2, ns_document_element_from_point),
        ),
        (
            "elementsFromPoint",
            quickjs::c_function(
                scope,
                "elementsFromPoint",
                2,
                ns_document_elements_from_point,
            ),
        ),
    ]
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { native(ctx, this_val, 0, ptr::null_mut(), $f) }
            }
        )*
    };
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

getters! {
    ns_element_get_shadowRoot => crate::root::get_shadow_root,
    ns_element_get_assignedSlot => crate::slot::get_assigned_slot,
}

methods! {
    ns_element_attachShadow => crate::root::attach_shadow,
    ns_element_getRootNode => crate::root::get_root_node,
    ns_element_assignedNodes => crate::slot::assigned_nodes,
    ns_element_assignedElements => crate::slot::assigned_elements,
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
