//! Southstar — the C ABI of the Performance API as declared in src/js_internal.h, and the page state it reads through js.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use std::ffi::CString;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::entry::{Fetch, ResourceInfo, Response};
use crate::timeline::{self, Observer};

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

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *const NsJs {
        self.0 as *const NsJs
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RawObject(JSValue);

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct NavigationTiming {
    pub origin_us: i64,
    pub origin_real_ms: f64,
    pub domain_lookup_start_ms: f64,
    pub domain_lookup_end_ms: f64,
    pub connect_start_ms: f64,
    pub connect_end_ms: f64,
    pub secure_connection_start_ms: f64,
    pub request_start_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
    pub dom_loading_ms: f64,
    pub dom_interactive_ms: f64,
    pub dom_content_loaded_event_start_ms: f64,
    pub dom_content_loaded_event_end_ms: f64,
    pub dom_complete_ms: f64,
    pub load_event_start_ms: f64,
    pub load_event_end_ms: f64,
}

const _: () = assert!(core::mem::size_of::<NavigationTiming>() == 136);

#[repr(C)]
pub struct NsPerfResourceInfo {
    timeline: *const c_void,
    document_url: *const c_char,
    render_blocking: c_int,
    cors_mode: c_int,
    next_hop_protocol: *const c_char,
    timing_allow_origin: *const c_char,
    status: c_long,
    body_size: i64,
}

const _: () = assert!(core::mem::size_of::<NsPerfResourceInfo>() == 56);

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub struct NsResponse {
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

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn ns_js_page_time_origin_us(js: *const NsJs) -> i64;
    fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64;
    fn ns_js_main_realm(js: *const NsJs) -> *mut JSContext;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_page_navigation_timing(js: *const NsJs) -> *const NavigationTiming;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_log_line(js: *const NsJs, line: *const c_char);
    fn ns_own_data_props_toJSON(
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

unsafe fn bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_bytes())
    }
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn real_us() -> i64 {
    unsafe { g_get_real_time() }
}

pub(crate) fn page_time_origin_us(js: Js) -> i64 {
    unsafe { ns_js_page_time_origin_us(js.ptr()) }
}

pub(crate) fn page_time_origin_real_ms(js: Js) -> f64 {
    unsafe { ns_js_page_time_origin_real_ms(js.ptr()) }
}

pub(crate) fn main_realm(js: Js) -> usize {
    unsafe { ns_js_main_realm(js.ptr()) as usize }
}

pub(crate) fn main_context(js: Js) -> Option<*mut JSContext> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    (!ctx.is_null()).then_some(ctx)
}

pub(crate) fn navigation_timing(js: Js) -> Option<NavigationTiming> {
    if js.is_null() {
        return None;
    }
    unsafe { ns_js_page_navigation_timing(js.ptr()).as_ref() }.copied()
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    unsafe { bytes(ns_js_current_url(js.ptr())) }
        .unwrap_or_default()
        .to_vec()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    if js.is_null() {
        return;
    }
    if let Ok(line) = CString::new(line) {
        unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) };
    }
}

