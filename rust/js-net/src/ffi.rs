//! Southstar — the C ABI of the networking bindings, and the js.c, net and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long, c_uint, c_void};
use std::ffi::CString;
use std::rc::Rc;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_bind::host::{self, HostKind};
use southstar_js_bind::{bind_constructor, bind_native};
use southstar_js_engine::{ContextHandle, NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::JsResult;

mod imports;

use imports::*;

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Js(usize);

impl Js {
    pub fn of_ptr(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn of(scope: &Scope<'_>) -> Js {
        Js(scope.context_opaque() as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(ContextHandle);

impl Realm {
    pub fn of(scope: &Scope<'_>) -> Realm {
        Realm(scope.handle())
    }

    pub fn enter(self, f: impl FnOnce(&mut Scope<'_>)) {
        let _ = self.0.with(f);
    }
}

pub(crate) const HO_ABORT_CONTROLLER: HostKind = HostKind::AbortController;
pub(crate) const HO_ABORT_SIGNAL: HostKind = HostKind::AbortSignal;
pub(crate) const HO_XHR: HostKind = HostKind::Xhr;
pub(crate) const HO_XHR_UPLOAD: HostKind = HostKind::XhrUpload;

type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_timeout_add(interval: c_uint, function: GSourceFunc, data: *mut c_void) -> c_uint;

}

#[allow(clippy::unnecessary_cast)]
fn long(value: c_long) -> i64 {
    value as i64
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes.to_vec())).unwrap_or_default()
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

fn before_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

pub(crate) fn url_resolve(base: Option<&[u8]>, url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(base.map(before_nul), before_nul(url))
}

pub(crate) fn url_is_http_or_https(url: &[u8]) -> bool {
    southstar_url::bytes::is_http_or_https(before_nul(url))
}

pub(crate) fn url_same_origin(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => southstar_url::bytes::same_origin(before_nul(a), before_nul(b)),
        _ => false,
    }
}

pub(crate) fn url_origin_from(url: Option<&[u8]>) -> Option<Vec<u8>> {
    url.and_then(|url| southstar_url::bytes::origin_from(before_nul(url)))
}

pub(crate) fn throw_dom(scope: &mut Scope<'_>, name: &CStr, code: c_int, message: &str) -> Value {
    nsr_dom_exception(scope, name.to_bytes(), code, message.as_bytes())
}

pub(crate) fn url_host_from(url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::host_from(before_nul(url))
}

pub(crate) fn hsts_should_upgrade(host: &[u8]) -> bool {
    southstar_net::hsts_should_upgrade(cstring(host).to_bytes())
}

pub(crate) fn enter_handler_realm(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
) -> api::HandlerRealm {
    api::enter_handler_realm(scope, target, kind)
}

pub(crate) fn leave_handler_realm(scope: &mut Scope<'_>, entered: api::HandlerRealm) {
    entered.leave(scope);
}

fn bind_c(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn, arity: u32) {
    bind_native(scope, object, name, arity, f);
}

pub(crate) fn socket_event(scope: &mut Scope<'_>, kind: &CStr) -> Value {
    let event = nsr_event_new(scope).unwrap_or_else(|_| Value::undefined());
    crate::set_str(scope, &event, "type", kind.to_bytes());
    crate::set(scope, &event, "bubbles", Value::boolean(false));
    crate::set(scope, &event, "cancelable", Value::boolean(false));
    crate::set(scope, &event, "defaultPrevented", Value::boolean(false));
    bind_c(
        scope,
        &event,
        "preventDefault",
        nsr_event_prevent_default,
        0,
    );
    bind_c(
        scope,
        &event,
        "stopPropagation",
        nsr_event_stop_propagation,
        0,
    );
    nsr_event_define_cancel_bubble(scope, &event);
    crate::set(scope, &event, "_is_trusted", Value::boolean(true));
    bind_c(
        scope,
        &event,
        "stopImmediatePropagation",
        nsr_event_stop_immediate,
        0,
    );
    bind_c(scope, &event, "composedPath", nsr_event_composed_path, 0);
    event
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &CStr) {
    nsr_event_adopt_interface(scope, event, &iface.to_string_lossy());
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    api::bind_event_target_listeners(scope, object);
    bind_c(scope, object, "dispatchEvent", nsr_target_dispatchEvent, 1);
}

pub(crate) fn bind_socket_ctors(scope: &mut Scope<'_>, global: &Value) {
    bind_constructor(scope, global, "WebSocket", 2, crate::socket::websocket_ctor);
    bind_constructor(
        scope,
        global,
        "EventSource",
        2,
        crate::socket::event_source_ctor,
    );
}

pub(crate) fn install_host_interfaces(scope: &mut Scope<'_>, global: &Value) {
    host::install_attributes(scope, global);
}

pub(crate) fn install_neighbours(scope: &mut Scope<'_>, global: &Value) {
    nsr_net_install_form_data(scope, global);
    nsr_net_install_ports(scope, global);
    nsr_net_install_file_reader(scope, global);
}

pub(crate) enum Transport {
    None,
    WebSocket(southstar_websocket::WebSocket),
    EventSource(southstar_eventsource::EventSource),
}

fn socket_ref(data: usize) -> Rc<crate::socket::Socket> {
    let ptr = data as *const crate::socket::Socket;
    // SAFETY: data is the address of a socket whose Rc the transport's owner keeps alive until the transport is freed.
    unsafe { Rc::increment_strong_count(ptr) };
    // SAFETY: the count was raised above, so this Rc owns one reference.
    unsafe { Rc::from_raw(ptr) }
}

fn socket_busy(data: usize) -> bool {
    socket_ref(data).js.in_pump()
}

fn socket_on_open(data: usize) {
    crate::socket::on_open(&socket_ref(data));
}

fn ws_on_text(text: &[u8], data: usize) {
    crate::socket::on_text(&socket_ref(data), text);
}

fn ws_on_binary(bytes: &[u8], data: usize) {
    crate::socket::on_binary(&socket_ref(data), bytes);
}

fn ws_on_close(code: i32, reason: &[u8], clean: bool, data: usize) {
    crate::socket::on_close(&socket_ref(data), code, reason, clean);
}

fn ws_on_error(message: &[u8], data: usize) {
    crate::socket::on_error(&socket_ref(data), message);
}

fn es_on_message(event: &[u8], text: &[u8], last_id: &[u8], data: usize) {
    crate::socket::on_event_message(&socket_ref(data), event, text, last_id);
}

fn es_on_error(fatal: bool, data: usize) {
    crate::socket::on_event_error(&socket_ref(data), fatal);
}

fn socket_addr(socket: &Rc<crate::socket::Socket>) -> usize {
    Rc::as_ptr(socket) as usize
}

impl Transport {
    pub fn none() -> Transport {
        Transport::None
    }

    pub fn websocket(
        url: &[u8],
        origin: &[u8],
        protocols: Option<&[Vec<u8>]>,
        socket: &Rc<crate::socket::Socket>,
    ) -> Transport {
        let events = southstar_websocket::Events {
            on_open: socket_on_open,
            on_text: ws_on_text,
            on_binary: ws_on_binary,
            on_close: ws_on_close,
            on_error: ws_on_error,
            busy: socket_busy,
        };
        let protocols = protocols
            .unwrap_or_default()
            .iter()
            .map(|p| crate::until_nul(p.clone()))
            .collect();
        Transport::WebSocket(southstar_websocket::WebSocket::open(
            crate::until_nul(url.to_vec()).as_slice(),
            Some(crate::until_nul(origin.to_vec()).as_slice()),
            protocols,
            events,
            socket_addr(socket),
        ))
    }

    pub fn event_source(
        url: &[u8],
        origin: &[u8],
        allow_url: southstar_eventsource::UrlCheck,
        socket: &Rc<crate::socket::Socket>,
    ) -> Transport {
        let events = southstar_eventsource::Events {
            on_open: socket_on_open,
            on_message: es_on_message,
            on_error: es_on_error,
            busy: socket_busy,
        };
        Transport::EventSource(southstar_eventsource::EventSource::open(
            crate::until_nul(url.to_vec()).as_slice(),
            Some(crate::until_nul(origin.to_vec()).as_slice()),
            b"",
            events,
            allow_url,
            socket_addr(socket),
        ))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Transport::None)
    }

    pub fn ws_state(&self) -> i32 {
        match self {
            Transport::WebSocket(ws) => ws.state(),
            _ => 3,
        }
    }

    pub fn protocol(&self) -> Vec<u8> {
        match self {
            Transport::WebSocket(ws) => crate::until_nul(ws.protocol()),
            _ => Vec::new(),
        }
    }

    pub fn send_text(&self, text: &[u8]) {
        if let Transport::WebSocket(ws) = self {
            ws.send_text(text);
        }
    }

    pub fn send_binary(&self, bytes: &[u8]) {
        if let Transport::WebSocket(ws) = self {
            ws.send_binary(bytes);
        }
    }

    pub fn close(&self, code: i32, reason: Option<&[u8]>) {
        match self {
            Transport::WebSocket(ws) => {
                let reason = reason.map(|r| crate::until_nul(r.to_vec()));
                ws.close(code, reason.as_deref());
            }
            Transport::EventSource(es) => es.close(),
            Transport::None => {}
        }
    }

    pub fn free(&mut self) {
        match core::mem::replace(self, Transport::None) {
            Transport::WebSocket(ws) => ws.free(),
            Transport::EventSource(es) => es.free(),
            Transport::None => {}
        }
    }
}

pub(crate) fn abort_error(scope: &mut Scope<'_>) -> Value {
    nsr_make_abort_error(scope)
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &CStr) {
    nsr_target_fire_event(scope, target, &kind.to_string_lossy());
}

pub(crate) fn fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &CStr,
    loaded: f64,
    total: f64,
    computable: bool,
) {
    nsr_xhr_fire_progress_event(
        scope,
        target,
        &kind.to_string_lossy(),
        loaded,
        total,
        computable,
    );
}

pub(crate) fn host_state(scope: &mut Scope<'_>, value: &Value, kind: HostKind) -> Option<Value> {
    host::state(scope, value, kind)
}

pub(crate) fn body_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    api::body_bytes(scope, value)
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Option<Vec<u8>> {
    Some(nsr_blob_bytes(scope, blob))
}

pub(crate) struct Serialized {
    pub body: Vec<u8>,
    pub content_type: Option<Vec<u8>>,
}

pub(crate) fn serialize_form_body(scope: &mut Scope<'_>, value: &Value) -> Option<Serialized> {
    if host::is(value, HostKind::FormData) {
        let (body, content_type) = nsr_js_form_data_serialize(scope, value);
        return Some(Serialized {
            body,
            content_type: Some(crate::until_nul(content_type)),
        });
    }
    if api::is_url_search_params(scope, value) {
        return Some(Serialized {
            body: api::usp_serialize(scope, value),
            content_type: Some(b"application/x-www-form-urlencoded;charset=UTF-8".to_vec()),
        });
    }
    None
}

pub(crate) fn host_construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    kind: HostKind,
) -> JsResult {
    host::construct(scope, new_target, kind)
}

