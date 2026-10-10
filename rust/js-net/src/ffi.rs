//! Southstar — the C ABI of the networking bindings as declared in src/js_internal.h, and the js.c, net and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_glib::{self as glib, GBoolean, GError};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::JsResult;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Js(usize);

impl Js {
    pub fn of_ptr(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn of(scope: &Scope<'_>) -> Js {
        Js(quickjs::context_opaque(scope) as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(*mut JSContext);

impl Realm {
    pub fn of(scope: &Scope<'_>) -> Realm {
        Realm(quickjs::raw_context(scope))
    }

    pub fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        unsafe { quickjs::with_context(self.0, f) }
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

#[repr(C)]
struct NsPerfResourceInfo {
    timeline: *const c_void,
    document_url: *const c_char,
    render_blocking: GBoolean,
    cors_mode: GBoolean,
    next_hop_protocol: *const c_char,
    timing_allow_origin: *const c_char,
    status: c_long,
    body_size: i64,
}

const _: () = assert!(core::mem::size_of::<NsPerfResourceInfo>() == 56);

pub(crate) const HO_ABORT_CONTROLLER: c_int = 1;
pub(crate) const HO_ABORT_SIGNAL: c_int = 2;
pub(crate) const HO_XHR: c_int = 10;
pub(crate) const HO_XHR_UPLOAD: c_int = 11;

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
    fn ns_url_host_from(url: *const c_char) -> *mut c_char;
    fn ns_net_hsts_should_upgrade(host: *const c_char) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_net_enter_handler_realm(
        ctx: *mut JSContext,
        obj: JSValue,
        kind: *const c_char,
    ) -> *mut c_void;
    fn ns_js_net_leave_handler_realm(ctx: *mut JSContext, scope: *mut c_void);
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_define_cancel_bubble(ctx: *mut JSContext, ev: JSValue);
    fn ns_event_adopt_interface(ctx: *mut JSContext, ev: JSValue, iface: *const c_char);
    fn ns_event_prevent_default(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_propagation(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_immediate(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_composed_path(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_bind_fn(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        f: quickjs::JSCFunction,
        argc: c_int,
    );
    fn ns_ho_install_attrs(ctx: *mut JSContext, global: JSValue);
    fn ns_net_install_form_data(ctx: *mut JSContext, global: JSValue);
    fn ns_net_install_ports(ctx: *mut JSContext, global: JSValue);
    fn ns_net_install_file_reader(ctx: *mut JSContext, global: JSValue);
}

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_timeout_add(interval: c_uint, function: GSourceFunc, data: *mut c_void) -> c_uint;
    fn g_cancellable_new() -> *mut c_void;
    fn g_cancellable_cancel(cancellable: *mut c_void);
    fn g_object_ref(object: *mut c_void) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
    fn g_bytes_get_data(bytes: *mut c_void, size: *mut usize) -> *const u8;
    fn g_byte_array_sized_new(reserved: c_uint) -> *mut GByteArray;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;

    fn ns_url_resolve(base: *const c_char, url: *const c_char) -> *mut c_char;
    fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> GBoolean;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean;
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

    fn ns_js_net_page_url(js: *const NsJs) -> *const c_char;
    fn ns_js_net_csp_allows_connect(
        js: *const NsJs,
        url: *const c_char,
        page: *const c_char,
    ) -> GBoolean;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_is_worker(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_realm_url(js: *mut NsJs, realm: *mut JSContext) -> *const c_char;
    fn ns_js_realm_document_url(js: *mut NsJs, realm: *mut JSContext) -> *const c_char;
    fn ns_perf_now_ms(js: *const NsJs) -> f64;
    fn ns_js_page_time_origin_us(js: *const NsJs) -> i64;
    fn ns_perf_add_resource_timed(
        js: *mut NsJs,
        info: *const NsPerfResourceInfo,
        url: *const c_char,
        initiator: *const c_char,
        start_us: i64,
        end_us: i64,
        resp: *const NsResponse,
    );
    fn ns_js_budget_enter(js: *mut NsJs) -> i64;
    fn ns_js_budget_leave(js: *mut NsJs, saved: i64);
    fn ns_drain_mutations(js: *mut NsJs);
    fn ns_js_attach_idle(js: *mut NsJs, func: GSourceFunc, data: *mut c_void) -> c_uint;
    fn ns_js_attach_timeout(
        js: *mut NsJs,
        ms: c_uint,
        func: GSourceFunc,
        data: *mut c_void,
    ) -> c_uint;
    fn ns_js_blob_url_lookup(
        js: *mut NsJs,
        url: *const c_char,
        out_type: *mut *mut c_char,
    ) -> *mut c_void;
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

    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_make_abort_error(ctx: *mut JSContext) -> JSValue;
    fn ns_xhr_fire_progress_event(
        ctx: *mut JSContext,
        target: JSValue,
        kind: *const c_char,
        loaded: f64,
        total: f64,
        length_computable: GBoolean,
    );
    fn ns_js_net_host_state(ctx: *mut JSContext, v: JSValue, kind: c_int) -> JSValue;
    fn ns_js_url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean;
    fn ns_js_net_pump_iteration(js: *mut NsJs) -> GBoolean;
    fn ns_js_credit_pumped_time(js: *mut NsJs, pump_start_us: i64);
    fn ns_js_body_bytes(ctx: *mut JSContext, value: JSValue, out_len: *mut usize) -> *mut c_char;
    fn ns_js_value_is_form_data(ctx: *mut JSContext, v: JSValue) -> GBoolean;
    fn ns_js_form_data_serialize(
        ctx: *mut JSContext,
        fd: JSValue,
        out_len: *mut usize,
        out_content_type: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_js_value_is_url_search_params(ctx: *mut JSContext, v: JSValue) -> GBoolean;
    fn ns_js_usp_serialize(
        ctx: *mut JSContext,
        usp: JSValue,
        out_len: *mut usize,
        out_content_type: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_blob_bytes_as_string(
        ctx: *mut JSContext,
        blob: JSValue,
        out_len: *mut usize,
    ) -> *mut c_char;
    fn ns_target_fire_event(ctx: *mut JSContext, obj: JSValue, kind: *const c_char);
    fn ns_ho_construct(ctx: *mut JSContext, new_target: JSValue, kind: c_int) -> JSValue;
    fn ns_ho_new_default(ctx: *mut JSContext, kind: c_int) -> JSValue;
    fn ns_js_net_host_is(v: JSValue, kind: c_int) -> GBoolean;
    fn ns_bind_ctor(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        f: quickjs::JSCFunction,
        argc: c_int,
    );
    fn ns_illegal_constructor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
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

unsafe fn take_gbytes(raw: *mut c_char, len: usize) -> Option<Vec<u8>> {
    if raw.is_null() {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(raw.cast::<u8>(), len) }.to_vec();
    unsafe { glib::g_free(raw.cast()) };
    Some(bytes)
}

fn borrowed(raw: *const c_char) -> Option<Vec<u8>> {
    (!raw.is_null()).then(|| unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec())
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn url_resolve(base: Option<&[u8]>, url: &[u8]) -> Option<Vec<u8>> {
    let base = base.map(cstring);
    let url = cstring(url);
    unsafe { take_gstr(ns_url_resolve(opt_ptr(base.as_ref()), url.as_ptr())) }
}

pub(crate) fn url_is_http_or_https(url: &[u8]) -> bool {
    let url = cstring(url);
    unsafe { ns_url_is_http_or_https(url.as_ptr()) != 0 }
}

pub(crate) fn url_same_origin(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    let a = a.map(cstring);
    let b = b.map(cstring);
    unsafe { ns_url_same_origin(opt_ptr(a.as_ref()), opt_ptr(b.as_ref())) != 0 }
}

pub(crate) fn url_origin_from(url: Option<&[u8]>) -> Option<Vec<u8>> {
    let url = url.map(cstring);
    unsafe { take_gstr(ns_url_origin_from(opt_ptr(url.as_ref()))) }
}

pub(crate) fn throw_dom(scope: &mut Scope<'_>, name: &CStr, code: c_int, message: &str) -> Value {
    let ctx = quickjs::raw_context(scope);
    let message = cstring(message.as_bytes());
    unsafe { ns_throw_dom_exception(ctx, name.as_ptr(), code, message.as_ptr()) };
    quickjs::take_exception(scope)
}

pub(crate) fn url_host_from(url: &[u8]) -> Option<Vec<u8>> {
    let url = cstring(url);
    unsafe { take_gstr(ns_url_host_from(url.as_ptr())) }
}

pub(crate) fn hsts_should_upgrade(host: &[u8]) -> bool {
    let host = cstring(host);
    unsafe { ns_net_hsts_should_upgrade(host.as_ptr()) != 0 }
}

pub(crate) struct HandlerRealm(*mut c_void);

pub(crate) fn enter_handler_realm(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
) -> HandlerRealm {
    let ctx = quickjs::raw_context(scope);
    let kind = cstring(kind.as_bytes());
    HandlerRealm(unsafe { ns_js_net_enter_handler_realm(ctx, quickjs::raw(target), kind.as_ptr()) })
}

pub(crate) fn leave_handler_realm(scope: &mut Scope<'_>, entered: HandlerRealm) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_js_net_leave_handler_realm(ctx, entered.0) };
}

fn bind_c(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &CStr,
    f: quickjs::JSCFunction,
    argc: c_int,
) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_bind_fn(ctx, quickjs::raw(object), name.as_ptr(), f, argc) };
}

pub(crate) fn socket_event(scope: &mut Scope<'_>, kind: &CStr) -> Value {
    let ctx = quickjs::raw_context(scope);
    let event = unsafe { quickjs::take_value(scope, ns_event_new(ctx)) };
    crate::set_str(scope, &event, "type", kind.to_bytes());
    crate::set(scope, &event, "bubbles", Value::boolean(false));
    crate::set(scope, &event, "cancelable", Value::boolean(false));
    crate::set(scope, &event, "defaultPrevented", Value::boolean(false));
    bind_c(
        scope,
        &event,
        c"preventDefault",
        ns_event_prevent_default,
        0,
    );
    bind_c(
        scope,
        &event,
        c"stopPropagation",
        ns_event_stop_propagation,
        0,
    );
    unsafe { ns_event_define_cancel_bubble(ctx, quickjs::raw(&event)) };
    crate::set(scope, &event, "_is_trusted", Value::boolean(true));
    bind_c(
        scope,
        &event,
        c"stopImmediatePropagation",
        ns_event_stop_immediate,
        0,
    );
    bind_c(scope, &event, c"composedPath", ns_event_composed_path, 0);
    event
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &CStr) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_event_adopt_interface(ctx, quickjs::raw(event), iface.as_ptr()) };
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_bind_event_target_listeners(ctx, quickjs::raw(object)) };
    bind_c(scope, object, c"dispatchEvent", ns_target_dispatchEvent, 1);
}

pub(crate) fn bind_socket_ctors(scope: &mut Scope<'_>, global: &Value) {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        ns_bind_ctor(
            ctx,
            quickjs::raw(global),
            c"WebSocket".as_ptr(),
            ns_window_websocket_ctor,
            2,
        );
        ns_bind_ctor(
            ctx,
            quickjs::raw(global),
            c"EventSource".as_ptr(),
            ns_window_eventsource_ctor,
            2,
        );
    }
}

pub(crate) fn install_host_interfaces(scope: &mut Scope<'_>, global: &Value) {
    let ctx = quickjs::raw_context(scope);
    let raw = quickjs::raw(global);
    unsafe { ns_ho_install_attrs(ctx, raw) };
}

pub(crate) fn install_neighbours(scope: &mut Scope<'_>, global: &Value) {
    let ctx = quickjs::raw_context(scope);
    let raw = quickjs::raw(global);
    unsafe {
        ns_net_install_form_data(ctx, raw);
        ns_net_install_ports(ctx, raw);
        ns_net_install_file_reader(ctx, raw);
    }
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
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_make_abort_error(ctx) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &CStr) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_target_fire_event(ctx, quickjs::raw(target), kind.as_ptr()) };
}

pub(crate) fn fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &CStr,
    loaded: f64,
    total: f64,
    computable: bool,
) {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        ns_xhr_fire_progress_event(
            ctx,
            quickjs::raw(target),
            kind.as_ptr(),
            loaded,
            total,
            glib::boolean(computable),
        )
    };
}

pub(crate) fn host_state(scope: &mut Scope<'_>, value: &Value, kind: c_int) -> Option<Value> {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_js_net_host_state(ctx, quickjs::raw(value), kind) };
    let state = unsafe { quickjs::take_value(scope, raw) };
    state.is_object().then_some(state)
}

pub(crate) fn body_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    unsafe { take_gbytes(ns_js_body_bytes(ctx, quickjs::raw(value), &mut len), len) }
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Option<Vec<u8>> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    unsafe {
        take_gbytes(
            ns_blob_bytes_as_string(ctx, quickjs::raw(blob), &mut len),
            len,
        )
    }
}

