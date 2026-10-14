//! Southstar — the Rust-ABI entry points of the markup, text and microdata bindings, and the engine and parser calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable, GHashTableIter, GPtrArray};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult, markup, microdata, text};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, node: *mut NsNode, name: *const c_char);
    fn ns_element_replace_all_recorded(js: *mut NsJs, node: *mut NsNode, added: *mut NsNode);
    fn ns_js_script_needs_prepare(js: *mut NsJs, script: *mut NsNode);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_orphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_js_record_child_change_arrays(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut GPtrArray,
        removed: *mut GPtrArray,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_ce_disconnect_subtree(js: *mut NsJs, root: *mut NsNode);
    fn ns_ce_upgrade_subtree_all(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_run_inserted_scripts(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_in_template_content(node: *const NsNode) -> GBoolean;
    fn ns_insert_sibling_before(reference: *mut NsNode, node: *mut NsNode);
    fn ns_mark_scripts_already_started(root: *mut NsNode);
    fn ns_node_is_shadow_root(node: *const NsNode) -> GBoolean;
    fn ns_node_is_embedded_doc(node: *const NsNode) -> GBoolean;
    fn ns_html_parse_fragment_with_scripting(
        context_tag: *const c_char,
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_html_parse_fragment_in_context(
        context: *const NsNode,
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_html_convert_declarative_shadow(root: *mut NsNode);
    fn g_str_hash(v: *const c_void) -> c_uint;
    fn g_str_equal(a: *const c_void, b: *const c_void) -> GBoolean;
}

unsafe extern "Rust" {
    safe fn nsr_make_token_list(
        scope: &mut Scope<'_>,
        element: &Value,
        attr: &'static CStr,
    ) -> Result<Value, Value>;
    safe fn nsr_nodelist_from_array(scope: &mut Scope<'_>, array: Value) -> Value;
    safe fn nsr_element_set_node_value(
        scope: &mut Scope<'_>,
        this: &Value,
        value: &Value,
    ) -> Result<(), Value>;
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

pub(crate) fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
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
    js.and_then(|js| node(unsafe { ns_js_current_document(js.0) }))
}

pub(crate) fn resolve_against_page(js: Option<Js>, href: &CStr) -> Option<Vec<u8>> {
    let base = unsafe { ns_js_current_url(raw_js(js)) };
    if base.is_null() || unsafe { *base } == 0 {
        return None;
    }
    let resolved = unsafe { ns_url_resolve(base, href.as_ptr()) };
    if resolved.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(resolved) }.to_bytes().to_vec();
    unsafe { glib::g_free(resolved.cast()) };
    Some(bytes)
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    unsafe {
        ns_js_set_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr(), value.as_ptr())
    };
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    unsafe { ns_js_remove_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn set_node_value(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    nsr_element_set_node_value(scope, this, val)
}

pub(crate) fn replace_all_recorded(js: Option<Js>, node: Element, added: Option<Element>) {
    unsafe { ns_element_replace_all_recorded(raw_js(js), node.as_mut_ptr(), raw(added)) };
}

pub(crate) fn script_needs_prepare(js: Js, node: Element) {
    unsafe { ns_js_script_needs_prepare(js.0, node.as_mut_ptr()) };
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

pub(crate) fn track_orphan(js: Js, node: Element) {
    unsafe { ns_js_track_orphan(js.0, node.as_mut_ptr()) };
}

pub(crate) fn unorphan(js: Js, node: Element) {
    unsafe { ns_js_unorphan_node(js.0, node.as_mut_ptr()) };
}

pub(crate) fn orphan_node(js: Option<Js>, node: Element) {
    unsafe { ns_js_orphan_node(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn clear_children_unrecorded(node: Element) {
    unsafe { ns_js_clear_children(ptr::null_mut(), node.as_mut_ptr()) };
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    unsafe {
        ns_js_record_child_change(
            js.0,
            parent.as_mut_ptr(),
            raw(added),
            ptr::null_mut(),
            raw(previous),
            raw(next),
        )
    };
}

fn ptr_array(nodes: &[Element]) -> *mut GPtrArray {
    let array = unsafe { glib::g_ptr_array_new() };
    for node in nodes {
        unsafe { glib::g_ptr_array_add(array, node.as_mut_ptr().cast()) };
    }
    array
}

pub(crate) fn record_child_changes(
    js: Js,
    parent: Element,
    added: &[Element],
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    let added = ptr_array(added);
    let removed = ptr_array(removed);
    unsafe {
        ns_js_record_child_change_arrays(
            js.0,
            parent.as_mut_ptr(),
            added,
            removed,
            raw(previous),
            raw(next),
        );
        glib::g_ptr_array_free(added, glib::TRUE);
        glib::g_ptr_array_free(removed, glib::TRUE);
    }
}

pub(crate) fn ce_disconnect_subtree(js: Js, root: Element) {
    unsafe { ns_ce_disconnect_subtree(js.0, root.as_mut_ptr()) };
}

pub(crate) fn ce_upgrade_subtree_all(js: Js, root: Element) {
    unsafe { ns_ce_upgrade_subtree_all(js.0, root.as_mut_ptr()) };
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

pub(crate) fn mark_scripts_already_started(root: Element) {
    unsafe { ns_mark_scripts_already_started(root.as_mut_ptr()) };
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    unsafe { ns_node_is_shadow_root(node.as_ptr()) != 0 }
}

pub(crate) fn hidden_child(node: Element) -> bool {
    unsafe {
        ns_node_is_embedded_doc(node.as_ptr()) != 0 || ns_node_is_shadow_root(node.as_ptr()) != 0
    }
}

pub(crate) fn parse_fragment_for_tag(
    tag: &CStr,
    markup: &[u8],
    scripting: bool,
) -> Option<Element> {
    node(unsafe {
        ns_html_parse_fragment_with_scripting(
            tag.as_ptr(),
            markup.as_ptr().cast(),
            markup.len() as isize,
            glib::boolean(scripting),
        )
    })
}

pub(crate) fn parse_fragment_in_context(
    context: Element,
    markup: &[u8],
    scripting: bool,
) -> Option<Element> {
    node(unsafe {
        ns_html_parse_fragment_in_context(
            context.as_ptr(),
            markup.as_ptr().cast(),
            markup.len() as isize,
            glib::boolean(scripting),
        )
    })
}

pub(crate) fn convert_declarative_shadow(root: Element) {
    unsafe { ns_html_convert_declarative_shadow(root.as_mut_ptr()) };
}

pub(crate) struct NameSet(*mut GHashTable);

impl NameSet {
    pub(crate) fn new() -> NameSet {
        NameSet(unsafe {
            glib::g_hash_table_new_full(
                Some(g_str_hash),
                Some(g_str_equal),
                Some(glib::g_free),
                None,
            )
        })
    }

    pub(crate) fn insert(&self, name: &[u8]) -> bool {
        let key = glib::strdup(name);
        if unsafe { glib::g_hash_table_contains(self.0, key.cast()) } != 0 {
            unsafe { glib::g_free(key.cast()) };
            return false;
        }
        unsafe { glib::g_hash_table_add(self.0, key.cast()) };
        true
    }

    pub(crate) fn names(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut iter = GHashTableIter::new();
        let mut key: *mut c_void = ptr::null_mut();
        unsafe {
            glib::g_hash_table_iter_init(&mut iter, self.0);
            while glib::g_hash_table_iter_next(&mut iter, &mut key, ptr::null_mut()) != 0 {
                out.push(CStr::from_ptr(key.cast()).to_bytes().to_vec());
            }
        }
        out
    }
}

impl Drop for NameSet {
    fn drop(&mut self) {
        unsafe { glib::g_hash_table_destroy(self.0) };
    }
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
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
            #[unsafe(no_mangle)]
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
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

getters! {
    nsr_element_get_textContent => text::text_content_get,
    nsr_element_get_text => text::text_get,
    nsr_element_get_innerHTML => markup::inner_html_get,
    nsr_element_get_outerHTML => markup::outer_html_get,
    nsr_element_get_itemScope => microdata::item_scope_get,
    nsr_element_get_itemId => microdata::item_id_get,
    nsr_element_get_itemType => microdata::item_type_get,
    nsr_element_get_itemProp => microdata::item_prop_get,
    nsr_element_get_itemRef => microdata::item_ref_get,
    nsr_element_get_itemValue => microdata::item_value_get,
    nsr_element_get_properties => microdata::properties_get,
}

setters! {
    nsr_element_set_textContent => text::text_content_set,
    nsr_element_set_text => text::text_set,
    nsr_element_set_innerHTML => markup::inner_html_set,
    nsr_element_set_outerHTML => markup::outer_html_set,
    nsr_element_set_itemScope => microdata::item_scope_set,
    nsr_element_set_itemId => microdata::item_id_set,
    nsr_element_set_itemValue => microdata::item_value_set,
}

methods! {
    nsr_element_getHTML => markup::get_html,
    nsr_element_setHTMLUnsafe => markup::set_html_unsafe,
    nsr_element_insertAdjacentHTML => markup::insert_adjacent_html,
    nsr_element_insertAdjacentElement => text::insert_adjacent_element,
    nsr_element_insertAdjacentText => text::insert_adjacent_text,
    nsr_document_getItems => microdata::get_items,
    nsr_xml_serializer_ctor => markup::xml_serializer_ctor,
}
