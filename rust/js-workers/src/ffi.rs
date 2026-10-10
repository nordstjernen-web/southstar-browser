//! Southstar — the C ABI of the messaging and worker bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{JsResult, broadcast, ports};

type JobFunc =
    unsafe extern "C" fn(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue;

unsafe extern "C" {
    fn ns_target_handler_realm(
        ctx: *mut JSContext,
        obj: JSValue,
        kind: *const c_char,
        listener_key: *const c_char,
    ) -> *mut JSContext;
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_define_cancel_bubble(ctx: *mut JSContext, ev: JSValue);
    fn ns_target_dispatch_with_event(
        ctx: *mut JSContext,
        obj: JSValue,
        kind: *const c_char,
        ev: JSValue,
    );
    fn ns_js_budget_enter(js: *mut c_void) -> i64;
    fn ns_js_budget_leave(js: *mut c_void, saved: i64);
    fn ns_js_queue_message_task(
        ctx: *mut JSContext,
        func: JobFunc,
        argc: c_int,
        argv: *mut JSValue,
    );
    fn ns_js_value_is_message_port(v: JSValue) -> c_int;
    fn ns_js_value_is_broadcast_channel(v: JSValue) -> c_int;
    fn ns_message_port_state(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn ns_message_port_new_object(ctx: *mut JSContext) -> JSValue;
    fn ns_message_channel_construct(ctx: *mut JSContext, new_target: JSValue) -> JSValue;
    fn ns_broadcast_channel_construct(ctx: *mut JSContext, new_target: JSValue) -> JSValue;
    fn ns_listener_parse_options(
        ctx: *mut JSContext,
        opts: JSValue,
        capture: *mut c_int,
        once: *mut c_int,
        passive: *mut c_int,
        passive_set: *mut c_int,
        signal_out: *mut JSValue,
        strict_signal: c_int,
    ) -> c_int;
    fn ns_listeners_compact_dead(ctx: *mut JSContext, owner: JSValue);
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_port_bridge_send(ctx: *mut JSContext, port: JSValue, id: u64, data: JSValue) -> JSValue;
}

fn ctx_of(scope: &Scope<'_>) -> *mut JSContext {
    quickjs::raw_context(scope)
}

fn take(scope: &mut Scope<'_>, raw: JSValue) -> JsResult {
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn is_message_port(value: &Value) -> bool {
    unsafe { ns_js_value_is_message_port(quickjs::raw(value)) != 0 }
}

pub(crate) fn is_broadcast_channel(value: &Value) -> bool {
    unsafe { ns_js_value_is_broadcast_channel(quickjs::raw(value)) != 0 }
}

pub(crate) fn port_state(scope: &mut Scope<'_>, port: &Value) -> Option<Value> {
    let raw = unsafe { ns_message_port_state(ctx_of(scope), quickjs::raw(port)) };
    let state = unsafe { quickjs::take_value(scope, raw) };
    state.is_object().then_some(state)
}

pub(crate) fn new_port_object(scope: &mut Scope<'_>) -> JsResult {
    let raw = unsafe { ns_message_port_new_object(ctx_of(scope)) };
    take(scope, raw)
}

pub(crate) fn construct_message_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    let raw = unsafe { ns_message_channel_construct(ctx_of(scope), quickjs::raw(new_target)) };
    take(scope, raw)
}

pub(crate) fn construct_broadcast_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    let raw = unsafe { ns_broadcast_channel_construct(ctx_of(scope), quickjs::raw(new_target)) };
    take(scope, raw)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &CStr,
) -> Value {
    unsafe { ns_throw_dom_exception(ctx_of(scope), name.as_ptr(), code, message.as_ptr()) };
    quickjs::take_exception(scope)
}

pub(crate) fn event_new(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(ctx_of(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    unsafe { ns_event_define_cancel_bubble(ctx_of(scope), quickjs::raw(event)) };
}

pub(crate) fn dispatch_with_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    event: &Value,
) {
    let kind = std::ffi::CString::new(kind).unwrap_or_default();
    unsafe {
        ns_target_dispatch_with_event(
            ctx_of(scope),
            quickjs::raw(target),
            kind.as_ptr(),
            quickjs::raw(event),
        )
    };
}

pub(crate) struct Budget {
    js: *mut c_void,
    saved: i64,
}

impl Budget {
    pub(crate) fn enter(scope: &Scope<'_>) -> Budget {
        let js = quickjs::context_opaque(scope);
        let saved = if js.is_null() {
            0
        } else {
            unsafe { ns_js_budget_enter(js) }
        };
        Budget { js, saved }
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        if !self.js.is_null() {
            unsafe { ns_js_budget_leave(self.js, self.saved) };
        }
    }
}

pub(crate) fn queue_delivery(scope: &mut Scope<'_>, args: &[Value]) {
    let mut raw: Vec<JSValue> = args.iter().map(quickjs::raw).collect();
    unsafe {
        ns_js_queue_message_task(
            ctx_of(scope),
            ns_port_deliver_job,
            raw.len() as c_int,
            raw.as_mut_ptr(),
        )
    };
}

pub(crate) fn parse_listener_options(
    scope: &mut Scope<'_>,
    options: &Value,
) -> Result<(bool, Value), Value> {
    let mut capture: c_int = 0;
    let mut once: c_int = 0;
    let mut signal = quickjs::into_raw(Value::null());
    let ok = unsafe {
        ns_listener_parse_options(
            ctx_of(scope),
            quickjs::raw(options),
            &mut capture,
            &mut once,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            &mut signal,
            1,
        )
    };
    let signal = unsafe { quickjs::take_value(scope, signal) };
    if ok == 0 {
        return Err(quickjs::take_exception(scope));
    }
    Ok((once != 0, signal))
}

pub(crate) fn compact_dead_listeners(scope: &mut Scope<'_>, owner: &Value) {
    unsafe { ns_listeners_compact_dead(ctx_of(scope), quickjs::raw(owner)) };
}

pub(crate) fn bridge_send(scope: &mut Scope<'_>, port: &Value, id: u64, data: &Value) -> JsResult {
    let raw =
        unsafe { ns_port_bridge_send(ctx_of(scope), quickjs::raw(port), id, quickjs::raw(data)) };
    take(scope, raw)
}

pub(crate) fn with_receiving_realm<R>(
    scope: &mut Scope<'_>,
    port: &Value,
    f: impl FnOnce(&mut Scope<'_>, &mut Scope<'_>, bool) -> R,
) -> R {
    let ctx = ctx_of(scope);
    let realm = unsafe {
        ns_target_handler_realm(ctx, quickjs::raw(port), c"message".as_ptr(), c"fn".as_ptr())
    };
    let realm = if realm.is_null() { ctx } else { realm };
    unsafe { quickjs::with_context(realm, |realm_scope| f(scope, realm_scope, realm != ctx)) }
}

pub(crate) fn with_port_realm<R>(
    scope: &mut Scope<'_>,
    port: &Value,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> R {
    let ctx = ctx_of(scope);
    let marker = crate::get(scope, port, "_realm");
    let realm = if scope.is_function(&marker) {
        quickjs::function_realm(scope, &marker).unwrap_or(ctx)
    } else {
        ctx
    };
    if realm == ctx {
        f(scope)
    } else {
        unsafe { quickjs::with_context(realm, f) }
    }
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

#[repr(C)]
pub struct EventAtTarget {
    phase: JSValue,
    current: JSValue,
    nested: c_int,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_at_target_begin(
    ctx: *mut JSContext,
    ev: JSValue,
    target: JSValue,
    st: *mut EventAtTarget,
) {
    let Some(st) = (unsafe { st.as_mut() }) else {
        return;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let ev = quickjs::borrow_value(scope, ev);
            let target = quickjs::borrow_value(scope, target);
            let dispatching = crate::get(scope, &ev, "_dispatching");
            st.nested = c_int::from(scope.to_bool(&dispatching));
            st.phase = quickjs::into_raw(crate::get(scope, &ev, "eventPhase"));
            st.current = quickjs::into_raw(crate::get(scope, &ev, "currentTarget"));
            crate::set(scope, &ev, "currentTarget", target);
            crate::set(scope, &ev, "eventPhase", Value::int(2));
            crate::set(scope, &ev, "_dispatching", Value::boolean(true));
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_at_target_end(
    ctx: *mut JSContext,
    ev: JSValue,
    st: *mut EventAtTarget,
) {
    let Some(st) = (unsafe { st.as_mut() }) else {
        return;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let ev = quickjs::borrow_value(scope, ev);
            let phase = quickjs::take_value(scope, st.phase);
            let current = quickjs::take_value(scope, st.current);
            st.phase = quickjs::UNDEFINED;
            st.current = quickjs::UNDEFINED;
            if st.nested != 0 {
                crate::set(scope, &ev, "eventPhase", phase);
                crate::set(scope, &ev, "currentTarget", current);
                return;
            }
            crate::set(scope, &ev, "eventPhase", Value::int(0));
            crate::set(scope, &ev, "currentTarget", Value::null());
            crate::set(scope, &ev, "_dispatching", Value::boolean(false));
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_deliver_job(
    ctx: *mut JSContext,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    if argc < 2 || argv.is_null() {
        return quickjs::UNDEFINED;
    }
    let raw = unsafe { core::slice::from_raw_parts(argv, argc as usize) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let port = quickjs::borrow_value(scope, raw[0]);
            let data = quickjs::borrow_value(scope, raw[1]);
            let ports = raw.get(2).map(|&p| quickjs::borrow_value(scope, p));
            ports::deliver(scope, &port, &data, ports.as_ref());
        })
    };
    quickjs::UNDEFINED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_new(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let result = ports::new_port(scope);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_bridge_id(ctx: *mut JSContext, port: JSValue) -> u64 {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let port = quickjs::borrow_value(scope, port);
            ports::bridge_id(scope, &port)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_transfer_prepare(
    ctx: *mut JSContext,
    transfer: JSValue,
    source_port: JSValue,
    realm: *mut JSContext,
    old_ports: *mut JSValue,
    new_ports: *mut JSValue,
) -> c_int {
    let realm = if realm.is_null() { ctx } else { realm };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let transfer = quickjs::borrow_value(scope, transfer);
            let source = quickjs::borrow_value(scope, source_port);
            let source = source.is_object().then_some(&source);
            let moved = quickjs::with_context(realm, |realm_scope| {
                ports::transfer_prepare(realm_scope, &transfer, source)
            });
            match moved {
                Some(moved) => {
                    *old_ports = quickjs::into_raw(moved.old);
                    *new_ports = quickjs::into_raw(moved.new);
                    0
                }
                None => {
                    *old_ports = quickjs::UNDEFINED;
                    *new_ports = quickjs::UNDEFINED;
                    let error = ports::transfer_error(scope);
                    quickjs::result_raw(scope, Err(error));
                    -1
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_transfer_commit(
    ctx: *mut JSContext,
    old_ports: JSValue,
    new_ports: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let moved = ports::Transfer {
                old: quickjs::borrow_value(scope, old_ports),
                new: quickjs::borrow_value(scope, new_ports),
            };
            ports::transfer_commit(scope, &moved);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_message_channel(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, ports::message_channel) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_broadcast_channel(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, broadcast::construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_add_event_listener(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, ports::add_event_listener) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_remove_event_listener(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, ports::remove_event_listener) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_ports(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            ports::install(scope, &global);
        })
    }
}
