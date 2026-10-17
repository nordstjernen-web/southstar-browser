//! Southstar — the C ABI of the networking bindings, and the js.c, net and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_glib::{self as glib, GBoolean, GError};
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

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub(crate) struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    content_disposition: *mut c_char,
    csp_header: *mut c_char,
    xframe_options: *mut c_char,
    x_content_type_options: *mut c_char,
    cors_allow_origin: *mut c_char,
    refresh: *mut c_char,
    content_language: *mut c_char,
    raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
    tls_warning: *mut c_char,
    remote_ip: *mut c_char,
    next_hop_protocol: *mut c_char,
    request_start_us: i64,
    request_start_real_ms: f64,
    domain_lookup_ms: f64,
    connect_ms: f64,
    tls_ms: f64,
    pretransfer_ms: f64,
    response_start_ms: f64,
    response_end_ms: f64,
    security: c_int,
    redirect_count: c_int,
}

const _: () = assert!(
    core::mem::size_of::<NsResponse>() == 200
        && core::mem::offset_of!(NsResponse, body) == 88
        && core::mem::offset_of!(NsResponse, response_end_ms) == 184
);

pub(crate) const HO_ABORT_CONTROLLER: HostKind = HostKind::AbortController;
pub(crate) const HO_ABORT_SIGNAL: HostKind = HostKind::AbortSignal;
pub(crate) const HO_XHR: HostKind = HostKind::Xhr;
pub(crate) const HO_XHR_UPLOAD: HostKind = HostKind::XhrUpload;

type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;
type GAsyncReadyCallback =
    unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, data: *mut c_void);

#[repr(C)]
struct WsCallbacks {
    on_open: Option<unsafe extern "C" fn(*mut c_void)>,
    on_text: Option<unsafe extern "C" fn(*const c_char, usize, *mut c_void)>,
    on_binary: Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>,
    on_close: Option<unsafe extern "C" fn(c_int, *const c_char, GBoolean, *mut c_void)>,
    on_error: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
    busy: Option<unsafe extern "C" fn(*mut c_void) -> GBoolean>,
}

#[repr(C)]
struct EsCallbacks {
    on_open: Option<unsafe extern "C" fn(*mut c_void)>,
    on_message:
        Option<unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, *mut c_void)>,
    on_error: Option<unsafe extern "C" fn(GBoolean, *mut c_void)>,
    busy: Option<unsafe extern "C" fn(*mut c_void) -> GBoolean>,
}

unsafe extern "C" {
    fn ns_ws_new(
        url: *const c_char,
        origin: *const c_char,
        protocols: *const *const c_char,
        cbs: *const WsCallbacks,
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn ns_ws_send_text(ws: *mut c_void, text: *const c_char, len: usize) -> GBoolean;
    fn ns_ws_send_binary(ws: *mut c_void, data: *const u8, len: usize) -> GBoolean;
    fn ns_ws_close(ws: *mut c_void, code: c_int, reason: *const c_char);
    fn ns_ws_state_get(ws: *mut c_void) -> c_int;
    fn ns_ws_protocol(ws: *mut c_void) -> *mut c_char;
    fn ns_ws_free(ws: *mut c_void);
    fn ns_es_new(
        url: *const c_char,
        origin: *const c_char,
        last_event_id: *const c_char,
        cbs: *const EsCallbacks,
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn ns_es_close(es: *mut c_void);
    fn ns_es_free(es: *mut c_void);
}

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_timeout_add(interval: c_uint, function: GSourceFunc, data: *mut c_void) -> c_uint;
    fn g_cancellable_new() -> *mut c_void;
    fn g_cancellable_cancel(cancellable: *mut c_void);
    fn g_object_ref(object: *mut c_void) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
    fn g_byte_array_sized_new(reserved: c_uint) -> *mut GByteArray;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;

    fn ns_net_request_async(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        extra_headers: *const *const c_char,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: *mut c_void,
    );
    fn ns_net_fetch_finish(result: *mut c_void, error: *mut *mut GError) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);

    fn ns_sw_controller_for(js: *mut NsJs, abs_url: *const c_char) -> *mut c_void;
    fn ns_sw_post_fetch_request(
        host: *mut c_void,
        id: c_uint,
        url: *const c_char,
        method: *const c_char,
        headers: *const *const c_char,
        body: *const u8,
        body_len: usize,
    );
}

#[allow(clippy::unnecessary_cast)]
fn long(value: c_long) -> i64 {
    value as i64
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes.to_vec())).unwrap_or_default()
}

fn opt_ptr(text: Option<&CString>) -> *const c_char {
    text.map_or(ptr::null(), |t| t.as_ptr())
}