pub(crate) fn host_new(scope: &mut Scope<'_>, kind: HostKind) -> JsResult {
    host::new_default(scope, kind)
}

pub(crate) fn host_is(value: &Value, kind: HostKind) -> bool {
    host::is(value, kind)
}

pub(crate) fn bind_illegal_ctor(scope: &mut Scope<'_>, global: &Value, name: &CStr) {
    bind_constructor(
        scope,
        global,
        &name.to_string_lossy(),
        0,
        api::illegal_constructor,
    );
}

impl Js {
    pub fn log_line(self, line: &[u8]) {
        api::log_line(self.ptr(), &cstring(line));
    }

    pub fn page_url(self) -> Option<Vec<u8>> {
        api::page_url(self.ptr()).map(CString::into_bytes)
    }

    pub fn url_parses(self, url: &[u8]) -> bool {
        !self.is_null() && api::url_parses(self.ptr(), &cstring(url))
    }

    pub fn pump_iteration(self) -> bool {
        !self.is_null() && api::pump_iteration(self.ptr())
    }

    pub fn credit_pumped_time(self, start_us: i64) {
        api::credit_pumped_time(self.ptr(), start_us);
    }

    pub fn connect_check(self, page: Option<&[u8]>) -> southstar_eventsource::UrlCheck {
        let csp = api::with_page_csp(self.ptr(), |csp| csp.cloned());
        let page = page.map(cstring);
        Box::new(move |url| {
            csp.as_ref().is_none_or(|csp| {
                csp.allows(
                    southstar_csp::CONNECT,
                    &cstring(url),
                    page.as_deref(),
                    None,
                    true,
                )
            })
        })
    }

