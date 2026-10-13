//! Southstar — the messaging and worker bindings' exports (C for pointer-only calls, Rust ABI for natives) and the engine, network and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_char, c_int, c_void};
use std::ffi::CString;
use std::sync::Arc;

use southstar_js_bind::host::{self, HostKind};
use southstar_js_bind::{make_constructor, natives};
use southstar_js_engine::{ContextHandle, NativeFn, Scope, Value};

use self::imports::*;
use crate::scope as worker_scope;
use crate::service::{self, FetchRequest, FetchResult};
use crate::worker::{self, ErrorReport, Host};
use crate::{JsResult, broadcast, ports};

pub(crate) fn is_message_port(value: &Value) -> bool {
    host::is(value, HostKind::MessagePort)
}

pub(crate) fn is_broadcast_channel(value: &Value) -> bool {
    host::is(value, HostKind::BroadcastChannel)
}

pub(crate) fn port_state(scope: &mut Scope<'_>, port: &Value) -> Option<Value> {
    host::state(scope, port, HostKind::MessagePort)
}

pub(crate) fn new_port_object(scope: &mut Scope<'_>) -> JsResult {
    host::new_default(scope, HostKind::MessagePort)
}

pub(crate) fn construct_message_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    host::construct(scope, new_target, HostKind::MessageChannel)
}

pub(crate) fn construct_broadcast_channel(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    host::construct(scope, new_target, HostKind::BroadcastChannel)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &CStr,
) -> Value {
    nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
}

pub(crate) fn event_new(scope: &mut Scope<'_>) -> Value {
    nsr_event_new(scope).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    nsr_event_define_cancel_bubble(scope, event);
}

pub(crate) fn dispatch_with_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    event: &Value,
) {
    let kind = kind.split('\0').next().unwrap_or_default();
    nsr_target_dispatch_with_event(scope, target, kind, event);
}

pub(crate) struct Budget {
    js: *mut c_void,
    saved: i64,
}

