//! Southstar — the C ABI and Rust-ABI entry points of the Performance API, and the page state they read.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use std::ffi::CString;

use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::entry::{Fetch, ResourceInfo, Response};
use crate::navigation;
use crate::timeline::{self, Observer};

pub(crate) use southstar_js_runtime::api::{NavigationTiming, NsJs};

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
pub(crate) struct RawObject(usize);

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
    api::time_origin_us(js.ptr())
}

pub(crate) fn page_time_origin_real_ms(js: Js) -> f64 {
    api::time_origin_real_ms(js.ptr())
}

pub(crate) fn main_realm(js: Js) -> usize {
    api::main_realm(js.ptr()).as_ptr() as usize
}

pub(crate) fn main_context(js: Js) -> Option<ContextHandle> {
    api::main_context(js.ptr()).non_null()
}

pub(crate) fn navigation_timing(js: Js) -> Option<NavigationTiming> {
    api::navigation_timing(js.ptr())
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    api::current_url(js.ptr()).into_bytes()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    if let Ok(line) = CString::new(line) {
        api::log_line(js.ptr(), &line);
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque() as usize)
}

pub(crate) fn realm_of(scope: &Scope<'_>) -> usize {
    scope.handle().as_ptr() as usize
}

pub(crate) fn raw_object(value: &Value) -> RawObject {
    RawObject(value.object_identity() as usize)
}

pub(crate) fn wrapper_value(scope: &mut Scope<'_>, observer: &Observer) -> Option<Value> {
    let raw = observer.wrapper.get()?;
    Some(scope.object_from_identity(raw.0 as *mut c_void))
}

pub(crate) fn pin_wrapper(js: Js, observer: &Observer) -> Option<Value> {
    main_context(js)?
        .with(|scope| wrapper_value(scope, observer))
        .flatten()
}

pub(crate) fn enqueue_drain(js: Js) {
    let Some(ctx) = main_context(js) else {
        return;
    };
    ctx.with(|scope| {
        let _ = scope.enqueue_job(crate::drain);
    });
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
pub fn nsr_perf_realm_now_ms(scope: &mut Scope<'_>) -> f64 {
    crate::realm_now_ms(js_of(scope), realm_of(scope))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_perf_realm_now_ms(ctx: ContextHandle) -> f64 {
    ctx.with(|scope| crate::realm_now_ms(js_of(scope), realm_of(scope)))
        .unwrap_or_else(|| crate::realm_now_ms(Js(0), 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_start_frame_clock(js: *const NsJs, frame: *const c_void) {
    timeline::start_frame_clock(Js::of(js), frame as usize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_adopt_frame_clock(
    js: *const NsJs,
    frame: *const c_void,
    ctx: ContextHandle,
) {
    timeline::adopt_frame_clock(Js::of(js), frame as usize, ctx.as_ptr() as usize);
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
pub unsafe extern "C" fn ns_perf_coarsen_navigation_timing(timing: *mut NavigationTiming) {
    if let Some(timing) = unsafe { timing.as_mut() } {
        navigation::coarsen(timing);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mark_navigation(js: *mut NsJs, legacy_key: *const c_char) {
    let js = Js::of(js);
    let Some(key) = (unsafe { bytes(legacy_key) }) else {
        return;
    };
    let Ok(key) = core::str::from_utf8(key) else {
        return;
    };
    let Some(mut timing) = api::navigation_timing(js.ptr()) else {
        return;
    };
    let Some(field) = navigation::milestone(&mut timing, key) else {
        return;
    };
    *field = crate::now_ms(js);
    let milestone_ms = *field;
    api::set_navigation_timing(js.ptr(), timing);
    let absolute_ms = (page_time_origin_real_ms(js) + milestone_ms).floor() as i64;
    if let Some(ctx) = main_context(js) {
        ctx.with(|scope| navigation::record_legacy_milestone(scope, key, absolute_ms));
    }
}

#[unsafe(no_mangle)]
pub fn nsr_perf_observer_ctor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::observer_ctor(scope, new_target, args)
}

#[unsafe(no_mangle)]
pub fn nsr_perf_observer_observe(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::observe(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_perf_observer_disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::disconnect(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_perf_observer_take_records(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::take_records(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_perf_install_entry_list(scope: &mut Scope<'_>, global: &Value) {
    crate::install_entry_list(scope, global);
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_make_performance_object(
    scope: &mut Scope<'_>,
    js: *const NsJs,
    include_memory: bool,
) -> Value {
    crate::object::make_performance_object(scope, Js::of(js), include_memory)
}

#[unsafe(no_mangle)]
pub fn nsr_perf_supported_entry_types(scope: &mut Scope<'_>) -> Value {
    crate::supported_entry_types(scope)
}

#[unsafe(no_mangle)]
pub fn nsr_performance_extend_event_target(
    scope: &mut Scope<'_>,
    global: &Value,
    performance: &Value,
) {
    crate::object::extend_event_target(scope, global, performance);
}
