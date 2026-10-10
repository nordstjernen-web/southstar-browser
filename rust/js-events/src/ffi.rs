//! Southstar — the C ABI of the event core as declared in src/js_internal.h, and the js.c calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_event_state(v: JSValue) -> JSValue;
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_new_proto(ctx: *mut JSContext, proto: JSValue) -> JSValue;
    fn ns_perf_realm_now_ms(ctx: *mut JSContext) -> f64;
    fn ns_submit_event_ctor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_js_dispatch_path(
        js: *mut NsJs,
        len: *mut c_uint,
        window: *mut GBoolean,
    ) -> *const *const NsNode;
}

fn ctx_of(scope: &Scope<'_>) -> *mut JSContext {
    quickjs::raw_context(scope)
}

fn js_of(scope: &Scope<'_>) -> *mut NsJs {
    quickjs::context_opaque(scope).cast()
}

pub(crate) fn text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(ctx_of(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn event_state(scope: &mut Scope<'_>, value: &Value) -> Option<Value> {
    let raw = unsafe { ns_event_state(quickjs::raw(value)) };
    let state = unsafe { quickjs::take_value(scope, raw) };
    state.is_object().then_some(state)
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(ctx_of(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn new_event_with_proto(scope: &mut Scope<'_>, proto: &Value) -> JsResult {
    let raw = unsafe { ns_event_new_proto(ctx_of(scope), quickjs::raw(proto)) };
    let event = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, event)
}

pub(crate) fn realm_now_ms(scope: &mut Scope<'_>) -> f64 {
    unsafe { ns_perf_realm_now_ms(ctx_of(scope)) }
}

pub(crate) fn submit_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    quickjs::call_c_function(scope, ns_submit_event_ctor, this, args)
}

pub(crate) fn dispatch_path(scope: &mut Scope<'_>) -> Option<(Vec<Element>, bool)> {
    let js = js_of(scope);
    if js.is_null() {
        return None;
    }
    let mut len: c_uint = 0;
    let mut window: GBoolean = 0;
    let nodes = unsafe { ns_js_dispatch_path(js, &mut len, &mut window) };
    if nodes.is_null() {
        return None;
    }
    let nodes = unsafe { core::slice::from_raw_parts(nodes, len as usize) };
    let nodes = nodes
        .iter()
        .filter_map(|&n| unsafe { Node::from_ptr(n) })
        .collect();
    Some((nodes, window != 0))
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

unsafe fn with_value(ctx: *mut JSContext, value: JSValue, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::ctors::event_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_prevent_default(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::event::prevent_default) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_stop_propagation(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::event::stop_propagation) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_stop_immediate(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::event::stop_immediate) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_composed_path(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::event::composed_path) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_define_cancel_bubble(ctx: *mut JSContext, ev: JSValue) {
    unsafe { with_value(ctx, ev, crate::event::define_cancel_bubble) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_mark_default_prevented(ctx: *mut JSContext, ev: JSValue) {
    unsafe { with_value(ctx, ev, crate::event::mark_default_prevented) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_event(
    ctx: *mut JSContext,
    kind: *const c_char,
    target: *const NsNode,
) -> JSValue {
    let kind = text(kind);
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let target = Node::from_ptr(target);
            quickjs::into_raw(crate::event::make_event(scope, &kind, target))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_adopt_interface(
    ctx: *mut JSContext,
    ev: JSValue,
    iface: *const c_char,
) {
    let iface = text(iface);
    unsafe {
        with_value(ctx, ev, |scope, ev| {
            crate::event::adopt_interface(scope, ev, &iface)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_define_source(ctx: *mut JSContext, ev: JSValue, source: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let source = quickjs::take_value(scope, source);
            let ev = quickjs::borrow_value(scope, ev);
            crate::event::define_source(scope, &ev, source);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_event_attribute_getters(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::install::install_attribute_getters) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_events_install_window_base(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::install::install_window_base) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_events_install_window(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::install::install_window) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_events_install_worker(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::install::install_worker) }
}
