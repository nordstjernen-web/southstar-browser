//! Southstar — the C ABI and Rust-ABI entry points of Range, Selection, TreeWalker and NodeIterator, and the engine calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_dom::{Node, NsNode, Tree};
use southstar_glib::GBoolean;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

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

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

pub(crate) struct SelectionState {
    pub text: Vec<u8>,
    pub has_range: bool,
    pub rect: [f64; 4],
}

unsafe extern "C" {
    fn ns_js_main_context(js: *const NsJs) -> ContextHandle;
    fn ns_js_selection_state(js: *const NsJs, text: *mut *const c_char, rect: *mut f64)
    -> GBoolean;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
}

unsafe extern "Rust" {
    safe fn nsr_make_dom_rect(scope: &mut Scope<'_>, x: f64, y: f64, w: f64, h: f64) -> Value;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

pub(crate) fn main_scope<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.0 == 0 {
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

pub(crate) fn dom_rect(scope: &mut Scope<'_>, rect: [f64; 4]) -> Value {
    let [x, y, w, h] = rect;
    nsr_make_dom_rect(scope, x, y, w, h)
}

pub(crate) fn selection_state(js: Js) -> SelectionState {
    let mut text: *const c_char = core::ptr::null();
    let mut rect = [0.0f64; 4];
    let has_range =
        js.0 != 0 && unsafe { ns_js_selection_state(js.ptr(), &mut text, rect.as_mut_ptr()) } != 0;
    let text = if text.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_bytes().to_vec()
    };
    SelectionState {
        text,
        has_range,
        rect,
    }
}

pub(crate) fn track_orphan(js: Js, node: Element) {
    unsafe { ns_js_track_orphan(js.ptr(), node.as_mut_ptr()) }
}

pub(crate) fn parse_html(source: &[u8]) -> Tree {
    let end = source.iter().position(|&c| c == 0).unwrap_or(source.len());
    southstar_html_parser::parse_document(&source[..end], true)
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    api::dom_exception(scope, name, code, message)
}

#[unsafe(no_mangle)]
pub fn nsr_window_get_selection(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::selection::get_selection(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_document_create_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::range::create_range(scope, this, args)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub fn nsr_native_range(
    scope: &mut southstar_js_engine::Scope<'_>,
    this: &southstar_js_engine::Value,
    args: &[southstar_js_engine::Value],
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::range::native_range(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_static_range_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::static_range::construct(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_document_create_tree_walker(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::walker::create(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_document_create_node_iterator(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::iterator::create(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tree_walker_install_proto(scope: &mut Scope<'_>, proto: &Value) {
    crate::walker::install_proto(scope, proto);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_iters_pre_remove(js: *const NsJs, removed: *const NsNode) {
    if let Some(removed) = unsafe { Node::from_ptr(removed) } {
        crate::iterator::pre_remove(Js::of(js), removed);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_traversal_teardown(js: *const NsJs) {
    crate::iterator::teardown(Js::of(js));
}