unsafe fn take_gstr(raw: *mut c_char) -> Option<Vec<u8>> {
    if raw.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
    unsafe { glib::g_free(raw.cast()) };
    Some(bytes)
}

fn borrowed(raw: *const c_char) -> Option<Vec<u8>> {
    (!raw.is_null()).then(|| unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec())
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
    WebSocket(*mut c_void),
    EventSource(*mut c_void),
}

unsafe fn socket_ref(data: *mut c_void) -> Rc<crate::socket::Socket> {
    let ptr = data.cast_const().cast::<crate::socket::Socket>();
    unsafe {
        Rc::increment_strong_count(ptr);
        Rc::from_raw(ptr)
    }
}

unsafe fn text_arg<'a>(text: *const c_char) -> &'a [u8] {
    unsafe { glib::bytes(text) }.unwrap_or_default()
}

unsafe extern "C" fn socket_busy(data: *mut c_void) -> GBoolean {
    let socket = unsafe { socket_ref(data) };
    glib::boolean(socket.js.in_pump())
}

unsafe extern "C" fn socket_on_open(data: *mut c_void) {
    let socket = unsafe { socket_ref(data) };
    crate::socket::on_open(&socket);
}

unsafe extern "C" fn ws_on_text(text: *const c_char, len: usize, data: *mut c_void) {
    let socket = unsafe { socket_ref(data) };
    let text = if text.is_null() {
        &[][..]
    } else {
        unsafe { glib::slice(text.cast(), len) }
    };
    crate::socket::on_text(&socket, text);
}

unsafe extern "C" fn ws_on_binary(bytes: *const u8, len: usize, data: *mut c_void) {
    let socket = unsafe { socket_ref(data) };
    let bytes = if bytes.is_null() {
        &[][..]
    } else {
        unsafe { glib::slice(bytes, len) }
    };
    crate::socket::on_binary(&socket, bytes);
}

unsafe extern "C" fn ws_on_close(
    code: c_int,
    reason: *const c_char,
    clean: GBoolean,
    data: *mut c_void,
) {
    let socket = unsafe { socket_ref(data) };
    crate::socket::on_close(&socket, code, unsafe { text_arg(reason) }, clean != 0);
}

unsafe extern "C" fn ws_on_error(message: *const c_char, data: *mut c_void) {
    let socket = unsafe { socket_ref(data) };
    crate::socket::on_error(&socket, unsafe { text_arg(message) });
}

unsafe extern "C" fn es_on_message(
    event: *const c_char,
    text: *const c_char,
    last_id: *const c_char,
    data: *mut c_void,
) {
    let socket = unsafe { socket_ref(data) };
    let (event, text, last_id) = unsafe { (text_arg(event), text_arg(text), text_arg(last_id)) };
    crate::socket::on_event_message(&socket, event, text, last_id);
}

