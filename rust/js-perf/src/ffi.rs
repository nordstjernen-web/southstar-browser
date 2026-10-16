//! Southstar — the Performance API's entry points for other crates and the page record's hooks, and the page state they read.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::entry::{self, Fetch, Response};
use crate::navigation;
use crate::timeline::{self, Observer};

use southstar_js_runtime::api::ResourceInfo;
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
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct NsResponse {
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

pub fn page_now_ms(js: *const NsJs) -> f64 {
    crate::now_ms(Js::of(js))
}

pub fn time_origin_us(js: *const NsJs, realm: ContextHandle) -> i64 {
    timeline::time_origin_us(Js::of(js), realm.as_ptr() as usize)
}

pub fn scope_now_ms(scope: &mut Scope<'_>) -> f64 {
    crate::realm_now_ms(js_of(scope), realm_of(scope))
}

pub fn context_now_ms(ctx: ContextHandle) -> f64 {
    ctx.with(|scope| crate::realm_now_ms(js_of(scope), realm_of(scope)))
        .unwrap_or_else(|| crate::realm_now_ms(Js(0), 0))
}

pub fn start_frame_clock(js: *const NsJs, frame: Node<'_>) {
    timeline::start_frame_clock(Js::of(js), frame.as_ptr() as usize);
}

pub fn adopt_frame_clock(js: *const NsJs, frame: Option<Node<'_>>, ctx: ContextHandle) {
    let frame = Node::ptr_or_null(frame) as usize;
    timeline::adopt_frame_clock(Js::of(js), frame, ctx.as_ptr() as usize);
}

pub fn clear_frame_clocks(js: *const NsJs, destroy: bool) {
    timeline::clear_frame_clocks(Js::of(js), destroy);
}

pub fn reset_observers(js: *const NsJs) {
    timeline::reset_observers(Js::of(js));
}

fn init(js: *const NsJs) {
    timeline::init(Js::of(js));
}

fn teardown(js: *const NsJs) {
    timeline::teardown(Js::of(js));
}

/// # Safety
///
/// The strings `info` points to are null or NUL-terminated and live for the call, and `resp` is
/// null or a live response.
pub unsafe fn add_resource_timed(
    js: *const NsJs,
    info: Option<&ResourceInfo>,
    url: &CStr,
    initiator: Option<&CStr>,
    (start_us, end_us): (i64, i64),
    resp: *const api::Response,
) {
    let js = Js::of(js);
    if js.is_null() {
        return;
    }
    let info = match info {
        Some(info) => unsafe {
            entry::ResourceInfo {
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
        None => entry::ResourceInfo {
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
    let response = unsafe { resp.cast::<NsResponse>().as_ref() }.map(|r| unsafe {
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
        url: url.to_bytes(),
        initiator: initiator.map(CStr::to_bytes),
        start_us,
        end_us,
    };
    crate::add_resource_entry(js, &info, &fetch, response.as_ref());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_perf_add_resource_timed(
    js: *const NsJs,
    info: *const ResourceInfo,
    url: *const c_char,
    initiator: *const c_char,
    start_us: i64,
    end_us: i64,
    resp: *const api::Response,
) {
    let Some(url) = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) }) else {
        return;
    };
    let initiator = (!initiator.is_null()).then(|| unsafe { CStr::from_ptr(initiator) });
    let info = unsafe { info.as_ref() };
    unsafe { add_resource_timed(js, info, url, initiator, (start_us, end_us), resp) };
}

pub fn has_resource_timing(
    js: *const NsJs,
    timeline: *const c_void,
    url: &CStr,
    initiator: Option<&CStr>,
) -> bool {
    let js = Js::of(js);
    !js.is_null()
        && crate::has_resource(
            js,
            timeline as usize,
            url.to_bytes(),
            initiator.map(CStr::to_bytes),
        )
}

pub fn move_frame_timeline(js: *const NsJs, frame: Node<'_>, realm: ContextHandle) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::move_timeline(js, frame.as_ptr() as usize, realm.as_ptr() as usize);
    }
}

fn coarsen_navigation_timing(timing: &mut NavigationTiming) {
    navigation::coarsen(timing);
}

pub fn mark_navigation(js: *const NsJs, legacy_key: &str) {
    let js = Js::of(js);
    let Some(mut timing) = api::navigation_timing(js.ptr()) else {
        return;
    };
    let Some(field) = navigation::milestone(&mut timing, legacy_key) else {
        return;
    };
    *field = crate::now_ms(js);
    let milestone_ms = *field;
    api::set_navigation_timing(js.ptr(), timing);
    let absolute_ms = (page_time_origin_real_ms(js) + milestone_ms).floor() as i64;
    if let Some(ctx) = main_context(js) {
        ctx.with(|scope| navigation::record_legacy_milestone(scope, legacy_key, absolute_ms));
    }
}

fn make_performance_object(scope: &mut Scope<'_>, js: *const NsJs, include_memory: bool) -> Value {
    crate::object::make_performance_object(scope, Js::of(js), include_memory)
}

pub fn install_runtime_hooks() {
    api::set_perf_hooks(api::PerfHooks {
        init,
        teardown,
        clear_frame_clocks,
        coarsen_navigation_timing,
        make_performance_object,
        supported_entry_types: crate::supported_entry_types,
        extend_event_target: crate::object::extend_event_target,
        install_entry_list: crate::install_entry_list,
        observer_ctor: crate::observer_ctor,
        observer_observe: crate::observe,
        observer_disconnect: crate::disconnect,
        observer_take_records: crate::take_records,
        add_resource_timed,
    });
}