impl Budget {
    pub(crate) fn enter(scope: &Scope<'_>) -> Budget {
        let js = scope.context_opaque();
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

fn port_deliver_job(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    if let [port, data, rest @ ..] = args {
        ports::deliver(scope, port, data, rest.first());
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_delivery(scope: &mut Scope<'_>, args: &[Value]) {
    let _ = nsr_queue_message_task(scope, port_deliver_job, args);
}

pub(crate) fn parse_listener_options(
    scope: &mut Scope<'_>,
    options: &Value,
) -> Result<(bool, Value), Value> {
    let (_, once, _, _, signal) = nsr_listener_parse_options(scope, options, true)?;
    Ok((once, signal.unwrap_or_else(Value::null)))
}

pub(crate) fn compact_dead_listeners(scope: &mut Scope<'_>, owner: &Value) {
    nsr_listeners_compact_dead(scope, owner);
}

pub(crate) fn with_receiving_realm<R>(
    scope: &mut Scope<'_>,
    port: &Value,
    f: impl FnOnce(&mut Scope<'_>, &mut Scope<'_>, bool) -> R,
) -> R {
    let ctx = scope.handle();
    let realm = nsr_target_handler_realm(scope, port, "message", "fn").or(ctx);
    realm
        .with(|realm_scope| f(scope, realm_scope, realm != ctx))
        .expect("a live realm")
}

pub(crate) fn with_port_realm<R>(
    scope: &mut Scope<'_>,
    port: &Value,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> R {
    let ctx = scope.handle();
    let marker = crate::get(scope, port, "_realm");
    let realm = if scope.is_function(&marker) {
        scope
            .function_handle(&marker)
            .ok()
            .and_then(ContextHandle::non_null)
            .unwrap_or(ctx)
    } else {
        ctx
    };
    if realm == ctx {
        return f(scope);
    }
    realm.with(f).expect("a live realm")
}

pub(crate) struct AtTarget {
    pub phase: Value,
    pub current: Value,
    pub nested: bool,
}

pub(crate) fn event_at_target_begin(scope: &mut Scope<'_>, ev: &Value, target: Value) -> AtTarget {
    let dispatching = crate::get(scope, ev, "_dispatching");
    let nested = scope.to_bool(&dispatching);
    let phase = crate::get(scope, ev, "eventPhase");
    let current = crate::get(scope, ev, "currentTarget");
    crate::set(scope, ev, "currentTarget", target);
    crate::set(scope, ev, "eventPhase", Value::int(2));
    crate::set(scope, ev, "_dispatching", Value::boolean(true));
    AtTarget {
        phase,
        current,
        nested,
    }
}

pub(crate) fn event_at_target_end(scope: &mut Scope<'_>, ev: &Value, saved: AtTarget) {
    if saved.nested {
        crate::set(scope, ev, "eventPhase", saved.phase);
        crate::set(scope, ev, "currentTarget", saved.current);
        return;
    }
    crate::set(scope, ev, "eventPhase", Value::int(0));
    crate::set(scope, ev, "currentTarget", Value::null());
    crate::set(scope, ev, "_dispatching", Value::boolean(false));
}

#[unsafe(no_mangle)]
pub fn nsr_event_at_target_begin(scope: &mut Scope<'_>, ev: &Value, target: &Value) -> AtTarget {
    event_at_target_begin(scope, ev, target.clone())
}

#[unsafe(no_mangle)]
pub fn nsr_event_at_target_end(scope: &mut Scope<'_>, ev: &Value, saved: AtTarget) {
    event_at_target_end(scope, ev, saved);
}

#[unsafe(no_mangle)]
pub fn nsr_port_new(scope: &mut Scope<'_>) -> JsResult {
    ports::new_port(scope)
}

#[unsafe(no_mangle)]
pub fn nsr_port_bridge_id(scope: &mut Scope<'_>, port: &Value) -> u64 {
    ports::bridge_id(scope, port)
}

#[unsafe(no_mangle)]
pub fn nsr_port_transfer_prepare(
    scope: &mut Scope<'_>,
    transfer: &Value,
    source_port: &Value,
    realm: ContextHandle,
) -> Result<(Value, Value), Value> {
    let realm = realm.or(scope.handle());
    let source = source_port.is_object().then_some(source_port);
    let moved = realm
        .with(|realm_scope| ports::transfer_prepare(realm_scope, transfer, source))
        .flatten();
    match moved {
        Some(moved) => Ok((moved.old, moved.new)),
        None => Err(ports::transfer_error(scope)),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value) {
    let moved = ports::Transfer {
        old: old_ports.clone(),
        new: new_ports.clone(),
    };
    ports::transfer_commit(scope, &moved);
}

#[unsafe(no_mangle)]
pub fn nsr_net_install_ports(scope: &mut Scope<'_>, global: &Value) {
    ports::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_worker_transfer_is_port(value: &Value) -> bool {
    is_message_port(value)
}

#[unsafe(no_mangle)]
pub fn nsr_port_bridge_send(
    scope: &mut Scope<'_>,
    port: &Value,
    id: u64,
    data: &Value,
) -> JsResult {
    worker::bridge_send(scope, port, id, data)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

natives! {
    nsr_window_message_channel => ports::message_channel,
    nsr_window_broadcast_channel => broadcast::construct,
    nsr_port_add_event_listener => ports::add_event_listener,
    nsr_port_remove_event_listener => ports::remove_event_listener,
    nsr_worker_ctor => worker::construct,
    nsr_worker_global_post_message => worker::global_post_message,
    nsr_worker_global_close => worker::global_close,
    nsr_worker_import_scripts => worker::import_scripts,
    nsr_worker_report_error => worker_scope::report_error,
    nsr_worker_performance_now => worker_scope::performance_now,
    nsr_worker_performance_entries => worker_scope::performance_entries,
    nsr_worker_performance_clear => crate::noop,
}

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

    fn ns_js_budget_enter(js: *mut c_void) -> i64;
    fn ns_js_budget_leave(js: *mut c_void, saved: i64);
    fn ns_js_worker_host(js: *mut c_void) -> *const c_void;
    fn ns_js_main_context(js: *mut c_void) -> ContextHandle;
    fn ns_js_current_url(js: *mut c_void) -> *const c_char;
    fn ns_js_set_current_url(js: *mut c_void, url: *const c_char);
    fn ns_js_log_line(js: *mut c_void, line: *const c_char);
    fn ns_js_halt(js: *mut c_void);
    fn ns_drain_microtasks(js: *mut c_void);
    fn ns_drain_mutations(js: *mut c_void);
    fn ns_js_free(js: *mut c_void);
    fn ns_worker_js_new(realm: *const WorkerRealm) -> *mut c_void;
    fn ns_js_report_error_event_in(
        js: *mut c_void,
        ctx: ContextHandle,
        message: *const c_char,
        filename: *const c_char,
        lineno: c_int,
        colno: c_int,
    );
    fn ns_js_link_interface_ctors(ctx: ContextHandle);
    fn ns_js_lock_global_prototypes(ctx: ContextHandle);
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

pub(crate) type Ctx = ContextHandle;

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque() as usize)
}

pub(crate) fn ctx_of(scope: &Scope<'_>) -> Ctx {
    scope.handle()
}

pub(crate) fn with_ctx<R>(ctx: Ctx, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    ctx.with(f)
}

pub(crate) fn with_js<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    unsafe { ns_js_main_context(js.ptr()) }.with(f)
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
    _scope: &mut Scope<'_>,
    js: Js,
    source: &str,
    url: &str,
    module: bool,
) -> Result<(), (bool, Value)> {
    let source = c_string(source);
    let url = c_string(url);
    unsafe {
        nsr_worker_js_eval(
            js.ptr().cast(),
            &source,
            source.as_bytes().len(),
            &url,
            module,
        )
    }
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, exception: &Value) -> String {
    let message = nsr_js_exception_message(scope, exception);
    String::from_utf8_lossy(&message).into_owned()
}

pub(crate) fn report_error_event_in(js: Js, ctx: Ctx, report: &ErrorReport) {
    let message = c_string(&report.message);
    let filename = c_string(&report.filename);
    unsafe {
        ns_js_report_error_event_in(
            js.ptr(),
            ctx,
            message.as_ptr(),
            filename.as_ptr(),
            report.lineno,
            report.colno,
        )
    };
}

pub(crate) fn dispatch_engine_event(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    nsr_js_dispatch_engine_event(scope, target, event);
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &str) {
    let kind = kind.split('\0').next().unwrap_or_default();
    nsr_target_fire_event(scope, target, kind);
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
    fn function(self) -> NativeFn {
        match self {
            CFn::PreventDefault => nsr_event_prevent_default,
            CFn::StopPropagation => nsr_event_stop_propagation,
            CFn::StopImmediate => nsr_event_stop_immediate,
            CFn::ComposedPath => nsr_event_composed_path,
            CFn::DispatchEvent => nsr_target_dispatchEvent,
            CFn::AddEventListener => nsr_target_addEventListener,
            CFn::RemoveEventListener => nsr_target_removeEventListener,
            CFn::ResolvedUndefined => natives::resolved_undefined,
            CFn::ResolvedFalse => natives::resolved_false,
            CFn::ResolvedEmptyArray => natives::resolved_empty_array,
            CFn::CacheOpen => nsr_cache_open,
        }
    }
}

pub(crate) fn bind_c(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: CFn) {
    let function = scope.function(name, arity, f.function());
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
    nsr_bind_event_target_listeners(scope, object);
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
    let f: NativeFn = match ctor {
        CCtor::Illegal => nsr_illegal_constructor,
        CCtor::Event => nsr_window_event_ctor,
        CCtor::Response => nsr_window_response_ctor,
        CCtor::Request => nsr_window_request_ctor,
    };
    let name = c_string(name);
    let name = name.to_str().unwrap_or_default();
    let constructor = make_constructor(scope, name, arity, f);
    crate::set(scope, object, name, constructor);
}

pub(crate) fn make_worker_ctor(scope: &mut Scope<'_>) -> Value {
    make_constructor(scope, "Worker", 1, nsr_worker_ctor)
}

pub(crate) fn fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    nsr_fetch_install_interfaces(scope, global);
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
    nsr_install_namespace_object(
        scope,
        global,
        name.to_str().unwrap_or_default(),
        object,
        tag.to_str().unwrap_or_default(),
    );
}

pub(crate) fn link_interface_ctors(scope: &mut Scope<'_>) {
    unsafe { ns_js_link_interface_ctors(scope.handle()) };
}

pub(crate) fn lock_global_prototypes(scope: &mut Scope<'_>) {
    unsafe { ns_js_lock_global_prototypes(scope.handle()) };
}

fn worker_fail_job(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    if let Some(target) = args.first().filter(|target| target.is_object()) {
        nsr_target_fire_event(scope, target, "error");
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_fail_job(scope: &mut Scope<'_>, object: &Value) {
    let _ = nsr_queue_message_task(scope, worker_fail_job, core::slice::from_ref(object));
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

#[unsafe(no_mangle)]
pub fn nsr_worker_install_constructor(scope: &mut Scope<'_>, global: &Value) {
    worker::install_constructor(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_sw_install_container(scope: &mut Scope<'_>, navigator: &Value) {
    service::install_container(scope, navigator);
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
pub fn nsr_worker_install_console(scope: &mut Scope<'_>, global: &Value) {
    worker_scope::install_console(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_sw_install_scope(scope: &mut Scope<'_>, global: &Value) {
    let Some(host) = worker_host_of(js_of(scope)) else {
        return;
    };
    service::install_scope(scope, global, &host);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_shape_global(ctx: ContextHandle, service_worker: c_int) {
    ctx.with(|scope| worker_scope::shape_global(scope, service_worker != 0));
}

#[unsafe(no_mangle)]
pub fn nsr_worker_install_location(scope: &mut Scope<'_>, global: &Value, url: &str) {
    worker_scope::install_location(scope, global, url);
}

#[unsafe(no_mangle)]
pub fn nsr_worker_promise_rejection_tracker(
    scope: &mut Scope<'_>,
    _promise: &Value,
    reason: &Value,
    handled: bool,
) {
    if !handled {
        worker_scope::unhandled_rejection(scope, reason);
    }
}

#[unsafe(no_mangle)]
pub fn nsr_worker_report_exception(js: *mut c_void, exception: &Value) -> bool {
    worker::report_exception(Js(js as usize), exception)
}
