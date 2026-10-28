//! Southstar — the node bindings' calls into the page record and the other binding crates, and the mutation methods' entry points.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::Element;

mod imports;
pub(crate) mod query;

pub(crate) use query::*;

#[derive(Clone, Copy)]
pub(crate) struct Js(Page);

impl Js {
    pub(crate) fn page(self) -> Page {
        self.0
    }
}

fn raw_js(js: Option<Js>) -> Option<Page> {
    js.map(|js| js.0)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    Page::current(scope).map(Js)
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
    api::mark_mutated(Some(js.0));
}

pub(crate) fn orphan(js: Js, node: Element) {
    southstar_js_mutation::track_orphan(Some(js.0), node);
}

pub(crate) fn unorphan(js: Js, node: Element) {
    southstar_js_mutation::unorphan_node(Some(js.0), node);
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_mutation::record_child_change(Some(js.0), parent, added, removed, previous, next);
}

pub(crate) fn index_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
) {
    southstar_js_mutation::index_child_change(Some(js.0), parent, added, removed);
}

pub(crate) fn emit_child_list(
    js: Js,
    target: Element,
    added: &[Element],
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_observers::record_child_list(Some(js.0), target, added, removed, previous, next);
}

pub(crate) fn mark_childlist_dirty(parent: Element, added: Option<Element>) {
    southstar_css::mark_childlist_dirty(parent, added);
}

pub(crate) fn ce_disconnect_subtree(js: Js, root: Element) {
    southstar_js_custom_elements::disconnect_subtree(Some(js.0), root);
}

pub(crate) fn ce_upgrade_subtree_all(js: Js, root: Element) {
    southstar_js_custom_elements::upgrade_subtree_all(Some(js.0), root);
}

pub(crate) fn iters_pre_remove(js: Js, node: Element) {
    southstar_js_traversal::iters_pre_remove(Some(js.0), node);
}

pub(crate) fn run_inserted_scripts(js: Js, root: Element) {
    southstar_js_loader::run_inserted_scripts(Some(js.0), root);
}

pub(crate) fn in_template_content(node: Element) -> bool {
    api::in_template_content(node)
}

pub(crate) fn insert_sibling_before(reference: Element, node: Element) {
    reference.insert_before(node);
}

pub(crate) fn insert_before_single(js: Option<Js>, node: Element, reference: Element) {
    api::insert_before_single(raw_js(js), node, reference);
}
