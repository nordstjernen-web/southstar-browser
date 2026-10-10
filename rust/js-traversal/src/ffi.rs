//! Southstar — the C ABI of Range, Selection, TreeWalker and NodeIterator as declared in src/js_internal.h, and the js.c calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
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
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_make_dom_rect(ctx: *mut JSContext, x: f64, y: f64, w: f64, h: f64) -> JSValue;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_selection_state(js: *const NsJs, text: *mut *const c_char, rect: *mut f64)
    -> GBoolean;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(quickjs::context_opaque(scope).cast())
}

pub(crate) fn main_scope<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.0 == 0 {
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

pub(crate) fn dom_rect(scope: &mut Scope<'_>, rect: [f64; 4]) -> Value {
    let [x, y, w, h] = rect;
    let raw = unsafe { ns_make_dom_rect(quickjs::raw_context(scope), x, y, w, h) };
    unsafe { quickjs::take_value(scope, raw) }
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

pub(crate) fn parse_html(source: &[u8]) -> Option<Element> {
    let source = CString::new(
        source
            .iter()
            .copied()
            .take_while(|&c| c != 0)
            .collect::<Vec<u8>>(),
    )
    .unwrap_or_default();
    unsafe { Node::from_ptr(ns_html_parse(source.as_ptr(), -1)) }
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
pub unsafe extern "C" fn ns_window_get_selection(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::selection::get_selection) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_create_range(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::range::create_range) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_native_range(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::range::native_range) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_create_tree_walker(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::walker::create) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_create_node_iterator(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::iterator::create) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tree_walker_install_proto(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            crate::walker::install_proto(scope, &proto);
        })
    }
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
