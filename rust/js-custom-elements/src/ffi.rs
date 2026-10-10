//! Southstar — the C ABI of custom elements as declared in src/js_internal.h, and the js.c calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::Element;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
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
    fn JS_GetCallerRealm(ctx: *mut JSContext) -> *mut JSContext;
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode;
    fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_node_wrapper(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_js_wrapper_pinned(js: *const NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_popover_removing(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_new_orphan_element(ctx: *mut JSContext, name: *const c_char) -> JSValue;
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
    Js::of(quickjs::context_opaque(scope).cast())
}

pub(crate) fn main_scope<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn node_wrapper(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_js_node_wrapper(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
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
    let realm = unsafe { JS_GetCallerRealm(quickjs::raw_context(scope)) };
    unsafe {
        quickjs::with_context(realm, |scope| {
            let global = scope.global();
            let document = crate::get(scope, &global, "document");
            unwrap_node(&document)
        })
    }
}

pub(crate) fn popover_removing(js: Js, node: Element) {
    unsafe { ns_js_popover_removing(js.ptr(), node.as_mut_ptr()) }
}

pub(crate) fn new_orphan_element(scope: &mut Scope<'_>, name: &str) -> Value {
    let name = c_string(name);
    let raw = unsafe { ns_js_new_orphan_element(quickjs::raw_context(scope), name.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
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

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_define(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::registry::define) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::registry::get) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_when_defined(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::registry::when_defined) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_upgrade(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::registry::upgrade) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_get_name(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::registry::get_name) }
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
pub unsafe extern "C" fn ns_ce_html_element_construct(
    ctx: *mut JSContext,
    new_target: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let new_target = quickjs::borrow_value(scope, new_target);
            let element = crate::registry::html_element_construct(scope, &new_target);
            quickjs::into_raw(element.unwrap_or_else(Value::undefined))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_class_for_node(js: *const NsJs, node: *const NsNode) -> JSValue {
    let (js, Some(node)) = (Js::of(js), unsafe { Node::from_ptr(node) }) else {
        return quickjs::UNDEFINED;
    };
    main_scope(js, |scope| {
        let class = crate::reactions::class_for_node(scope, js, node);
        quickjs::into_raw(class.unwrap_or_else(Value::undefined))
    })
    .unwrap_or(quickjs::UNDEFINED)
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
