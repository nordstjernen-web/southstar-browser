//! Southstar — the C ABI of the text bindings, and the js.c and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_style::StyleTable;

use crate::Element;

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

unsafe extern "C" {
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_orphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_record_character_data(js: *mut NsJs, node: *mut NsNode, old_value: *const c_char);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_insert_sibling_before(reference: *mut NsNode, node: *mut NsNode);
    fn ns_qcache_invalidate(js: *const NsJs);
    fn g_utf8_strup(text: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_strdown(text: *const c_char, len: isize) -> *mut c_char;
    fn g_unichar_totitle(c: u32) -> u32;
    fn g_unichar_isspace(c: u32) -> GBoolean;
}

unsafe extern "Rust" {
    safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    nsr_dom_exception(scope, name.as_bytes(), code, message.as_bytes())
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(api::style_table(js.0)) }
}

pub(crate) fn flush_layout(js: Js) {
    api::flush_layout(js.0);
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn clear_children(js: Js, node: Element) {
    unsafe { ns_js_clear_children(js.0, node.as_mut_ptr()) };
}

pub(crate) fn qcache_invalidate(js: Js) {
    unsafe { ns_qcache_invalidate(js.0) };
}

pub(crate) fn mark_restyle_dirty(node: Element) {
    southstar_css::mark_restyle_dirty(node);
}

pub(crate) fn orphan_node(js: Js, node: Element) {
    unsafe { ns_js_orphan_node(js.0, node.as_mut_ptr()) };
}

pub(crate) fn record_character_data(js: Js, node: Element, old: &[u8]) {
    if js.is_null() {
        return;
    }
    let old = CString::new(old).unwrap_or_default();
    unsafe { ns_js_record_character_data(js.0, node.as_mut_ptr(), old.as_ptr()) };
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    if js.is_null() {
        return;
    }
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

pub(crate) fn insert_sibling_before(reference: Element, node: Element) {
    unsafe { ns_insert_sibling_before(reference.as_mut_ptr(), node.as_mut_ptr()) };
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn new_element(name: &[u8]) -> Element {
    southstar_dom::node::new_element(name)
}

pub(crate) fn replace_text(node: Element, text: &[u8]) {
    node.set_text(text);
}

fn take_glib_string(text: *mut c_char) -> Vec<u8> {
    if text.is_null() {
        return Vec::new();
    }
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    unsafe { glib::g_free(text.cast()) };
    bytes
}

pub(crate) fn utf8_upper(text: &CStr) -> Vec<u8> {
    take_glib_string(unsafe { g_utf8_strup(text.as_ptr(), -1) })
}

pub(crate) fn utf8_lower(text: &CStr) -> Vec<u8> {
    take_glib_string(unsafe { g_utf8_strdown(text.as_ptr(), -1) })
}

pub(crate) fn unichar_to_title(c: char) -> char {
    char::from_u32(unsafe { g_unichar_totitle(c as u32) }).unwrap_or(c)
}

pub(crate) fn unichar_is_space(c: char) -> bool {
    unsafe { g_unichar_isspace(c as u32) != 0 }
}