pub(crate) struct Serialized {
    pub body: Vec<u8>,
    pub content_type: Option<Vec<u8>>,
}

pub(crate) fn serialize_form_body(scope: &mut Scope<'_>, value: &Value) -> Option<Serialized> {
    let ctx = quickjs::raw_context(scope);
    let raw = quickjs::raw(value);
    let mut len = 0usize;
    let mut content_type = ptr::null_mut();
    let body = unsafe {
        if ns_js_value_is_form_data(ctx, raw) != 0 {
            ns_js_form_data_serialize(ctx, raw, &mut len, &mut content_type)
        } else if ns_js_value_is_url_search_params(ctx, raw) != 0 {
            ns_js_usp_serialize(ctx, raw, &mut len, &mut content_type)
        } else {
            return None;
        }
    };
    let content_type = unsafe { take_gstr(content_type) };
    let body = unsafe { take_gbytes(body, len) }.unwrap_or_default();
    Some(Serialized { body, content_type })
}

pub(crate) fn host_construct(scope: &mut Scope<'_>, new_target: &Value, kind: c_int) -> JsResult {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_ho_construct(ctx, quickjs::raw(new_target), kind) };
    quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })
}

pub(crate) fn host_new(scope: &mut Scope<'_>, kind: c_int) -> JsResult {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_ho_new_default(ctx, kind) };
    quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })
}