    pub fn csp_allows_connect(self, url: &[u8], page: Option<&[u8]>) -> bool {
        let page = page.map(cstring);
        api::csp_allows_connect(self.ptr(), &cstring(url), page.as_deref())
    }

    pub fn main_context(self) -> Realm {
        Realm(api::main_context(self.ptr()))
    }

    pub fn is_worker(self) -> bool {
        api::is_worker(self.ptr())
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.ptr())
    }

    pub fn realm_url(self, realm: Realm) -> Option<Vec<u8>> {
        api::realm_url(self.ptr(), realm.0).map(CString::into_bytes)
    }

    pub fn perf_now_ms(self) -> f64 {
        if self.is_null() {
            return 0.0;
        }
        southstar_js_perf::page_now_ms(self.ptr())
    }

    pub fn time_origin_us(self) -> i64 {
        api::time_origin_us(self.ptr())
    }

    pub fn add_resource_timing(self, timing: &Timing<'_>, resp: &Response) {
        let info = api::ResourceInfo {
            timeline: timing.timeline.0.as_ptr().cast_const(),
            document_url: api::realm_document_url_ptr(self.ptr(), timing.timeline.0),
            cors_mode: glib::boolean(timing.cors_mode),
            ..api::ResourceInfo::default()
        };
        let url = cstring(timing.url);
        let span = (timing.start_us, timing.end_us);
        // SAFETY: the realm's document URL and the response are live for the call.
        unsafe {
            southstar_js_perf::add_resource_timed(
                self.ptr(),
                Some(&info),
                &url,
                Some(timing.initiator),
                span,
                resp.0.as_ref(),
            )
        };
    }

    pub fn with_budget<R>(self, f: impl FnOnce() -> R) -> R {
        let saved = api::budget_enter(self.ptr());
        let result = f();
        api::budget_leave(self.ptr(), saved);
        result
    }

    pub fn drain_mutations(self) {
        api::drain_mutations(self.ptr());
    }

    pub fn blob_url(self, url: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let url = cstring(url);
        southstar_js_url::blob_url_lookup(self.ptr(), url.to_bytes())
            .map(|(bytes, kind)| (bytes.to_vec(), kind))
    }

    pub fn service_worker_for(self, url: &[u8]) -> Option<ServiceWorker> {
        if self.is_null() || self.is_worker() {
            return None;
        }
        (api::workers_hooks().sw_controller_for)(self.ptr(), &String::from_utf8_lossy(url))
            .map(ServiceWorker)
    }

    pub fn attach_idle(self, func: GSourceFunc, data: *mut c_void) {
        api::attach_idle(self.ptr(), func, data);
    }

    pub fn attach_timeout(self, ms: u32, func: GSourceFunc, data: *mut c_void) {
        api::attach_timeout(self.ptr(), ms, func, data);
    }
}