unsafe extern "C" fn es_on_error(fatal: GBoolean, data: *mut c_void) {
    let socket = unsafe { socket_ref(data) };
    crate::socket::on_event_error(&socket, fatal != 0);
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
        let url = cstring(url);
        let origin = cstring(origin);
        let list: Option<Vec<CString>> = protocols.map(|p| p.iter().map(|p| cstring(p)).collect());
        let pointers: Option<Vec<*const c_char>> = list.as_ref().map(|list| {
            list.iter()
                .map(|p| p.as_ptr())
                .chain(core::iter::once(ptr::null()))
                .collect()
        });
        let callbacks = WsCallbacks {
            on_open: Some(socket_on_open),
            on_text: Some(ws_on_text),
            on_binary: Some(ws_on_binary),
            on_close: Some(ws_on_close),
            on_error: Some(ws_on_error),
            busy: Some(socket_busy),
        };
        let ws = unsafe {
            ns_ws_new(
                url.as_ptr(),
                origin.as_ptr(),
                pointers.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
                &callbacks,
                Rc::as_ptr(socket).cast_mut().cast(),
            )
        };
        if ws.is_null() {
            Transport::None
        } else {
            Transport::WebSocket(ws)
        }
    }

    pub fn event_source(
        url: &[u8],
        origin: &[u8],
        socket: &Rc<crate::socket::Socket>,
    ) -> Transport {
        let url = cstring(url);
        let origin = cstring(origin);
        let callbacks = EsCallbacks {
            on_open: Some(socket_on_open),
            on_message: Some(es_on_message),
            on_error: Some(es_on_error),
            busy: Some(socket_busy),
        };
        let es = unsafe {
            ns_es_new(
                url.as_ptr(),
                origin.as_ptr(),
                ptr::null(),
                &callbacks,
                Rc::as_ptr(socket).cast_mut().cast(),
            )
        };
        if es.is_null() {
            Transport::None
        } else {
            Transport::EventSource(es)
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Transport::None)
    }

    pub fn ws_state(&self) -> i32 {
        match self {
            Transport::WebSocket(ws) => unsafe { ns_ws_state_get(*ws) },
            _ => 3,
        }
    }

    pub fn protocol(&self) -> Vec<u8> {
        match self {
            Transport::WebSocket(ws) => {
                unsafe { take_gstr(ns_ws_protocol(*ws)) }.unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    pub fn send_text(&self, text: &[u8]) {
        if let Transport::WebSocket(ws) = self {
            unsafe { ns_ws_send_text(*ws, text.as_ptr().cast(), text.len()) };
        }
    }

    pub fn send_binary(&self, bytes: &[u8]) {
        if let Transport::WebSocket(ws) = self {
            unsafe { ns_ws_send_binary(*ws, bytes.as_ptr(), bytes.len()) };
        }
    }

    pub fn close(&self, code: i32, reason: Option<&[u8]>) {
        match self {
            Transport::WebSocket(ws) => {
                let reason = reason.map(cstring);
                unsafe { ns_ws_close(*ws, code, opt_ptr(reason.as_ref())) };
            }
            Transport::EventSource(es) => unsafe { ns_es_close(*es) },
            Transport::None => {}
        }
    }

    pub fn free(&mut self) {
        match core::mem::replace(self, Transport::None) {
            Transport::WebSocket(ws) => unsafe { ns_ws_free(ws) },
            Transport::EventSource(es) => unsafe { ns_es_free(es) },
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
                resp.0.cast(),
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
        let url = cstring(url);
        let host = unsafe { ns_sw_controller_for(self.ptr(), url.as_ptr()) };
        (!host.is_null()).then_some(ServiceWorker(host))
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

pub(crate) struct ServiceWorker(*mut c_void);

impl ServiceWorker {
    pub fn post_fetch(&self, id: u32, request: &Request) {
        let headers = request.header_pointers();
        unsafe {
            ns_sw_post_fetch_request(
                self.0,
                id,
                request.url.as_ptr(),
                request.method.as_ptr(),
                headers.as_ptr(),
                request.body.as_ptr(),
                request.body.len(),
            )
        };
    }
}

pub(crate) struct Cancellable(*mut c_void);

impl Cancellable {
    pub fn new() -> Cancellable {
        Cancellable(unsafe { g_cancellable_new() })
    }

    pub fn cancel(&self) {
        unsafe { g_cancellable_cancel(self.0) };
    }

    pub fn share(&self) -> Cancellable {
        Cancellable(unsafe { g_object_ref(self.0) })
    }
}

impl Drop for Cancellable {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0) };
    }
}

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
    fn header_pointers(&self) -> Vec<*const c_char> {
        self.headers
            .iter()
            .map(|h| h.as_ptr())
            .chain(core::iter::once(ptr::null()))
            .collect()
    }

    pub fn send(
        &self,
        cancellable: Option<&Cancellable>,
        callback: GAsyncReadyCallback,
        data: *mut c_void,
    ) {
        let headers = self.header_pointers();
        let body = if self.has_body {
            self.body.as_ptr().cast()
        } else {
            ptr::null()
        };
        unsafe {
            ns_net_request_async(
                self.url.as_ptr(),
                opt_ptr(self.top.as_ref()),
                self.method.as_ptr(),
                body,
                self.body.len(),
                opt_ptr(self.content_type.as_ref()),
                headers.as_ptr(),
                cancellable.map_or(ptr::null_mut(), |c| c.0),
                callback,
                data,
            )
        };
    }
}

pub(crate) struct Response(*mut NsResponse);

impl Response {
    pub unsafe fn finish(result: *mut c_void) -> (Response, Option<Vec<u8>>) {
        let mut error: *mut GError = ptr::null_mut();
        let resp = unsafe { ns_net_fetch_finish(result, &mut error) };
        let message = unsafe { error.as_ref() }.map(|e| borrowed(e.message).unwrap_or_default());
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        (Response(resp), message)
    }

    pub fn synthesized() -> Response {
        Response(unsafe { glib::g_malloc0(core::mem::size_of::<NsResponse>()) }.cast())
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    fn get(&self) -> Option<&NsResponse> {
        unsafe { self.0.as_ref() }
    }

    fn get_mut(&mut self) -> Option<&mut NsResponse> {
        unsafe { self.0.as_mut() }
    }

    pub fn status(&self) -> i64 {
        self.get().map_or(0, |r| long(r.status))
    }

    pub fn error(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| borrowed(r.error))
    }

    pub fn final_url(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| borrowed(r.final_url))
    }

    pub fn content_type(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| borrowed(r.content_type))
    }

    pub fn cors_allow_origin(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| borrowed(r.cors_allow_origin))
    }

    pub fn raw_headers(&self) -> Option<Vec<u8>> {
        self.get().and_then(|r| borrowed(r.raw_headers))
    }

    pub fn redirect_count(&self) -> i32 {
        self.get().map_or(0, |r| r.redirect_count)
    }

    pub fn known_headers(&self) -> [(&'static [u8], Option<Vec<u8>>); 5] {
        let field = |f: fn(&NsResponse) -> *mut c_char| self.get().and_then(|r| borrowed(f(r)));
        [
            (b"content-type", field(|r| r.content_type)),
            (b"content-disposition", field(|r| r.content_disposition)),
            (b"content-security-policy", field(|r| r.csp_header)),
            (b"x-frame-options", field(|r| r.xframe_options)),
            (
                b"access-control-allow-origin",
                field(|r| r.cors_allow_origin),
            ),
        ]
    }

    pub fn body(&self) -> &[u8] {
        let Some(body) = self.get().and_then(|r| unsafe { r.body.as_ref() }) else {
            return &[];
        };
        if body.data.is_null() || body.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(body.data, body.len as usize) }
    }

    pub fn set_error(&mut self, message: &[u8]) {
        if let Some(r) = self.get_mut() {
            if !r.error.is_null() {
                unsafe { glib::g_free(r.error.cast()) };
            }
            r.error = glib::strdup(message);
        }
    }

    pub fn set_status(&mut self, status: i64) {
        if let Some(r) = self.get_mut() {
            r.status = status as c_long;
        }
    }

    pub fn set_final_url(&mut self, url: &[u8]) {
        if let Some(r) = self.get_mut() {
            r.final_url = glib::strdup(url);
        }
    }

    pub fn set_content_type(&mut self, value: &[u8]) {
        if let Some(r) = self.get_mut() {
            r.content_type = glib::strdup(value);
        }
    }

    pub fn set_cors_allow_origin(&mut self, value: &[u8]) {
        if let Some(r) = self.get_mut() {
            r.cors_allow_origin = glib::strdup(value);
        }
    }

    pub fn set_raw_headers(&mut self, value: &[u8]) {
        if let Some(r) = self.get_mut() {
            r.raw_headers = glib::strdup(value);
        }
    }

    pub fn set_body(&mut self, bytes: &[u8]) {
        if let Some(r) = self.get_mut() {
            let array = unsafe { g_byte_array_sized_new(bytes.len() as c_uint) };
            if !bytes.is_empty() {
                unsafe { g_byte_array_append(array, bytes.as_ptr(), bytes.len() as c_uint) };
            }
            r.body = array;
        }
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ns_response_free(self.0) };
        }
    }
}