pub(crate) fn host_is(value: &Value, kind: c_int) -> bool {
    unsafe { ns_js_net_host_is(quickjs::raw(value), kind) != 0 }
}

pub(crate) fn bind_illegal_ctor(scope: &mut Scope<'_>, global: &Value, name: &CStr) {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        ns_bind_ctor(
            ctx,
            quickjs::raw(global),
            name.as_ptr(),
            ns_illegal_constructor,
            0,
        )
    };
}

impl Js {
    pub fn log_line(self, line: &[u8]) {
        if self.is_null() {
            return;
        }
        let line = cstring(line);
        unsafe { ns_js_log_line(self.ptr(), line.as_ptr()) };
    }

    pub fn page_url(self) -> Option<Vec<u8>> {
        if self.is_null() {
            return None;
        }
        borrowed(unsafe { ns_js_net_page_url(self.ptr()) })
    }

    pub fn url_parses(self, url: &[u8]) -> bool {
        let url = cstring(url);
        !self.is_null() && unsafe { ns_js_url_parses(self.ptr(), url.as_ptr()) } != 0
    }

    pub fn pump_iteration(self) -> bool {
        !self.is_null() && unsafe { ns_js_net_pump_iteration(self.ptr()) } != 0
    }

    pub fn credit_pumped_time(self, start_us: i64) {
        if !self.is_null() {
            unsafe { ns_js_credit_pumped_time(self.ptr(), start_us) };
        }
    }

