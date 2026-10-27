//! Southstar — the C ABI and Rust-ABI entry points of Range, Selection, TreeWalker and NodeIterator, and the engine calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::{Node, Tree};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

pub(crate) struct SelectionState {
    pub text: Vec<u8>,
    pub has_range: bool,
    pub rect: [f64; 4],
}

use southstar_js_geometry::nsr_make_dom_rect;

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

pub(crate) fn main_scope<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.ptr()).with(f)
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn dom_rect(scope: &mut Scope<'_>, rect: [f64; 4]) -> Value {
    let [x, y, w, h] = rect;
    nsr_make_dom_rect(scope, x, y, w, h)
}

pub(crate) fn selection_state(js: Js) -> SelectionState {
    let selection = api::selection_state(js.ptr());
    SelectionState {
        text: selection.text.map(CString::into_bytes).unwrap_or_default(),
        has_range: selection.has_range,
        rect: selection.rect,
    }
}

pub(crate) fn track_orphan(js: Js, node: Element) {
    southstar_js_mutation::track_orphan(js.ptr(), node);
}

pub(crate) fn parse_html(source: &[u8]) -> Tree {
    let end = source.iter().position(|&c| c == 0).unwrap_or(source.len());
    southstar_html_parser::parse_document(&source[..end], true)
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    api::dom_exception(scope, name, code, message)
}

pub fn nsr_window_get_selection(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::selection::get_selection(scope, this, args)
}

pub fn nsr_document_create_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::range::create_range(scope, this, args)
}

#[allow(non_snake_case)]
pub fn nsr_native_range(
    scope: &mut southstar_js_engine::Scope<'_>,
    this: &southstar_js_engine::Value,
    args: &[southstar_js_engine::Value],
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::range::native_range(scope, this, args)
}

pub fn nsr_static_range_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::static_range::construct(scope, this, args)
}

pub fn nsr_document_create_tree_walker(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::walker::create(scope, this, args)
}

pub fn nsr_document_create_node_iterator(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::iterator::create(scope, this, args)
}

pub fn nsr_tree_walker_install_proto(scope: &mut Scope<'_>, proto: &Value) {
    crate::walker::install_proto(scope, proto);
}

pub fn iters_pre_remove(js: *const NsJs, removed: Element) {
    crate::iterator::pre_remove(Js::of(js), removed);
}

fn pre_remove(js: *const NsJs, removed: Node<'_>) {
    if let Some(removed) = removed.downgrade().get() {
        iters_pre_remove(js, removed);
    }
}

fn teardown(js: *const NsJs) {
    crate::iterator::teardown(Js::of(js));
}

pub fn install_runtime_hooks() {
    api::set_traversal_hooks(api::TraversalHooks {
        pre_remove,
        teardown,
    });
    southstar_js_runtime::links::set_js_traversal_links(
        southstar_js_runtime::links::JsTraversalLinks {
            window_get_selection: crate::ffi::nsr_window_get_selection,
            static_range_ctor: crate::ffi::nsr_static_range_ctor,
        },
    );
}
