//! Southstar — the custom element entry points for other crates and the page record's hooks, and the page-record calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::Element;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub(crate) fn of(js: Option<Page>) -> Js {
        Js(js)
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }
}

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "\u{fffd}")).unwrap_or_default()
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
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

pub(crate) fn node_wrapper(scope: &mut Scope<'_>, node: Element) -> Value {
    api::node_wrapper(scope, node.as_ptr()).unwrap_or_else(Value::undefined)
}

pub(crate) fn wrapper_pinned(js: Js, node: Element) -> bool {
    api::wrapper_pinned(js.ptr(), node)
}

pub(crate) fn node_in_page(js: Js, node: Element) -> bool {
    api::node_in_page(js.ptr(), Some(node))
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.ptr())
}

pub(crate) fn main_document(js: Js) -> usize {
    Node::ptr_or_null(api::ce_main_doc(js.ptr())) as usize
}

pub(crate) fn realm_document(scope: &mut Scope<'_>) -> Option<Element> {
    scope
        .caller_handle()
        .with(|scope| {
            let global = scope.global();
            let document = crate::get(scope, &global, "document");
            unwrap_node(&document)
        })
        .flatten()
}

pub(crate) fn popover_removing(js: Js, node: Element) {
    (api::top_layer_hooks().popover_removing)(js.ptr(), node)
}

pub(crate) fn new_orphan_element(scope: &mut Scope<'_>, name: &str) -> Value {
    api::new_orphan_element(scope, name.as_bytes()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn log_enabled(js: Js) -> bool {
    api::log_enabled(js.ptr())
}

pub(crate) fn log_line(js: Js, line: &str) {
    api::log_line(js.ptr(), &c_string(line));
}

pub(crate) fn register_defined_element(name: &str) {
    southstar_css::register_defined_element(c_string(name).to_bytes());
}

fn element(node: Node<'_>) -> Option<Element> {
    node.downgrade().get()
}

fn lossy(text: &CStr) -> String {
    text.to_string_lossy().into_owned()
}

pub fn html_element_construct(scope: &mut Scope<'_>, new_target: &Value) -> Value {
    crate::registry::html_element_construct(scope, new_target).unwrap_or_else(Value::undefined)
}

pub fn class_for_node(js: Option<Page>, node: Node<'_>) -> Value {
    let (js, Some(node)) = (Js::of(js), element(node)) else {
        return Value::undefined();
    };
    main_scope(js, |scope| {
        crate::reactions::class_for_node(scope, js, node).unwrap_or_else(Value::undefined)
    })
    .unwrap_or_else(Value::undefined)
}

pub fn upgrade_element(js: Option<Page>, node: Node<'_>) {
    if let Some(node) = element(node) {
        crate::reactions::upgrade_element(Js::of(js), node);
    }
}

pub fn upgrade_subtree_all(js: Option<Page>, root: Node<'_>) {
    if let Some(root) = element(root) {
        crate::reactions::upgrade_subtree_all(Js::of(js), root);
    }
}

pub fn upgrade_subtree_detached(js: Option<Page>, root: Node<'_>) {
    if let Some(root) = element(root) {
        crate::reactions::upgrade_subtree_detached(Js::of(js), root);
    }
}

pub fn disconnect_subtree(js: Option<Page>, root: Node<'_>) {
    if let Some(root) = element(root) {
        crate::reactions::disconnect_subtree(Js::of(js), root);
    }
}

fn attribute_changed(
    js: Option<Page>,
    node: Node<'_>,
    attr: &CStr,
    old_value: Option<&CStr>,
    new_value: Option<&CStr>,
) {
    let Some(node) = element(node) else {
        return;
    };
    crate::reactions::attribute_changed(
        Js::of(js),
        node,
        &lossy(attr),
        old_value.map(lossy).as_deref(),
        new_value.map(lossy).as_deref(),
    );
}

pub fn has_pending(js: Option<Page>) -> bool {
    crate::existing_page(Js::of(js)).is_some_and(|page| !page.pending.borrow().is_empty())
}

pub fn upgrading(js: Option<Page>) -> bool {
    crate::existing_page(Js::of(js)).is_some_and(|page| page.upgrading.borrow().is_some())
}

pub fn reset(js: Option<Page>) {
    if let Some(page) = crate::existing_page(Js::of(js)) {
        page.reset();
    }
}

fn teardown(js: Option<Page>) {
    crate::teardown(Js::of(js));
}

pub fn install_runtime_hooks() {
    api::set_custom_elements_hooks(api::CustomElementsHooks {
        name_valid: crate::name_valid,
        attribute_changed,
        teardown: |js| teardown(Some(js)),
        define: crate::registry::define,
        get: crate::registry::get,
        when_defined: crate::registry::when_defined,
        upgrade: crate::registry::upgrade,
        get_name: crate::registry::get_name,
    });
}
