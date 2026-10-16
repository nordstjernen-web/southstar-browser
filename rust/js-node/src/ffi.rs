//! Southstar — the node bindings' calls into the page record and the other binding crates, and the mutation methods' entry points.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GPtrArray};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::Element;

mod imports;
mod query;

pub(crate) use query::*;

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
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
    fn ns_ce_disconnect_subtree(js: *mut NsJs, root: *mut NsNode);
    fn ns_ce_upgrade_subtree_all(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_iters_pre_remove(js: *mut NsJs, removed: *mut NsNode);
    fn ns_js_run_inserted_scripts(js: *mut NsJs, root: *mut NsNode);
    fn ns_insert_sibling_before(reference: *mut NsNode, node: *mut NsNode);
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = scope.context_opaque().cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    imports::nsr_dom_exception(scope, name.as_bytes(), code, c_prefix(message.as_bytes()))
}

fn c_prefix(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
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
    southstar_css::mark_childlist_dirty(parent, added);
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
    api::in_template_content(node)
}

pub(crate) fn insert_sibling_before(reference: Element, node: Element) {
    unsafe { ns_insert_sibling_before(reference.as_mut_ptr(), node.as_mut_ptr()) };
}

pub(crate) fn insert_before_single(js: Option<Js>, node: Element, reference: Element) {
    api::insert_before_single(raw_js(js), node, reference);
}