    pub fn csp_allows_connect(self, url: &[u8], page: Option<&[u8]>) -> bool {
        if self.is_null() {
            return true;
        }
        let url = cstring(url);
        let page = page.map(cstring);
        unsafe {
            ns_js_net_csp_allows_connect(self.ptr(), url.as_ptr(), opt_ptr(page.as_ref())) != 0
        }
    }

    pub fn main_context(self) -> Realm {
        if self.is_null() {
            return Realm(ptr::null_mut());
        }
        Realm(unsafe { ns_js_main_context(self.ptr()) })
    }

    pub fn is_worker(self) -> bool {
        !self.is_null() && unsafe { ns_js_is_worker(self.ptr()) } != 0
    }

    pub fn in_pump(self) -> bool {
        !self.is_null() && unsafe { ns_js_in_pump(self.ptr()) } != 0
    }

    pub fn realm_url(self, realm: Realm) -> Option<Vec<u8>> {
        if self.is_null() {
            return None;
        }
        borrowed(unsafe { ns_js_realm_url(self.ptr(), realm.0) })
    }

    pub fn perf_now_ms(self) -> f64 {
        if self.is_null() {
            return 0.0;
        }
        unsafe { ns_perf_now_ms(self.ptr()) }
    }

    pub fn time_origin_us(self) -> i64 {
        unsafe { ns_js_page_time_origin_us(self.ptr()) }
    }

