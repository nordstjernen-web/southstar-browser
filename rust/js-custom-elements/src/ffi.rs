//! Southstar — the C ABI of custom elements, and the js.c calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::Element;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    pub(crate) fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

unsafe extern "C" {
    fn ns_js_main_context(js: *const NsJs) -> ContextHandle;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode;
    fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_wrapper_pinned(js: *const NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_popover_removing(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_log_enabled(js: *const NsJs) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_css_register_defined_element(tag: *const c_char);
}

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "\u{fffd}")).unwrap_or_default()
}

fn text(text: *const c_char) -> Option<String> {
    (!text.is_null()).then(|| {
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    })
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

pub(crate) fn main_scope<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    unsafe { ns_js_main_context(js.ptr()) }.with(f)
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
    unsafe { ns_js_wrapper_pinned(js.ptr(), node.as_ptr()) != 0 }
}

pub(crate) fn node_in_page(js: Js, node: Element) -> bool {
    unsafe { ns_js_node_in_page(js.ptr(), node.as_ptr()) != 0 }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    unsafe { Node::from_ptr(ns_js_current_document(js.ptr())) }
}

pub(crate) fn main_document(js: Js) -> usize {
    unsafe { ns_js_ce_main_doc(js.ptr()) as usize }
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
    unsafe { ns_js_popover_removing(js.ptr(), node.as_mut_ptr()) }
}

pub(crate) fn new_orphan_element(scope: &mut Scope<'_>, name: &str) -> Value {
    api::new_orphan_element(scope, name.as_bytes()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn log_enabled(js: Js) -> bool {
    unsafe { ns_js_log_enabled(js.ptr()) != 0 }
}

pub(crate) fn log_line(js: Js, line: &str) {
    let line = c_string(line);
    unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) }
}

pub(crate) fn register_defined_element(name: &str) {
    let name = c_string(name);
    unsafe { ns_css_register_defined_element(name.as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_name_valid(name: *const c_char) -> GBoolean {
    if name.is_null() {
        return glib::FALSE;
    }
    glib::boolean(crate::name_valid(
        unsafe { CStr::from_ptr(name) }.to_bytes(),
    ))
}

#[unsafe(no_mangle)]
pub fn nsr_ce_define(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    crate::registry::define(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_ce_get(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    crate::registry::get(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_ce_when_defined(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::registry::when_defined(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_ce_upgrade(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    crate::registry::upgrade(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_ce_get_name(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::registry::get_name(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_ce_html_element_construct(scope: &mut Scope<'_>, new_target: &Value) -> Value {
    crate::registry::html_element_construct(scope, new_target).unwrap_or_else(Value::undefined)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_ce_class_for_node(js: *const NsJs, node: *const NsNode) -> Value {
    let (js, Some(node)) = (Js::of(js), unsafe { Node::from_ptr(node) }) else {
        return Value::undefined();
    };
    main_scope(js, |scope| {
        crate::reactions::class_for_node(scope, js, node).unwrap_or_else(Value::undefined)
    })
    .unwrap_or_else(Value::undefined)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_upgrade_element(js: *const NsJs, node: *const NsNode) {
    if let Some(node) = unsafe { Node::from_ptr(node) } {
        crate::reactions::upgrade_element(Js::of(js), node);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_upgrade_subtree_all(js: *const NsJs, root: *const NsNode) {
    if let Some(root) = unsafe { Node::from_ptr(root) } {
        crate::reactions::upgrade_subtree_all(Js::of(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_upgrade_subtree_detached(js: *const NsJs, root: *const NsNode) {
    if let Some(root) = unsafe { Node::from_ptr(root) } {
        crate::reactions::upgrade_subtree_detached(Js::of(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_disconnect_subtree(js: *const NsJs, root: *const NsNode) {
    if let Some(root) = unsafe { Node::from_ptr(root) } {
        crate::reactions::disconnect_subtree(Js::of(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attribute_changed(
    js: *const NsJs,
    node: *const NsNode,
    attr: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    let (Some(node), Some(attr)) = (unsafe { Node::from_ptr(node) }, text(attr)) else {
        return;
    };
    crate::reactions::attribute_changed(
        Js::of(js),
        node,
        &attr,
        text(old_value).as_deref(),
        text(new_value).as_deref(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_has_pending(js: *const NsJs) -> GBoolean {
    let pending =
        crate::existing_page(Js::of(js)).is_some_and(|page| !page.pending.borrow().is_empty());
    glib::boolean(pending)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_upgrading(js: *const NsJs) -> GBoolean {
    let upgrading =
        crate::existing_page(Js::of(js)).is_some_and(|page| page.upgrading.borrow().is_some());
    glib::boolean(upgrading)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_reset(js: *const NsJs) {
    if let Some(page) = crate::existing_page(Js::of(js)) {
        page.reset();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}