pub(crate) fn timeout_add(ms: u32, func: GSourceFunc, data: *mut c_void) {
    unsafe { g_timeout_add(ms, func, data) };
}

pub(crate) struct Timing<'a> {
    pub timeline: Realm,
    pub cors_mode: bool,
    pub url: &'a [u8],
    pub initiator: &'a CStr,
    pub start_us: i64,
    pub end_us: i64,
}

pub(crate) struct ServiceWorker(api::ServiceWorkerController);

impl ServiceWorker {
    pub fn post_fetch(&self, id: u32, request: &Request) {
        let headers: Vec<String> = request
            .headers
            .iter()
            .map(|h| h.to_string_lossy().into_owned())
            .collect();
        let fetch = api::ServiceWorkerFetch {
            id,
            url: &request.url.to_string_lossy(),
            method: &request.method.to_string_lossy(),
            headers: &headers,
            body: &request.body,
        };
        (api::workers_hooks().sw_post_fetch)(&self.0, &fetch);
    }
}

pub(crate) use southstar_net::Cancellable;

pub(crate) struct Request {
    pub url: CString,
    pub top: Option<CString>,
    pub method: CString,
    pub body: Vec<u8>,
    pub has_body: bool,
    pub content_type: Option<CString>,
    pub headers: Vec<CString>,
}

