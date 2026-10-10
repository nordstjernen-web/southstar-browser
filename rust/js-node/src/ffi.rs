//! Southstar — the C ABI of the node mutation bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::Element;

mod query;

pub(crate) use query::*;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_js_index_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
    );
    fn ns_mut_record_emit_child_list_arrays(
        js: *mut NsJs,
        target: *mut NsNode,
        added: *mut GPtrArray,
        removed: *mut GPtrArray,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_css_mark_childlist_dirty(parent: *mut NsNode, added: *mut NsNode);
    fn ns_ce_disconnect_subtree(js: *mut NsJs, root: *mut NsNode);
    fn ns_ce_upgrade_subtree_all(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_iters_pre_remove(js: *mut NsJs, removed: *mut NsNode);
    fn ns_js_run_inserted_scripts(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_in_template_content(node: *const NsNode) -> GBoolean;
    fn ns_insert_sibling_before(reference: *mut NsNode, node: *mut NsNode);
    fn ns_element_insert_before_single(
        js: *mut NsJs,
        parent: *mut NsNode,
        node: *mut NsNode,
        reference: *mut NsNode,
    );
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = quickjs::context_opaque(scope).cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let name = CString::new(name).unwrap_or_default();
    let message = CString::new(message).unwrap_or_default();
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            name.as_ptr(),
            code,
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(
        southstar_dom::node::KIND_TEXT,
        glib::strdup(text),
        text.len() as u32,
    )
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.0) };
}

pub(crate) fn orphan(js: Js, node: Element) {
    unsafe { ns_js_track_orphan(js.0, node.as_mut_ptr()) };
}

pub(crate) fn unorphan(js: Js, node: Element) {
    unsafe { ns_js_unorphan_node(js.0, node.as_mut_ptr()) };
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    unsafe {
        ns_js_record_child_change(
            js.0,
            parent.as_mut_ptr(),
            raw(added),
            raw(removed),
            raw(previous),
            raw(next),
        )
    };
}

pub(crate) fn index_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
) {
    unsafe { ns_js_index_child_change(js.0, parent.as_mut_ptr(), raw(added), raw(removed)) };
}

fn ptr_array(nodes: &[Element]) -> *mut GPtrArray {
    if nodes.is_empty() {
        return ptr::null_mut();
    }
    let array = unsafe { glib::g_ptr_array_new() };
    for node in nodes {
        unsafe { glib::g_ptr_array_add(array, node.as_mut_ptr().cast()) };
    }
    array
}

fn free_ptr_array(array: *mut GPtrArray) {
    if !array.is_null() {
        unsafe { glib::g_ptr_array_free(array, glib::TRUE) };
    }
}

pub(crate) fn emit_child_list(
    js: Js,
    target: Element,
    added: &[Element],
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    let added = ptr_array(added);
    let removed = ptr_array(removed);
    unsafe {
        ns_mut_record_emit_child_list_arrays(
            js.0,
            target.as_mut_ptr(),
            added,
            removed,
            raw(previous),
            raw(next),
        )
    };
    free_ptr_array(added);
    free_ptr_array(removed);
}

pub(crate) fn mark_childlist_dirty(parent: Element, added: Option<Element>) {
    unsafe { ns_css_mark_childlist_dirty(parent.as_mut_ptr(), raw(added)) };
}

pub(crate) fn ce_disconnect_subtree(js: Js, root: Element) {
    unsafe { ns_ce_disconnect_subtree(js.0, root.as_mut_ptr()) };
}

pub(crate) fn ce_upgrade_subtree_all(js: Js, root: Element) {
    unsafe { ns_ce_upgrade_subtree_all(js.0, root.as_mut_ptr()) };
}

pub(crate) fn iters_pre_remove(js: Js, node: Element) {
    unsafe { ns_node_iters_pre_remove(js.0, node.as_mut_ptr()) };
}

pub(crate) fn run_inserted_scripts(js: Js, root: Element) {
    unsafe { ns_js_run_inserted_scripts(js.0, root.as_mut_ptr()) };
}

pub(crate) fn in_template_content(node: Element) -> bool {
    unsafe { ns_node_in_template_content(node.as_ptr()) != 0 }
}

pub(crate) fn insert_sibling_before(reference: Element, node: Element) {
    unsafe { ns_insert_sibling_before(reference.as_mut_ptr(), node.as_mut_ptr()) };
}

pub(crate) fn insert_before_single(
    js: Option<Js>,
    parent: Element,
    node: Element,
    reference: Element,
) {
    unsafe {
        ns_element_insert_before_single(
            raw_js(js),
            parent.as_mut_ptr(),
            node.as_mut_ptr(),
            reference.as_mut_ptr(),
        )
    };
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

methods! {
    ns_element_appendChild => crate::child::append_child,
    ns_element_removeChild => crate::child::remove_child,
    ns_element_insertBefore => crate::child::insert_before,
    ns_element_replaceChild => crate::child::replace_child,
    ns_element_moveBefore => crate::child::move_before,
    ns_element_before => crate::sequence::before,
    ns_element_after => crate::sequence::after,
    ns_element_replaceWith => crate::sequence::replace_with,
    ns_element_remove_self => crate::sequence::remove,
    ns_element_append => crate::sequence::append,
    ns_element_prepend => crate::sequence::prepend,
    ns_element_replaceChildren => crate::sequence::replace_children,
}