#[unsafe(no_mangle)]
pub fn nsr_js_fetch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::fetch::fetch(scope, this, args)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_navigator_sendBeacon(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::beacon::send_beacon(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_window_response_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::body::response_ctor(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_window_request_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::body::request_ctor(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_window_xhr_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::xhr::construct(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_window_abort_controller_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    crate::abort::controller_ctor(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    crate::body::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_xhr_install_interface(scope: &mut Scope<'_>, global: &Value) {
    crate::xhr::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_js_net_install_sockets(scope: &mut Scope<'_>, global: &Value) {
    crate::socket::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_net_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    crate::install_interfaces(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_abort_signal_interface(scope: &mut Scope<'_>, global: &Value) {
    crate::abort::install(scope, global);
}

pub fn reset_page(js: *mut NsJs) {
    crate::page_reset(Js::of_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_teardown(js: *mut NsJs) {
    crate::page_teardown(Js::of_ptr(js));
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

pub(crate) unsafe extern "C" fn on_beacon_done(
    _source: *mut c_void,
    result: *mut c_void,
    _data: *mut c_void,
) {
    drop(unsafe { Response::finish(result) });
}

pub(crate) unsafe extern "C" fn on_fetch_done(
    _source: *mut c_void,
    result: *mut c_void,
    data: *mut c_void,
) {
    let ticket = unsafe { Box::from_raw(data.cast::<crate::fetch::Ticket>()) };
    let (resp, error) = unsafe { Response::finish(result) };
    crate::fetch::done(*ticket, resp, error);
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

pub(crate) unsafe extern "C" fn on_xhr_done(
    _source: *mut c_void,
    result: *mut c_void,
    data: *mut c_void,
) {
    let ticket = unsafe { Box::from_raw(data.cast::<crate::xhr::Ticket>()) };
    let (resp, error) = unsafe { Response::finish(result) };
    crate::xhr::done(*ticket, resp, error.is_some());
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