impl Request {
    pub fn send(
        &self,
        cancellable: Option<&Cancellable>,
        done: impl FnOnce(Response, Option<Vec<u8>>) + 'static,
    ) {
        let headers: Vec<Vec<u8>> = self.headers.iter().map(|h| h.to_bytes().to_vec()).collect();
        let request = southstar_net::FetchRequest {
            url: self.url.to_bytes(),
            top_url: self.top.as_deref().map(CStr::to_bytes),
            method: Some(self.method.to_bytes()),
            body: self.has_body.then_some(self.body.as_slice()),
            content_type: self.content_type.as_deref().map(CStr::to_bytes),
            headers: &headers,
        };
        southstar_net::request_with(&request, cancellable, move |outcome| match outcome {
            Ok(resp) => done(Response(Some(resp)), None),
            Err(message) => done(Response(None), Some(message)),
        });
    }
}

pub(crate) struct Response(Option<southstar_net::Response>);

fn text_of(value: Option<&CStr>) -> Option<Vec<u8>> {
    value.map(|v| v.to_bytes().to_vec())
}

impl Response {
    pub fn synthesized() -> Response {
        Response(Some(southstar_net::Response::new()))
    }

    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }

    fn get(&self) -> Option<&southstar_net::Response> {
        self.0.as_ref()
    }

    pub fn status(&self) -> i64 {
        self.get().map_or(0, |r| long(r.status()))
    }

    pub fn error(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| text_of(r.error()))
    }

    pub fn final_url(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| text_of(r.final_url()))
    }

    pub fn content_type(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| text_of(r.content_type()))
    }

    pub fn cors_allow_origin(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| text_of(r.cors_allow_origin()))
    }

    pub fn raw_headers(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| text_of(r.raw_headers()))
    }

    pub fn redirect_count(&self) -> i32 {
        self.get()
            .map_or(0, southstar_net::Response::redirect_count)
    }

    pub fn known_headers(&self) -> [(&'static [u8], Option<Vec<u8>>); 5] {
        let field = |f: fn(&southstar_net::Response) -> Option<&CStr>| {
            self.get().and_then(|r| text_of(f(r)))
        };
        [
            (
                b"content-type",
                field(southstar_net::Response::content_type),
            ),
            (
                b"content-disposition",
                field(southstar_net::Response::content_disposition),
            ),
            (
                b"content-security-policy",
                field(southstar_net::Response::csp_header),
            ),
            (
                b"x-frame-options",
                field(southstar_net::Response::xframe_options),
            ),
            (
                b"access-control-allow-origin",
                field(southstar_net::Response::cors_allow_origin),
            ),
        ]
    }

    pub fn body(&self) -> &[u8] {
        self.get()
            .and_then(southstar_net::Response::body)
            .unwrap_or_default()
    }

    pub fn set_error(&mut self, message: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_error(message);
        }
    }

    pub fn set_status(&mut self, status: i64) {
        if let Some(r) = self.0.as_mut() {
            r.set_status(status as c_long);
        }
    }

    pub fn set_final_url(&mut self, url: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_final_url(&cstring(url));
        }
    }

    pub fn set_content_type(&mut self, value: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_content_type(&cstring(value));
        }
    }

    pub fn set_cors_allow_origin(&mut self, value: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_cors_allow_origin(value);
        }
    }

    pub fn set_raw_headers(&mut self, value: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_raw_headers(value);
        }
    }

    pub fn set_body(&mut self, bytes: &[u8]) {
        if let Some(r) = self.0.as_mut() {
            r.set_body(bytes);
        }
    }
}

pub fn nsr_js_fetch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::fetch::fetch(scope, this, args)
}

#[allow(non_snake_case)]
pub fn nsr_navigator_sendBeacon(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::beacon::send_beacon(scope, this, args)
}

pub fn nsr_window_response_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::body::response_ctor(scope, this, args)
}

pub fn nsr_window_request_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::body::request_ctor(scope, this, args)
}

pub fn nsr_window_xhr_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::xhr::construct(scope, this, args)
}

pub fn nsr_window_abort_controller_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::abort::controller_ctor(scope, this, args)
}

pub fn nsr_fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    crate::body::install(scope, global);
}

pub fn nsr_xhr_install_interface(scope: &mut Scope<'_>, global: &Value) {
    crate::xhr::install(scope, global);
}

pub fn nsr_js_net_install_sockets(scope: &mut Scope<'_>, global: &Value) {
    crate::socket::install(scope, global);
}

pub fn nsr_net_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    crate::install_interfaces(scope, global);
}

