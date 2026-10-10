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
}

fn raw_ctx(scope: &Scope<'_>) -> *mut JSContext {
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
    let raw = unsafe { ns_message_port_state(raw_ctx(scope), quickjs::raw(port)) };
    let state = unsafe { quickjs::take_value(scope, raw) };
    state.is_object().then_some(state)
}

pub(crate) fn new_port_object(scope: &mut Scope<'_>) -> JsResult {
    let raw = unsafe { ns_message_port_new_object(raw_ctx(scope)) };
    take(scope, raw)
}

pub(crate) fn construct_message_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    let raw = unsafe { ns_message_channel_construct(raw_ctx(scope), quickjs::raw(new_target)) };
    take(scope, raw)
}

pub(crate) fn construct_broadcast_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    let raw = unsafe { ns_broadcast_channel_construct(raw_ctx(scope), quickjs::raw(new_target)) };
    take(scope, raw)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &CStr,
) -> Value {
    unsafe { ns_throw_dom_exception(raw_ctx(scope), name.as_ptr(), code, message.as_ptr()) };
    quickjs::take_exception(scope)
}

pub(crate) fn event_new(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(raw_ctx(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    unsafe { ns_event_define_cancel_bubble(raw_ctx(scope), quickjs::raw(event)) };
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
            raw_ctx(scope),
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
            raw_ctx(scope),
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
            raw_ctx(scope),
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
    unsafe { ns_listeners_compact_dead(raw_ctx(scope), quickjs::raw(owner)) };
}

pub(crate) fn with_receiving_realm<R>(
    scope: &mut Scope<'_>,
    port: &Value,
    f: impl FnOnce(&mut Scope<'_>, &mut Scope<'_>, bool) -> R,
) -> R {
    let ctx = raw_ctx(scope);
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
    let ctx = raw_ctx(scope);
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

use std::ffi::CString;
use std::sync::Arc;

use crate::scope as worker_scope;
use crate::service::{self, FetchRequest, FetchResult};
use crate::worker::{self, ErrorReport, Host};

#[repr(C)]
struct GMainContext {
    _private: [u8; 0],
}

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

#[repr(C)]
struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: core::ffi::c_uint,
}

#[repr(C)]
struct NsResponse {
    status: core::ffi::c_long,
    final_url: *mut c_char,
    _headers: [*mut c_char; 9],
    body: *mut GByteArray,
    error: *mut c_char,
}

#[repr(C)]
struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

#[repr(C)]
struct WorkerRealm {
    host: *const c_void,
    context: *mut GMainContext,
    url: *const c_char,
    base_url: *const c_char,
    origin: *const c_char,
    name: *const c_char,
    is_service_worker: c_int,
}

type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> c_int;
type GDestroyNotify = unsafe extern "C" fn(data: *mut c_void);
type CFunction = quickjs::JSCFunction;

unsafe extern "C" {
    fn g_main_context_new() -> *mut GMainContext;
    fn g_main_context_ref(context: *mut GMainContext) -> *mut GMainContext;
    fn g_main_context_unref(context: *mut GMainContext);
    fn g_main_context_invoke_full(
        context: *mut GMainContext,
        priority: c_int,
        function: GSourceFunc,
        data: *mut c_void,
        notify: Option<GDestroyNotify>,
    );
    fn g_main_context_pending(context: *mut GMainContext) -> c_int;
    fn g_main_context_iteration(context: *mut GMainContext, may_block: c_int) -> c_int;
    fn g_main_context_push_thread_default(context: *mut GMainContext);
    fn g_main_context_pop_thread_default(context: *mut GMainContext);
    fn g_main_loop_new(context: *mut GMainContext, is_running: c_int) -> *mut GMainLoop;
    fn g_main_loop_run(main_loop: *mut GMainLoop);
    fn g_main_loop_quit(main_loop: *mut GMainLoop);
    fn g_main_loop_unref(main_loop: *mut GMainLoop);
    fn g_get_monotonic_time() -> i64;
    fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const u8;

    fn ns_js_worker_host(js: *mut c_void) -> *const c_void;
    fn ns_js_main_context(js: *mut c_void) -> *mut JSContext;
    fn ns_js_current_url(js: *mut c_void) -> *const c_char;
    fn ns_js_set_current_url(js: *mut c_void, url: *const c_char);
    fn ns_js_log_line(js: *mut c_void, line: *const c_char);
    fn ns_js_halt(js: *mut c_void);
    fn ns_drain_microtasks(js: *mut c_void);
    fn ns_drain_mutations(js: *mut c_void);
    fn ns_js_free(js: *mut c_void);
    fn ns_worker_js_new(realm: *const WorkerRealm) -> *mut c_void;
    fn ns_worker_js_eval(
        js: *mut c_void,
        src: *const c_char,
        len: usize,
        url: *const c_char,
        module: c_int,
        exception: *mut JSValue,
    ) -> c_int;
    fn ns_js_exception_message(ctx: *mut JSContext, ex: JSValue) -> *mut c_char;
    fn ns_js_report_error_event_in(
        js: *mut c_void,
        ctx: *mut JSContext,
        message: *const c_char,
        filename: *const c_char,
        lineno: c_int,
        colno: c_int,
    );
    fn ns_js_dispatch_engine_event(ctx: *mut JSContext, target: JSValue, ev: JSValue);
    fn ns_target_fire_event(ctx: *mut JSContext, obj: JSValue, kind: *const c_char);
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_bind_ctor(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        f: CFunction,
        argc: c_int,
    );
    fn ns_make_ctor(ctx: *mut JSContext, f: CFunction, name: *const c_char, argc: c_int)
    -> JSValue;
    fn ns_fetch_install_interfaces(ctx: *mut JSContext, global: JSValue);
    fn ns_install_namespace_object(
        ctx: *mut JSContext,
        global: JSValue,
        name: *const c_char,
        obj: JSValue,
        tag: *const c_char,
    );
    fn ns_js_link_interface_ctors(ctx: *mut JSContext);
    fn ns_js_lock_global_prototypes(ctx: *mut JSContext);
    fn ns_js_blob_url_lookup(
        js: *mut c_void,
        url: *const c_char,
        out_type: *mut *mut c_char,
    ) -> *mut GBytes;
    fn ns_js_doc_base_url(js: *mut c_void) -> *mut c_char;
    fn ns_js_decode_data_url(url: *const c_char, out_len: *mut usize) -> *mut c_char;
    fn ns_js_csp_allows_worker(js: *mut c_void, url: *const c_char) -> c_int;
    fn ns_js_net_sw_fetch_result(
        js: *mut c_void,
        id: core::ffi::c_uint,
        outcome: c_int,
        status: core::ffi::c_long,
        content_type: *const c_char,
        raw_headers: *const c_char,
        body: *const u8,
        body_len: usize,
        error: *const c_char,
    );
    fn ns_perf_relative_ms(now_us: i64, origin_us: i64) -> f64;
    fn ns_js_page_time_origin_us(js: *mut c_void) -> i64;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> c_int;
    fn ns_url_is_http_or_https(url: *const c_char) -> c_int;
    fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts;
    fn ns_url_parts_free(parts: *mut NsUrlParts);
    fn ns_net_request_blocking(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        extra_headers: *const *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut southstar_glib::GError,
    ) -> *mut NsResponse;
    fn ns_response_free(response: *mut NsResponse);

    fn ns_event_prevent_default(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_propagation(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_immediate(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_composed_path(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_returns_resolved_undefined(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_returns_resolved_false(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_returns_resolved_empty_array(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_cache_open(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_illegal_constructor(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_event_ctor(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_response_ctor(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_request_ctor(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Js(usize);

impl Js {
    pub(crate) fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut c_void {
        self.0 as *mut c_void
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Ctx(usize);

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn ctx_of(scope: &Scope<'_>) -> Ctx {
    Ctx(raw_ctx(scope) as usize)
}

pub(crate) fn with_ctx<R>(ctx: Ctx, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    unsafe { quickjs::with_context(ctx.0 as *mut JSContext, f) }
}

pub(crate) fn with_js<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

unsafe fn borrowed(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

unsafe fn taken(p: *mut c_char) -> Option<String> {
    let text = unsafe { borrowed(p) };
    if !p.is_null() {
        unsafe { southstar_glib::g_free(p.cast()) };
    }
    text
}

pub(crate) struct MainContext(usize);

unsafe impl Send for MainContext {}
unsafe impl Sync for MainContext {}

type Task = Box<dyn FnOnce() + Send>;

unsafe extern "C" fn run_task(data: *mut c_void) -> c_int {
    let slot = unsafe { &mut *data.cast::<Option<Task>>() };
    if let Some(task) = slot.take() {
        task();
    }
    0
}

unsafe extern "C" fn drop_task(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Option<Task>>()) });
}

fn invoke_on(context: *mut GMainContext, task: impl FnOnce() + Send + 'static) {
    let slot: Box<Option<Task>> = Box::new(Some(Box::new(task)));
    unsafe {
        g_main_context_invoke_full(
            context,
            0,
            run_task,
            Box::into_raw(slot).cast(),
            Some(drop_task),
        )
    };
}

pub(crate) fn invoke_default(task: impl FnOnce() + Send + 'static) {
    invoke_on(core::ptr::null_mut(), task);
}

impl MainContext {
    pub(crate) fn new() -> MainContext {
        MainContext(unsafe { g_main_context_new() } as usize)
    }

    fn raw(&self) -> *mut GMainContext {
        self.0 as *mut GMainContext
    }

    pub(crate) fn invoke(&self, task: impl FnOnce() + Send + 'static) {
        invoke_on(self.raw(), task);
    }

    pub(crate) fn drain(&self, limit: usize) {
        for _ in 0..limit {
            if unsafe { g_main_context_pending(self.raw()) } == 0 {
                break;
            }
            unsafe { g_main_context_iteration(self.raw(), 0) };
        }
    }

    pub(crate) fn push_thread_default(&self) {
        unsafe { g_main_context_push_thread_default(self.raw()) };
    }

    pub(crate) fn pop_thread_default(&self) {
        unsafe { g_main_context_pop_thread_default(self.raw()) };
    }

    pub(crate) fn new_loop(&self) -> usize {
        unsafe { g_main_loop_new(self.raw(), 0) as usize }
    }
}

impl Clone for MainContext {
    fn clone(&self) -> MainContext {
        MainContext(unsafe { g_main_context_ref(self.raw()) } as usize)
    }
}

impl Drop for MainContext {
    fn drop(&mut self) {
        unsafe { g_main_context_unref(self.raw()) };
    }
}

pub(crate) fn run_loop(main_loop: usize) {
    unsafe { g_main_loop_run(main_loop as *mut GMainLoop) };
}

pub(crate) fn quit_loop(main_loop: usize) {
    unsafe { g_main_loop_quit(main_loop as *mut GMainLoop) };
}

pub(crate) fn unref_loop(main_loop: usize) {
    unsafe { g_main_loop_unref(main_loop as *mut GMainLoop) };
}

fn host_from_ptr(host: *const c_void) -> Option<Arc<Host>> {
    if host.is_null() {
        return None;
    }
    let host = host.cast::<Host>();
    unsafe {
        Arc::increment_strong_count(host);
        Some(Arc::from_raw(host))
    }
}

pub(crate) fn worker_host_of(js: Js) -> Option<Arc<Host>> {
    if js.is_null() {
        return None;
    }
    host_from_ptr(unsafe { ns_js_worker_host(js.ptr()) })
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    unsafe { borrowed(ns_js_current_url(js.ptr())) }
}

pub(crate) fn set_current_url(js: Js, url: Option<&str>) {
    let url = url.map(c_string);
    unsafe {
        ns_js_set_current_url(
            js.ptr(),
            url.as_ref().map_or(core::ptr::null(), |u| u.as_ptr()),
        )
    };
}

pub(crate) fn log_line(js: Js, line: &str) {
    let line = c_string(line);
    unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) };
}

pub(crate) fn halt(js: Js) {
    unsafe { ns_js_halt(js.ptr()) };
}

pub(crate) fn drain_microtasks(js: Js) {
    unsafe { ns_drain_microtasks(js.ptr()) };
}

pub(crate) fn drain_mutations(js: Js) {
    unsafe { ns_drain_mutations(js.ptr()) };
}

pub(crate) fn js_free(js: Js) {
    unsafe { ns_js_free(js.ptr()) };
}

pub(crate) fn worker_js_new(host: &Arc<Host>) -> Js {
    let url = c_string(&host.url);
    let origin = c_string(&host.origin);
    let name = c_string(&host.name);
    let realm = WorkerRealm {
        host: Arc::as_ptr(host).cast(),
        context: host.context.raw(),
        url: url.as_ptr(),
        base_url: host.base_url.as_ptr(),
        origin: origin.as_ptr(),
        name: name.as_ptr(),
        is_service_worker: c_int::from(host.is_service_worker),
    };
    Js(unsafe { ns_worker_js_new(&realm) } as usize)
}

pub(crate) fn worker_eval(
    scope: &mut Scope<'_>,
    js: Js,
    source: &str,
    url: &str,
    module: bool,
) -> Result<(), (bool, Value)> {
    let source = c_string(source);
    let url = c_string(url);
    let mut exception = quickjs::UNDEFINED;
    let status = unsafe {
        ns_worker_js_eval(
            js.ptr(),
            source.as_ptr(),
            source.as_bytes().len(),
            url.as_ptr(),
            c_int::from(module),
            &mut exception,
        )
    };
    let exception = unsafe { quickjs::take_value(scope, exception) };
    match status {
        0 => Ok(()),
        1 => Err((true, exception)),
        _ => Err((false, exception)),
    }
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, exception: &Value) -> String {
    unsafe {
        taken(ns_js_exception_message(
            raw_ctx(scope),
            quickjs::raw(exception),
        ))
    }
    .unwrap_or_default()
}

pub(crate) fn report_error_event_in(js: Js, ctx: Ctx, report: &ErrorReport) {
    let message = c_string(&report.message);
    let filename = c_string(&report.filename);
    unsafe {
        ns_js_report_error_event_in(
            js.ptr(),
            ctx.0 as *mut JSContext,
            message.as_ptr(),
            filename.as_ptr(),
            report.lineno,
            report.colno,
        )
    };
}

pub(crate) fn dispatch_engine_event(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    unsafe {
        ns_js_dispatch_engine_event(raw_ctx(scope), quickjs::raw(target), quickjs::raw(event))
    };
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &str) {
    let kind = c_string(kind);
    unsafe { ns_target_fire_event(raw_ctx(scope), quickjs::raw(target), kind.as_ptr()) };
}

#[derive(Clone, Copy)]
pub(crate) enum CFn {
    PreventDefault,
    StopPropagation,
    StopImmediate,
    ComposedPath,
    DispatchEvent,
    AddEventListener,
    RemoveEventListener,
    ResolvedUndefined,
    ResolvedFalse,
    ResolvedEmptyArray,
    CacheOpen,
}

impl CFn {
    fn function(self) -> CFunction {
        match self {
            CFn::PreventDefault => ns_event_prevent_default,
            CFn::StopPropagation => ns_event_stop_propagation,
            CFn::StopImmediate => ns_event_stop_immediate,
            CFn::ComposedPath => ns_event_composed_path,
            CFn::DispatchEvent => ns_target_dispatchEvent,
            CFn::AddEventListener => ns_target_addEventListener,
            CFn::RemoveEventListener => ns_target_removeEventListener,
            CFn::ResolvedUndefined => ns_returns_resolved_undefined,
            CFn::ResolvedFalse => ns_returns_resolved_false,
            CFn::ResolvedEmptyArray => ns_returns_resolved_empty_array,
            CFn::CacheOpen => ns_cache_open,
        }
    }
}

pub(crate) fn bind_c(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: CFn) {
    let function = quickjs::c_function(scope, name, arity, f.function());
    crate::set(scope, object, name, function);
}

pub(crate) fn bind_c_if_not_callable(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: CFn,
) {
    let current = scope.get(object, name).ok();
    if !current.is_some_and(|current| scope.is_function(&current)) {
        bind_c(scope, object, name, arity, f);
    }
}

pub(crate) fn bind_event_target_listeners(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(raw_ctx(scope), quickjs::raw(object)) };
}

#[derive(Clone, Copy)]
pub(crate) enum CCtor {
    Illegal,
    Event,
    Response,
    Request,
}

pub(crate) fn bind_ctor(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    ctor: CCtor,
) {
    let f: CFunction = match ctor {
        CCtor::Illegal => ns_illegal_constructor,
        CCtor::Event => ns_window_event_ctor,
        CCtor::Response => ns_window_response_ctor,
        CCtor::Request => ns_window_request_ctor,
    };
    let name = c_string(name);
    unsafe {
        ns_bind_ctor(
            raw_ctx(scope),
            quickjs::raw(object),
            name.as_ptr(),
            f,
            arity as c_int,
        )
    };
}

pub(crate) fn make_worker_ctor(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_make_ctor(raw_ctx(scope), ns_worker_ctor, c"Worker".as_ptr(), 1) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    unsafe { ns_fetch_install_interfaces(raw_ctx(scope), quickjs::raw(global)) };
}

pub(crate) fn install_namespace_object(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    object: Value,
    tag: &str,
) {
    let name = c_string(name);
    let tag = c_string(tag);
    unsafe {
        ns_install_namespace_object(
            raw_ctx(scope),
            quickjs::raw(global),
            name.as_ptr(),
            quickjs::into_raw(object),
            tag.as_ptr(),
        )
    };
}

pub(crate) fn link_interface_ctors(scope: &mut Scope<'_>) {
    unsafe { ns_js_link_interface_ctors(raw_ctx(scope)) };
}

pub(crate) fn lock_global_prototypes(scope: &mut Scope<'_>) {
    unsafe { ns_js_lock_global_prototypes(raw_ctx(scope)) };
}

pub(crate) fn queue_fail_job(scope: &mut Scope<'_>, object: &Value) {
    let mut raw = [quickjs::raw(object)];
    unsafe { ns_js_queue_message_task(raw_ctx(scope), worker_fail_job, 1, raw.as_mut_ptr()) };
}

unsafe extern "C" fn worker_fail_job(
    ctx: *mut JSContext,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    if argc >= 1 && !argv.is_null() {
        let target = unsafe { *argv };
        if quickjs::raw_is_object(target) {
            unsafe { ns_target_fire_event(ctx, target, c"error".as_ptr()) };
        }
    }
    quickjs::UNDEFINED
}

pub(crate) fn blob_url_lookup(js: Js, url: &str) -> Option<Vec<u8>> {
    let url = c_string(url);
    let bytes = unsafe { ns_js_blob_url_lookup(js.ptr(), url.as_ptr(), core::ptr::null_mut()) };
    if bytes.is_null() {
        return None;
    }
    let mut len = 0usize;
    let data = unsafe { g_bytes_get_data(bytes, &mut len) };
    Some(if data.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }.to_vec()
    })
}

pub(crate) fn doc_base_url(js: Js) -> Option<String> {
    unsafe { taken(ns_js_doc_base_url(js.ptr())) }
}

pub(crate) fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    let url = c_string(url);
    let mut len = 0usize;
    let data = unsafe { ns_js_decode_data_url(url.as_ptr(), &mut len) };
    if data.is_null() {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec();
    unsafe { southstar_glib::g_free(data.cast()) };
    Some(bytes)
}

pub(crate) fn csp_allows_worker(js: Js, url: &str) -> bool {
    let url = c_string(url);
    unsafe { ns_js_csp_allows_worker(js.ptr(), url.as_ptr()) != 0 }
}

pub(crate) fn sw_fetch_deliver(js: Js, result: &FetchResult) {
    let raw_headers = result.raw_headers.as_deref().map(c_string);
    let content_type = result.content_type.as_deref().map(c_string);
    let error = result.error.as_deref().map(c_string);
    let opt = |s: &Option<CString>| s.as_ref().map_or(core::ptr::null(), |s| s.as_ptr());
    unsafe {
        ns_js_net_sw_fetch_result(
            js.ptr(),
            result.id,
            result.outcome,
            result.status as core::ffi::c_long,
            opt(&content_type),
            opt(&raw_headers),
            if result.body.is_empty() {
                core::ptr::null()
            } else {
                result.body.as_ptr()
            },
            result.body.len(),
            opt(&error),
        )
    };
}

pub(crate) fn worker_now_ms(js: Js) -> f64 {
    unsafe { ns_perf_relative_ms(g_get_monotonic_time(), ns_js_page_time_origin_us(js.ptr())) }
}

pub(crate) fn url_resolve(base: Option<&str>, href: &str) -> Option<String> {
    let base = base.map(c_string);
    let href = c_string(href);
    unsafe {
        taken(ns_url_resolve(
            base.as_ref().map_or(core::ptr::null(), |b| b.as_ptr()),
            href.as_ptr(),
        ))
    }
}

pub(crate) fn url_origin_from(url: &str) -> Option<String> {
    let url = c_string(url);
    unsafe { taken(ns_url_origin_from(url.as_ptr())) }
}

pub(crate) fn url_same_origin(a: &str, b: &str) -> bool {
    let a = c_string(a);
    let b = c_string(b);
    unsafe { ns_url_same_origin(a.as_ptr(), b.as_ptr()) != 0 }
}

pub(crate) fn url_is_http_or_https(url: &str) -> bool {
    let url = c_string(url);
    unsafe { ns_url_is_http_or_https(url.as_ptr()) != 0 }
}

#[derive(Default)]
pub(crate) struct UrlParts {
    pub origin: String,
    pub protocol: String,
    pub host: String,
    pub hostname: String,
    pub port: String,
    pub pathname: String,
    pub search: String,
    pub hash: String,
}

pub(crate) fn url_parts(url: &str) -> UrlParts {
    let url = c_string(url);
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    let Some(p) = (unsafe { parts.as_ref() }) else {
        return UrlParts::default();
    };
    let field = |s: *mut c_char| unsafe { borrowed(s) }.unwrap_or_default();
    let out = UrlParts {
        origin: field(p.origin),
        protocol: field(p.protocol),
        host: field(p.host),
        hostname: field(p.hostname),
        port: field(p.port),
        pathname: field(p.pathname),
        search: field(p.search),
        hash: field(p.hash),
    };
    unsafe { ns_url_parts_free(parts) };
    out
}

pub(crate) struct NetResponse {
    pub received: bool,
    pub status: core::ffi::c_long,
    pub final_url: Option<String>,
    pub body: Option<Vec<u8>>,
    pub error: Option<String>,
}

pub(crate) fn net_get(url: &str, top_url: &str, header: &str) -> NetResponse {
    let url = c_string(url);
    let top_url = c_string(top_url);
    let header = c_string(header);
    let headers = [header.as_ptr(), core::ptr::null()];
    let mut error: *mut southstar_glib::GError = core::ptr::null_mut();
    let response = unsafe {
        ns_net_request_blocking(
            url.as_ptr(),
            top_url.as_ptr(),
            c"GET".as_ptr(),
            core::ptr::null(),
            0,
            core::ptr::null(),
            headers.as_ptr(),
            core::ptr::null_mut(),
            &mut error,
        )
    };
    let gerror = unsafe { error.as_ref() }.and_then(|e| unsafe { borrowed(e.message) });
    if !error.is_null() {
        unsafe { southstar_glib::g_error_free(error) };
    }
    let mut out = NetResponse {
        received: !response.is_null(),
        status: 0,
        final_url: None,
        body: None,
        error: gerror,
    };
    if let Some(r) = unsafe { response.as_ref() } {
        out.status = r.status;
        out.final_url = unsafe { borrowed(r.final_url) };
        out.body = unsafe { r.body.as_ref() }.map(|b| {
            if b.data.is_null() || b.len == 0 {
                Vec::new()
            } else {
                unsafe { core::slice::from_raw_parts(b.data, b.len as usize) }.to_vec()
            }
        });
        if out.error.is_none() {
            out.error = unsafe { borrowed(r.error) };
        }
        unsafe { ns_response_free(response) };
    }
    out
}

unsafe fn native_raw(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! export_native {
    ($name:ident, $f:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            ctx: *mut JSContext,
            this_val: JSValue,
            argc: c_int,
            argv: *mut JSValue,
        ) -> JSValue {
            unsafe { native_raw(ctx, this_val, argc, argv, $f) }
        }
    };
}

export_native!(ns_worker_ctor, worker::construct);
export_native!(ns_worker_global_post_message, worker::global_post_message);
export_native!(ns_worker_global_close, worker::global_close);
export_native!(ns_worker_import_scripts, worker::import_scripts);
export_native!(ns_worker_report_error, worker_scope::report_error);
export_native!(ns_worker_performance_now, worker_scope::performance_now);
export_native!(
    ns_worker_performance_entries,
    worker_scope::performance_entries
);
export_native!(ns_worker_performance_clear, crate::noop);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_transfer_is_port(_ctx: *mut JSContext, v: JSValue) -> c_int {
    unsafe { ns_js_value_is_message_port(v) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_bridge_send(
    ctx: *mut JSContext,
    port: JSValue,
    id: u64,
    data: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let port = quickjs::borrow_value(scope, port);
            let data = quickjs::borrow_value(scope, data);
            let result = worker::bridge_send(scope, &port, id, &data);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_install_constructor(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            worker::install_constructor(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sw_install_container(ctx: *mut JSContext, navigator: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let navigator = quickjs::borrow_value(scope, navigator);
            service::install_container(scope, &navigator);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sw_controller_for(
    js: *mut c_void,
    url: *const c_char,
) -> *const c_void {
    let Some(url) = (unsafe { borrowed(url) }) else {
        return core::ptr::null();
    };
    match service::controller_for(Js(js as usize), &url) {
        Some(host) => Arc::as_ptr(&host).cast(),
        None => core::ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sw_post_fetch_request(
    host: *const c_void,
    id: core::ffi::c_uint,
    url: *const c_char,
    method: *const c_char,
    headers: *const *const c_char,
    body: *const u8,
    body_len: usize,
) {
    let Some(host) = host_from_ptr(host) else {
        return;
    };
    let mut list = Vec::new();
    if !headers.is_null() {
        let mut at = headers;
        while let Some(header) = unsafe { borrowed(*at) } {
            list.push(header);
            at = unsafe { at.add(1) };
        }
    }
    let request = FetchRequest {
        id,
        url: unsafe { borrowed(url) }.unwrap_or_default(),
        method: unsafe { borrowed(method) }.unwrap_or_default(),
        headers: list,
        body: if body.is_null() || body_len == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(body, body_len) }.to_vec()
        },
    };
    service::post_fetch_request(&host, request);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_report_exception(js: *mut c_void, ex: JSValue) -> c_int {
    let js = Js(js as usize);
    let Some(handled) = with_js(js, |scope| {
        let ex = unsafe { quickjs::borrow_value(scope, ex) };
        worker::report_exception(js, &ex)
    }) else {
        return 0;
    };
    c_int::from(handled)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_host_closing(host: *const c_void) -> c_int {
    match unsafe { host.cast::<Host>().as_ref() } {
        Some(host) => c_int::from(host.closing()),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_host_base_url(host: *const c_void) -> *const c_char {
    match unsafe { host.cast::<Host>().as_ref() } {
        Some(host) => host.base_url.as_ptr(),
        None => core::ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_workers_pending(js: *mut c_void) -> c_int {
    c_int::from(worker::pending(Js(js as usize)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_workers_teardown(js: *mut c_void) {
    worker::teardown(Js(js as usize));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_log_cb(line: *const c_char, user_data: *mut c_void) {
    let Some(host) = host_from_ptr(user_data) else {
        return;
    };
    let Some(line) = (unsafe { borrowed(line) }) else {
        return;
    };
    invoke_default(move || {
        if !host.owner_alive.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some((js, _, _)) = worker::owner_of(&host) {
            log_line(js, &line);
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_promise_rejection_tracker(
    ctx: *mut JSContext,
    _promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    _opaque: *mut c_void,
) {
    if is_handled {
        return;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let reason = quickjs::borrow_value(scope, reason);
            worker_scope::unhandled_rejection(scope, &reason);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_install_console(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            worker_scope::install_console(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_install_location(
    ctx: *mut JSContext,
    global: JSValue,
    url: *const c_char,
) {
    let url = unsafe { borrowed(url) }.unwrap_or_default();
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            worker_scope::install_location(scope, &global, &url);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sw_install_scope(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let Some(host) = worker_host_of(js_of(scope)) else {
                return;
            };
            let global = quickjs::borrow_value(scope, global);
            service::install_scope(scope, &global, &host);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_shape_global(ctx: *mut JSContext, service_worker: c_int) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            worker_scope::shape_global(scope, service_worker != 0);
        })
    }
}
