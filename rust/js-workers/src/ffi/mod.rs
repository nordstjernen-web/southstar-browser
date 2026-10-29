//! Southstar — the messaging and worker bindings' exports (C for pointer-only calls, Rust ABI for natives) and the engine, network and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_int, c_void};
use std::ffi::CString;
use std::sync::Arc;

use southstar_js_bind::host::{self, HostKind};
use southstar_js_bind::{make_constructor, natives};
use southstar_js_engine::{ContextHandle, NativeFn, Scope, Value};
use southstar_js_runtime::api::Page;
use southstar_js_runtime::api::{self};

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
    js: Option<Page>,
    saved: i64,
}

impl Budget {
    pub(crate) fn enter(scope: &Scope<'_>) -> Budget {
        let js = Page::current(scope);
        let saved = api::budget_enter(js);
        Budget { js, saved }
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        api::budget_leave(self.js, self.saved);
    }
}

fn port_deliver_job(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    if let [port, data, rest @ ..] = args {
        ports::deliver(scope, port, data, rest.first());
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_delivery(scope: &mut Scope<'_>, args: &[Value]) {
    let _ = api::queue_message_task(scope, port_deliver_job, args);
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

pub fn nsr_port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value) {
    let moved = ports::Transfer {
        old: old_ports.clone(),
        new: new_ports.clone(),
    };
    ports::transfer_commit(scope, &moved);
}

pub fn nsr_net_install_ports(scope: &mut Scope<'_>, global: &Value) {
    ports::install(scope, global);
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Js(usize);

impl Js {
    fn of(page: Option<Page>) -> Js {
        Js(page.map_or(0, Page::addr))
    }

    pub(crate) fn is_null(self) -> bool {
        self.0 == 0
    }

    fn page(self) -> Option<Page> {
        Page::from_addr(self.0)
    }
}

pub(crate) type Ctx = ContextHandle;

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(Page::current(scope))
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
    api::main_context(js.page()).with(f)
}

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

fn without_nul(text: &str) -> String {
    text.replace('\0', "")
}

fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) use southstar_glib::{MainContext, MainLoop};

pub(crate) fn invoke_default(task: impl FnOnce() + Send + 'static) {
    MainContext::global().invoke(task);
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
    host_from_ptr(api::worker_host(js.page()))
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    Some(api::current_url(js.page()).to_string_lossy().into_owned())
}

pub(crate) fn set_current_url(js: Js, url: Option<&str>) {
    let url = url.map(c_string);
    api::set_current_url(js.page(), url.as_deref());
}

pub(crate) fn log_line(js: Js, line: &str) {
    api::log_line(js.page(), &c_string(line));
}

pub(crate) fn halt(js: Js) {
    api::halt(js.page());
}

pub(crate) fn drain_microtasks(js: Js) {
    api::drain_microtasks(js.page());
}

pub(crate) fn drain_mutations(js: Js) {
    api::drain_mutations(js.page());
}

pub(crate) fn js_free(js: Js) {
    if let Some(page) = js.page() {
        api::free(page);
    }
}

pub(crate) fn worker_js_new(host: &Arc<Host>) -> Js {
    let url = c_string(&host.url);
    let origin = c_string(&host.origin);
    let name = c_string(&host.name);
    let options = api::WorkerOptions {
        host: Arc::as_ptr(host).cast_mut().cast(),
        context: host.context.clone(),
        url: Some(&url),
        base_url: Some(&host.base_url),
        origin: Some(&origin),
        name: Some(&name),
        service_worker: host.is_service_worker,
    };
    Js::of(api::new_worker(&options))
}

pub(crate) fn worker_eval(
    _scope: &mut Scope<'_>,
    js: Js,
    source: &str,
    url: &str,
    module: bool,
) -> Result<(), (bool, Value)> {
    api::worker_eval(js.page(), &c_string(source), &c_string(url), module)
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, exception: &Value) -> String {
    let message = api::exception_message(scope, exception);
    String::from_utf8_lossy(&message).into_owned()
}

pub(crate) fn report_error_event_in(js: Js, ctx: Ctx, report: &ErrorReport) {
    southstar_js_dispatch::report_error_event_in(
        js.page(),
        ctx,
        Some(&report.message),
        Some(&report.filename),
        report.lineno,
        report.colno,
    );
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
    api::bind_event_target_listeners(scope, object);
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
        CCtor::Illegal => api::illegal_constructor,
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
    southstar_js_interfaces::link_interface_ctors(scope);
}

pub(crate) fn lock_global_prototypes(scope: &mut Scope<'_>) {
    southstar_js_interfaces::lock_global_prototypes(scope);
}

fn worker_fail_job(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    if let Some(target) = args.first().filter(|target| target.is_object()) {
        nsr_target_fire_event(scope, target, "error");
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_fail_job(scope: &mut Scope<'_>, object: &Value) {
    let _ = api::queue_message_task(scope, worker_fail_job, core::slice::from_ref(object));
}

pub(crate) fn blob_url_lookup(js: Js, url: &str, requester: Option<&str>) -> Option<Vec<u8>> {
    let url = c_string(url);
    let requester = requester.map(str::as_bytes);
    southstar_js_url::blob_url_lookup(js.page(), url.to_bytes(), requester)
        .map(|(bytes, _)| bytes.to_vec())
}

pub(crate) fn doc_base_url(js: Js) -> Option<String> {
    southstar_js_document::doc_base_url(js.page())
        .map(|url| String::from_utf8_lossy(&url).into_owned())
}

pub(crate) fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    southstar_js_loader::decode_data_url(&c_string(url))
}

pub(crate) fn csp_allows_worker(js: Js, url: &str) -> bool {
    api::csp_allows_worker(js.page(), &c_string(url))
}

pub(crate) fn sw_fetch_deliver(js: Js, result: &FetchResult) {
    let text = |s: &Option<String>| s.as_deref().map(|s| c_string(s).into_bytes());
    southstar_js_net::service_worker_fetch_result(
        js.page(),
        result.id,
        southstar_js_net::SwOutcome {
            outcome: result.outcome,
            status: i64::from(result.status),
            content_type: text(&result.content_type),
            raw_headers: text(&result.raw_headers),
            body: &result.body,
            error: text(&result.error),
        },
    );
}

pub(crate) fn worker_now_ms(js: Js) -> f64 {
    let origin_us = api::time_origin_us(js.page());
    southstar_js_perf::relative_ms(southstar_glib::monotonic_us(), origin_us)
}

pub(crate) fn url_resolve(base: Option<&str>, href: &str) -> Option<String> {
    let base = base.map(without_nul);
    let href = without_nul(href);
    southstar_url::bytes::resolve(base.as_deref().map(str::as_bytes), href.as_bytes()).map(lossy)
}

pub(crate) fn url_origin_from(url: &str) -> Option<String> {
    southstar_url::bytes::origin_from(without_nul(url).as_bytes()).map(lossy)
}

pub(crate) fn url_same_origin(a: &str, b: &str) -> bool {
    southstar_url::bytes::same_origin(without_nul(a).as_bytes(), without_nul(b).as_bytes())
}

pub(crate) fn url_is_http_or_https(url: &str) -> bool {
    southstar_url::bytes::is_http_or_https(without_nul(url).as_bytes())
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
    let Some(p) = southstar_url::bytes::parts(without_nul(url).as_bytes()) else {
        return UrlParts::default();
    };
    UrlParts {
        origin: lossy(p.origin),
        protocol: lossy(p.protocol),
        host: lossy(p.host),
        hostname: lossy(p.hostname),
        port: lossy(p.port),
        pathname: lossy(p.pathname),
        search: lossy(p.search),
        hash: lossy(p.hash),
    }
}

pub(crate) struct NetResponse {
    pub received: bool,
    pub status: core::ffi::c_long,
    pub final_url: Option<String>,
    pub body: Option<Vec<u8>>,
    pub error: Option<String>,
}

pub(crate) fn net_get(url: &str, top_url: &str, header: &str) -> NetResponse {
    let text = |s: &str| c_string(s).into_bytes();
    let (url, top_url) = (text(url), text(top_url));
    let headers = [text(header)];
    let request = southstar_net::FetchRequest {
        url: &url,
        top_url: Some(&top_url),
        method: Some(b"GET"),
        headers: &headers,
        ..southstar_net::FetchRequest::default()
    };
    let lossy = |s: &core::ffi::CStr| s.to_string_lossy().into_owned();
    match southstar_net::request_blocking(&request) {
        Ok(response) => NetResponse {
            received: true,
            status: response.status(),
            final_url: response.final_url().map(lossy),
            body: response.body().map(<[u8]>::to_vec),
            error: response.error().map(lossy),
        },
        Err(failure) => NetResponse {
            received: false,
            status: 0,
            final_url: None,
            body: None,
            error: Some(String::from_utf8_lossy(failure.message()).into_owned()),
        },
    }
}

pub fn nsr_worker_install_constructor(scope: &mut Scope<'_>, global: &Value) {
    worker::install_constructor(scope, global);
}

pub fn nsr_sw_install_container(scope: &mut Scope<'_>, navigator: &Value) {
    service::install_container(scope, navigator);
}

pub fn nsr_worker_install_console(scope: &mut Scope<'_>, global: &Value) {
    worker_scope::install_console(scope, global);
}

pub fn nsr_sw_install_scope(scope: &mut Scope<'_>, global: &Value) {
    let Some(host) = worker_host_of(js_of(scope)) else {
        return;
    };
    service::install_scope(scope, global, &host);
}

pub fn nsr_worker_install_location(scope: &mut Scope<'_>, global: &Value, url: &str) {
    worker_scope::install_location(scope, global, url);
}

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

pub fn nsr_worker_report_exception(js: Option<Page>, exception: &Value) -> bool {
    worker::report_exception(Js::of(js), exception)
}

/// # Safety
///
/// `host` is null or a live worker host.
unsafe fn host_closing(host: *const c_void) -> bool {
    // SAFETY: the caller passes null or a live worker host.
    unsafe { host.cast::<Host>().as_ref() }.is_some_and(Host::closing)
}

/// # Safety
///
/// `host` is null or a live worker host.
unsafe fn host_log_line(host: *mut c_void, line: &CStr) {
    let Some(host) = host_from_ptr(host) else {
        return;
    };
    let line = line.to_string_lossy().into_owned();
    invoke_default(move || {
        if !host.owner_alive.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some((js, _, _)) = worker::owner_of(&host) {
            log_line(js, &line);
        }
    });
}

fn teardown(js: Page) {
    worker::teardown(Js::of(Some(js)));
}

pub fn install_runtime_hooks() {
    api::set_workers_hooks(api::WorkersHooks {
        host_closing,
        log_line: host_log_line,
        shape_global: worker_scope::shape_global,
        teardown,
        sw_controller_for,
        sw_post_fetch,
        host_base_url,
        pending: workers_pending,
    });
    southstar_js_clone::links::set_js_workers_links(southstar_js_clone::links::JsWorkersLinks {
        port_transfer_commit: crate::ffi::nsr_port_transfer_commit,
        port_transfer_prepare: crate::ffi::nsr_port_transfer_prepare,
    });
    southstar_js_dispatch::links::set_js_workers_links(
        southstar_js_dispatch::links::JsWorkersLinks {
            worker_report_exception: crate::ffi::nsr_worker_report_exception,
        },
    );
    southstar_js_net::links::set_js_workers_links(southstar_js_net::links::JsWorkersLinks {
        net_install_ports: crate::ffi::nsr_net_install_ports,
    });
    southstar_js_runtime::links::set_js_workers_links(
        southstar_js_runtime::links::JsWorkersLinks {
            worker_performance_clear: crate::ffi::nsr_worker_performance_clear,
            worker_performance_entries: crate::ffi::nsr_worker_performance_entries,
            worker_performance_now: crate::ffi::nsr_worker_performance_now,
            worker_report_error: crate::ffi::nsr_worker_report_error,
            worker_import_scripts: crate::ffi::nsr_worker_import_scripts,
            worker_global_close: crate::ffi::nsr_worker_global_close,
            worker_global_post_message: crate::ffi::nsr_worker_global_post_message,
            window_broadcast_channel: crate::ffi::nsr_window_broadcast_channel,
            window_message_channel: crate::ffi::nsr_window_message_channel,
            sw_install_scope: crate::ffi::nsr_sw_install_scope,
            worker_install_console: crate::ffi::nsr_worker_install_console,
            worker_install_constructor: crate::ffi::nsr_worker_install_constructor,
            sw_install_container: crate::ffi::nsr_sw_install_container,
            worker_promise_rejection_tracker: crate::ffi::nsr_worker_promise_rejection_tracker,
            worker_install_location: crate::ffi::nsr_worker_install_location,
        },
    );
    southstar_js_window::links::set_js_workers_links(southstar_js_window::links::JsWorkersLinks {
        port_transfer_commit: crate::ffi::nsr_port_transfer_commit,
        port_transfer_prepare: crate::ffi::nsr_port_transfer_prepare,
    });
    southstar_js_cssom::links::set_js_workers_links(southstar_js_cssom::links::JsWorkersLinks {
        port_remove_event_listener: crate::ffi::nsr_port_remove_event_listener,
        port_add_event_listener: crate::ffi::nsr_port_add_event_listener,
    });
}

fn sw_controller_for(js: Option<Page>, url: &str) -> Option<api::ServiceWorkerController> {
    service::controller_for(Js::of(js), url)
        .map(|host| api::ServiceWorkerController(Box::new(host)))
}

fn sw_post_fetch(controller: &api::ServiceWorkerController, fetch: &api::ServiceWorkerFetch<'_>) {
    let Some(host) = controller.0.downcast_ref::<Arc<Host>>() else {
        return;
    };
    let request = FetchRequest {
        id: fetch.id,
        url: fetch.url.to_owned(),
        method: fetch.method.to_owned(),
        headers: fetch.headers.to_vec(),
        body: fetch.body.to_vec(),
    };
    service::post_fetch_request(host, request);
}

fn host_base_url(js: Option<Page>) -> Option<CString> {
    let host = api::worker_host(js).cast::<Host>();
    // SAFETY: a worker page's host slot holds null or its live Host, which outlives the page.
    unsafe { host.as_ref() }.map(|host| host.base_url.clone())
}

fn workers_pending(js: Option<Page>) -> bool {
    worker::pending(Js::of(js))
}