    pub fn add_resource_timing(self, timing: &Timing<'_>, resp: &Response) {
        let info = NsPerfResourceInfo {
            timeline: timing.timeline.0.cast(),
            document_url: unsafe { ns_js_realm_document_url(self.ptr(), timing.timeline.0) },
            render_blocking: 0,
            cors_mode: glib::boolean(timing.cors_mode),
            next_hop_protocol: ptr::null(),
            timing_allow_origin: ptr::null(),
            status: 0,
            body_size: 0,
        };
        let url = cstring(timing.url);
        unsafe {
            ns_perf_add_resource_timed(
                self.ptr(),
                &info,
                url.as_ptr(),
                timing.initiator.as_ptr(),
                timing.start_us,
                timing.end_us,
                resp.0,
            )
        };
    }

    pub fn with_budget<R>(self, f: impl FnOnce() -> R) -> R {
        if self.is_null() {
            return f();
        }
        let saved = unsafe { ns_js_budget_enter(self.ptr()) };
        let result = f();
        unsafe { ns_js_budget_leave(self.ptr(), saved) };
        result
    }

    pub fn drain_mutations(self) {
        if !self.is_null() {
            unsafe { ns_drain_mutations(self.ptr()) };
        }
    }

    pub fn blob_url(self, url: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let url = cstring(url);
        let mut kind = ptr::null_mut();
        let bytes = unsafe { ns_js_blob_url_lookup(self.ptr(), url.as_ptr(), &mut kind) };
        if bytes.is_null() {
            return None;
        }
        let mut len = 0usize;
        let data = unsafe { g_bytes_get_data(bytes, &mut len) };
        let data = if data.is_null() {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(data, len) }.to_vec()
        };
        Some((data, borrowed(kind)))
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
        unsafe { ns_js_attach_idle(self.ptr(), func, data) };
    }

    pub fn attach_timeout(self, ms: u32, func: GSourceFunc, data: *mut c_void) {
        unsafe { ns_js_attach_timeout(self.ptr(), ms, func, data) };
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

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn with_global(ctx: *mut JSContext, global: JSValue, f: fn(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            f(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fetch(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::fetch::fetch) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_navigator_sendBeacon(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::beacon::send_beacon) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_response_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::body::response_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_request_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::body::request_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_fetch_install_interfaces(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::body::install) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_xhr_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::xhr::construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xhr_install_interface(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::xhr::install) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_websocket_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::socket::websocket_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_eventsource_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::socket::event_source_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_install_sockets(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::socket::install) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_interfaces(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::install_interfaces) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_abort_controller_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::abort::controller_ctor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_abort_signal_interface(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::abort::install) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_init(js: *mut NsJs) {
    crate::page_init(Js::of_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_reset(js: *mut NsJs) {
    crate::page_reset(Js::of_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_teardown(js: *mut NsJs) {
    crate::page_teardown(Js::of_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_pending_fetches(js: *const NsJs) -> c_uint {
    crate::pending_fetches(Js::of_ptr(js)) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_pending_xhrs(js: *const NsJs) -> c_uint {
    crate::pending_xhrs(Js::of_ptr(js)) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_pending_sockets(js: *const NsJs) -> c_uint {
    crate::pending_sockets(Js::of_ptr(js)) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_sw_fetch_result(
    js: *mut NsJs,
    id: c_uint,
    outcome: c_int,
    status: c_long,
    content_type: *const c_char,
    raw_headers: *const c_char,
    body: *const u8,
    body_len: usize,
    error: *const c_char,
) {
    let body = if body.is_null() || body_len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(body, body_len) }
    };
    crate::fetch::service_worker_result(
        Js::of_ptr(js),
        id,
        crate::fetch::SwOutcome {
            outcome,
            status: long(status),
            content_type: borrowed(content_type),
            raw_headers: borrowed(raw_headers),
            body,
            error: borrowed(error),
        },
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cors_allows(
    doc_url: *const c_char,
    resp_url: *const c_char,
    cors_header: *const c_char,
) -> GBoolean {
    glib::boolean(crate::headers::cors_allows(
        borrowed(doc_url).as_deref(),
        borrowed(resp_url).as_deref(),
        borrowed(cors_header).as_deref(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_header_value_is_safe(value: *const c_char) -> GBoolean {
    glib::boolean(borrowed(value).is_some_and(|v| crate::headers::value_is_safe(&v)))
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