pub(crate) fn own_data_props_to_json(scope: &mut Scope<'_>) -> Value {
    quickjs::c_function(scope, "toJSON", 0, ns_own_data_props_toJSON)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn realm_of(scope: &Scope<'_>) -> usize {
    quickjs::raw_context(scope) as usize
}

pub(crate) fn raw_object(value: &Value) -> RawObject {
    RawObject(quickjs::raw(value))
}

pub(crate) fn wrapper_value(scope: &Scope<'_>, observer: &Observer) -> Option<Value> {
    let raw = observer.wrapper.get()?;
    Some(unsafe { quickjs::borrow_value(scope, raw.0) })
}

pub(crate) fn pin_wrapper(js: Js, observer: &Observer) -> Option<Value> {
    let ctx = main_context(js)?;
    unsafe { quickjs::with_context(ctx, |scope| wrapper_value(scope, observer)) }
}

pub(crate) fn enqueue_drain(js: Js) {
    let Some(ctx) = main_context(js) else {
        return;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let _ = scope.enqueue_job(crate::drain);
        });
    }
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: southstar_js_engine::NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_perf_relative_ms(now_us: i64, origin_us: i64) -> f64 {
    crate::entry::relative_ms(now_us, origin_us)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_now_ms(js: *const NsJs) -> f64 {
    crate::now_ms(Js::of(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_time_origin_us(js: *const NsJs, realm: *const c_void) -> i64 {
    timeline::time_origin_us(Js::of(js), realm as usize)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_time_origin_real_ms(js: *const NsJs, realm: *const c_void) -> f64 {
    timeline::time_origin_real_ms(Js::of(js), realm as usize)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_realm_now_ms(ctx: *mut JSContext) -> f64 {
    if ctx.is_null() {
        return crate::realm_now_ms(Js(0), 0);
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::realm_now_ms(js_of(scope), realm_of(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_start_frame_clock(js: *const NsJs, frame: *const c_void) {
    timeline::start_frame_clock(Js::of(js), frame as usize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_adopt_frame_clock(
    js: *const NsJs,
    frame: *const c_void,
    ctx: *mut JSContext,
) {
    timeline::adopt_frame_clock(Js::of(js), frame as usize, ctx as usize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clear_frame_clocks(js: *const NsJs, destroy: c_int) {
    timeline::clear_frame_clocks(Js::of(js), destroy != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_init(js: *const NsJs) {
    timeline::init(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_reset_observers(js: *const NsJs) {
    timeline::reset_observers(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_teardown(js: *const NsJs) {
    timeline::teardown(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_add_resource_timed(
    js: *const NsJs,
    info: *const NsPerfResourceInfo,
    url: *const c_char,
    initiator: *const c_char,
    start_us: i64,
    end_us: i64,
    resp: *const NsResponse,
) {
    let js = Js::of(js);
    let Some(url) = (unsafe { bytes(url) }) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let info = match unsafe { info.as_ref() } {
        Some(info) => unsafe {
            ResourceInfo {
                timeline: info.timeline as usize,
                document_url: bytes(info.document_url),
                render_blocking: info.render_blocking != 0,
                cors_mode: info.cors_mode != 0,
                next_hop_protocol: bytes(info.next_hop_protocol),
                timing_allow_origin: bytes(info.timing_allow_origin),
                status: long(info.status),
                body_size: info.body_size,
            }
        },
        None => ResourceInfo {
            timeline: 0,
            document_url: None,
            render_blocking: false,
            cors_mode: false,
            next_hop_protocol: None,
            timing_allow_origin: None,
            status: 0,
            body_size: 0,
        },
    };
    let response = unsafe { resp.as_ref() }.map(|r| unsafe {
        Response {
            status: long(r.status),
            cors_allow_origin: bytes(r.cors_allow_origin),
            raw_headers: bytes(r.raw_headers),
            body_len: r.body.as_ref().map(|body| i64::from(body.len)),
            next_hop_protocol: bytes(r.next_hop_protocol),
            request_start_us: r.request_start_us,
            domain_lookup_ms: r.domain_lookup_ms,
            connect_ms: r.connect_ms,
            tls_ms: r.tls_ms,
            pretransfer_ms: r.pretransfer_ms,
            response_start_ms: r.response_start_ms,
        }
    });
    let fetch = Fetch {
        url,
        initiator: unsafe { bytes(initiator) },
        start_us,
        end_us,
    };
    crate::add_resource_timed(js, &info, &fetch, response.as_ref());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_has_resource(
    js: *const NsJs,
    timeline: *const c_void,
    url: *const c_char,
    initiator: *const c_char,
) -> c_int {
    let js = Js::of(js);
    let Some(url) = (unsafe { bytes(url) }) else {
        return 0;
    };
    if js.is_null() {
        return 0;
    }
    c_int::from(crate::has_resource(js, timeline as usize, url, unsafe {
        bytes(initiator)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_move_timeline(
    js: *const NsJs,
    from: *const c_void,
    to: *const c_void,
) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::move_timeline(js, from as usize, to as usize);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_new_performance_object(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::new_performance_object(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_set_performance_objects(
    ctx: *mut JSContext,
    perf: JSValue,
    timing: JSValue,
    navigation: JSValue,
    event_counts: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let objects = [
                quickjs::take_value(scope, timing),
                quickjs::take_value(scope, navigation),
                quickjs::take_value(scope, event_counts),
            ];
            let perf = quickjs::borrow_value(scope, perf);
            crate::set_performance_objects(scope, &perf, objects);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_performance_time_origin_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::time_origin_get) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_performance_object_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue {
    let which = match magic {
        0 => 0,
        1 => 1,
        _ => 2,
    };
    unsafe {
        quickjs::call_native(ctx, this_val, argc, argv, |scope, this, _| {
            crate::object_get(scope, this, which)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_supported_entry_types(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::supported_entry_types(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_install_entry_list(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::install_entry_list(scope, &global);
        });
    }
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

export_native! {
    ns_perf_observer_ctor => crate::observer_ctor,
    ns_perf_observer_observe => crate::observe,
    ns_perf_observer_disconnect => crate::disconnect,
    ns_perf_observer_takeRecords => crate::take_records,
    ns_window_performance_now => crate::performance_now,
    ns_window_performance_mark => crate::mark,
    ns_window_performance_measure => crate::measure,
    ns_window_performance_clearMarks => crate::clear_marks,
    ns_window_performance_clearMeasures => crate::clear_measures,
    ns_window_performance_clearResourceTimings => crate::clear_resource_timings,
    ns_window_performance_getEntries => crate::get_entries,
    ns_window_performance_getEntriesByName => crate::get_entries_by_name,
    ns_window_performance_getEntriesByType => crate::get_entries_by_type,
    ns_window_performance_memory_get => crate::memory_get,
}