pub fn nsr_install_abort_signal_interface(scope: &mut Scope<'_>, global: &Value) {
    crate::abort::install(scope, global);
}

pub fn reset_page(js: *mut NsJs) {
    crate::page_reset(Js::of_ptr(js));
}

pub fn pending_fetch_count(js: *const NsJs) -> usize {
    crate::pending_fetches(Js::of_ptr(js))
}

pub fn pending_xhr_count(js: *const NsJs) -> usize {
    crate::pending_xhrs(Js::of_ptr(js))
}

pub fn pending_socket_count(js: *const NsJs) -> usize {
    crate::pending_sockets(Js::of_ptr(js))
}

pub fn service_worker_fetch_result(js: *const NsJs, id: u32, outcome: crate::fetch::SwOutcome<'_>) {
    crate::fetch::service_worker_result(Js::of_ptr(js), id, outcome);
}

pub(crate) unsafe extern "C" fn on_fetch_idle(data: *mut c_void) -> GBoolean {
    let delivery = unsafe { Box::from_raw(data.cast::<crate::fetch::Delivery>()) };
    crate::fetch::deliver_idle(*delivery);
    glib::FALSE
}

pub(crate) fn schedule_fetch_idle(js: Js, delivery: crate::fetch::Delivery, retry: bool) {
    let data = Box::into_raw(Box::new(delivery)).cast();
    if retry {
        timeout_add(4, on_fetch_idle, data);
    } else {
        js.attach_idle(on_fetch_idle, data);
    }
}

unsafe extern "C" fn on_xhr_idle(data: *mut c_void) -> GBoolean {
    let delivery = unsafe { Box::from_raw(data.cast::<crate::xhr::Delivery>()) };
    crate::xhr::deliver_idle(*delivery);
    glib::FALSE
}

pub(crate) fn schedule_xhr_delivery(delivery: crate::xhr::Delivery) {
    timeout_add(4, on_xhr_idle, Box::into_raw(Box::new(delivery)).cast());
}

unsafe extern "C" fn on_xhr_blocked(data: *mut c_void) -> GBoolean {
    let ticket = unsafe { Box::from_raw(data.cast::<crate::xhr::Ticket>()) };
    crate::xhr::blocked(*ticket);
    glib::FALSE
}

pub(crate) fn schedule_xhr_blocked(js: Js, ticket: crate::xhr::Ticket) {
    js.attach_idle(on_xhr_blocked, Box::into_raw(Box::new(ticket)).cast());
}

pub(crate) fn schedule_xhr_blocked_retry(ticket: crate::xhr::Ticket) {
    timeout_add(4, on_xhr_blocked, Box::into_raw(Box::new(ticket)).cast());
}

pub(crate) unsafe extern "C" fn on_abort_timeout(data: *mut c_void) -> GBoolean {
    let ticket = unsafe { Box::from_raw(data.cast::<crate::abort::TimeoutTicket>()) };
    crate::abort::timeout_fired(*ticket);
    glib::FALSE
}

pub(crate) fn schedule_abort_timeout(js: Js, ms: u32, ticket: crate::abort::TimeoutTicket) {
    let data = Box::into_raw(Box::new(ticket)).cast();
    js.attach_timeout(ms, on_abort_timeout, data);
}

fn teardown(js: *const NsJs) {
    crate::page_teardown(Js::of_ptr(js));
}

pub fn install_runtime_hooks() {
    api::set_net_hooks(api::PageStateHooks {
        init: |_| {},
        teardown,
    });
    southstar_js_runtime::links::set_js_net_links(southstar_js_runtime::links::JsNetLinks {
        net_install_interfaces: crate::ffi::nsr_net_install_interfaces,
        js_net_install_sockets: crate::ffi::nsr_js_net_install_sockets,
        fetch_install_interfaces: crate::ffi::nsr_fetch_install_interfaces,
        install_abort_signal_interface: crate::ffi::nsr_install_abort_signal_interface,
        xhr_install_interface: crate::ffi::nsr_xhr_install_interface,
        window_request_ctor: crate::ffi::nsr_window_request_ctor,
        window_response_ctor: crate::ffi::nsr_window_response_ctor,
        window_abort_controller_ctor: crate::ffi::nsr_window_abort_controller_ctor,
        window_xhr_ctor: crate::ffi::nsr_window_xhr_ctor,
        js_fetch: crate::ffi::nsr_js_fetch,
    });
}
