//! Southstar — the Rust-ABI entry points of the markup, text and microdata bindings, and the engine and parser calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::CString;

use southstar_dom::Tree;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::{Element, JsResult, markup};

#[derive(Clone, Copy)]
pub(crate) struct Js(Page);

use southstar_js_attrs::nsr_make_token_list;
use southstar_js_collections::nsr_nodelist_from_array;
use southstar_js_elements::nsr_element_set_node_value;

fn raw_js(js: Option<Js>) -> Option<Page> {
    js.map(|js| js.0)
}

pub(crate) fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    Page::current(scope).map(Js)
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: i32,
    message: &CStr,
) -> Value {
    api::dom_exception(
        scope,
        &name.to_string_lossy(),
        code,
        &message.to_string_lossy(),
    )
}

pub(crate) fn token_list(scope: &mut Scope<'_>, this: &Value, attr: &'static CStr) -> JsResult {
    nsr_make_token_list(scope, this, attr)
}

pub(crate) fn nodelist_from_array(scope: &mut Scope<'_>, array: Value) -> Value {
    nsr_nodelist_from_array(scope, array)
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    api::array_length(scope, array)
}

pub(crate) fn current_document(js: Option<Js>) -> Option<Element> {
    js.and_then(|js| api::current_document(Some(js.0)))
}

pub(crate) fn resolve_against_page(js: Option<Js>, href: &CStr) -> Option<Vec<u8>> {
    let base = api::current_url(raw_js(js));
    if base.is_empty() {
        return None;
    }
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    southstar_js_mutation::set_attr_recorded(raw_js(js), node, name, value.to_bytes());
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    southstar_js_mutation::remove_attr_recorded(raw_js(js), node, name);
}

pub(crate) fn set_node_value(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    nsr_element_set_node_value(scope, this, val)
}

pub(crate) fn replace_all_recorded(js: Option<Js>, node: Element, added: Option<Element>) {
    southstar_js_mutation::replace_all_recorded(raw_js(js), node, added);
}

pub(crate) fn script_needs_prepare(js: Js, node: Element) {
    southstar_js_loader::script_needs_prepare(Some(js.0), node);
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(Some(js.0));
}

pub(crate) fn track_orphan(js: Js, node: Element) {
    southstar_js_mutation::track_orphan(Some(js.0), node);
}

pub(crate) fn unorphan(js: Js, node: Element) {
    southstar_js_mutation::unorphan_node(Some(js.0), node);
}

pub(crate) fn orphan_node(js: Option<Js>, node: Element) {
    southstar_js_mutation::orphan_node(raw_js(js), node);
}

pub(crate) fn clear_children_unrecorded(node: Element) {
    southstar_js_mutation::clear_children(None, node);
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_mutation::record_child_change(Some(js.0), parent, added, None, previous, next);
}

pub(crate) fn record_child_changes(
    js: Js,
    parent: Element,
    added: &[Element],
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_mutation::record_child_changes(Some(js.0), parent, added, removed, previous, next);
}

pub(crate) fn ce_disconnect_subtree(js: Js, root: Element) {
    southstar_js_custom_elements::disconnect_subtree(Some(js.0), root);
}

pub(crate) fn ce_upgrade_subtree_all(js: Js, root: Element) {
    southstar_js_custom_elements::upgrade_subtree_all(Some(js.0), root);
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

pub(crate) fn mark_scripts_already_started(root: Element) {
    southstar_js_mutation::mark_scripts_already_started(root);
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    southstar_js_shadow::is_shadow_root(node)
}

pub(crate) fn hidden_child(node: Element) -> bool {
    southstar_dom::serialize::is_embedded_doc(node) || southstar_js_shadow::is_shadow_root(node)
}

pub(crate) fn parse_fragment_for_tag(tag: &CStr, markup: &[u8], scripting: bool) -> Tree {
    southstar_html_parser::parse_fragment_for_tag(tag, markup, scripting)
}

pub(crate) fn parse_fragment_in_context(context: Element, markup: &[u8], scripting: bool) -> Tree {
    southstar_html_parser::parse_fragment(Some(context), markup, scripting)
}

pub(crate) fn convert_declarative_shadow(root: Element) {
    southstar_html_parser::attach_declarative_shadow_roots(root);
}

#[derive(Default)]
pub(crate) struct NameSet {
    seen: RefCell<HashSet<Vec<u8>>>,
    order: RefCell<Vec<Vec<u8>>>,
}

impl NameSet {
    pub(crate) fn new() -> NameSet {
        NameSet::default()
    }

    pub(crate) fn insert(&self, name: &[u8]) -> bool {
        let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
        if !self.seen.borrow_mut().insert(name.to_vec()) {
            return false;
        }
        self.order.borrow_mut().push(name.to_vec());
        true
    }

    pub(crate) fn names(&self) -> Vec<Vec<u8>> {
        self.order.borrow().clone()
    }
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
                $f(scope, this)
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
                $f(scope, this, value)
            }
        )*
    };
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

getters! {}

setters! {}

methods! {
    nsr_xml_serializer_ctor => markup::xml_serializer_ctor,
}
