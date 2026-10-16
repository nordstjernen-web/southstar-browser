//! Southstar — the calls loading a frame's document makes: fetching, decoding and parsing it, the engine state its load runs under and the page events it fires.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_dom::{Node, NsNode, Tree};
use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_js_engine::Value;
use southstar_js_runtime::api::{self, ResourceInfo, Upload};

use super::ctx::Ctx;
use super::imports::{nsr_realmdoc_deny_cookie, nsr_window_link_outward};
use super::{Js, NsJs, c_string, opt_ptr, owned_text};

#[repr(C)]
pub(crate) struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    content_disposition: *mut c_char,
    csp_header: *mut c_char,
    xframe_options: *mut c_char,
    x_content_type_options: *mut c_char,
    _cors_allow_origin: *mut c_char,
    _refresh: *mut c_char,
    _content_language: *mut c_char,
    _raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
}

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct NsCsp {
    _private: [u8; 0],
}

#[repr(C)]
struct ScriptTask {
    node: *mut NsNode,
    schedule: c_int,
}

const CSP_FRAME: c_int = 7;
const CSP_FRAME_ANCESTORS: c_int = 10;
const CSP_OBJECT: c_int = 11;
const SCRIPT_DEFERRED: c_int = 1;
const SCRIPT_ASYNC: c_int = 2;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn ns_csp_allows(
        csp: *const NsCsp,
        kind: c_int,
        url: *const c_char,
        page: *const c_char,
    ) -> GBoolean;
    fn ns_csp_parse(header: *const c_char) -> *mut NsCsp;
    fn ns_csp_free(csp: *mut NsCsp);
    fn ns_js_start_frame_clock(js: *mut NsJs, frame: *const c_void);
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_js_element_perf_info(js: *mut NsJs, el: *const NsNode, info: *mut ResourceInfo);
    fn ns_perf_add_resource_timed(
        js: *mut NsJs,
        info: *const ResourceInfo,
        url: *const c_char,
        initiator: *const c_char,
        start_us: i64,
        end_us: i64,
        resp: *const NsResponse,
    );
    fn ns_net_header_is_nosniff(value: *const c_char) -> GBoolean;
    fn ns_content_type_is_javascript(content_type: *const c_char) -> GBoolean;
    fn ns_xml_well_formed(input: *const c_char, len: isize, root_ns: *mut *mut c_char) -> GBoolean;
    fn ns_net_referer_for(
        url: *const c_char,
        referrer_url: *const c_char,
        policy: c_int,
    ) -> *mut c_char;
    fn ns_js_dispatch_resource_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
    ) -> GBoolean;
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_fire_page_transition(js: *mut NsJs, kind: *const c_char, persisted: GBoolean);
    fn ns_js_mark_scripts_already_started(node: *mut NsNode);
    fn ns_js_collect_script_tasks(node: *mut NsNode, tasks: *mut GArray);
    fn ns_js_run_parser_blocking_scripts(js: *mut NsJs, tasks: *mut GArray, origin: *const c_char);
    fn ns_js_run_script_schedule(
        js: *mut NsJs,
        tasks: *mut GArray,
        schedule: c_int,
        origin: *const c_char,
    );
    fn ns_js_schedule_pending_script_drain(js: *mut NsJs);
    fn ns_script_type_supported(node: *const NsNode) -> GBoolean;
    fn ns_script_skipped_by_nomodule(node: *const NsNode) -> GBoolean;
    fn ns_script_type_is_module(node: *const NsNode) -> GBoolean;
    fn ns_js_purge_frame_rafs(js: *mut NsJs, frame: *const NsNode);
    fn ns_services_purge_frame_timers(js: *mut NsJs, frame: *const NsNode);
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
}

unsafe fn c_str<'a>(text: *const c_char) -> Option<&'a CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

pub(crate) struct Response(NonNull<NsResponse>);

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { ns_response_free(self.0.as_ptr()) };
    }
}

impl Response {
    fn take(resp: *mut NsResponse) -> Option<Response> {
        NonNull::new(resp).map(Response)
    }

    fn get(&self) -> &NsResponse {
        unsafe { self.0.as_ref() }
    }

    fn as_ptr(&self) -> *const NsResponse {
        self.0.as_ptr()
    }

    pub fn status(&self) -> c_long {
        self.get().status
    }

    pub fn final_url(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().final_url) }
    }

    pub fn content_type(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().content_type) }
    }

    pub fn content_disposition(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().content_disposition) }
    }

    pub fn csp_header(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().csp_header) }
    }

    pub fn xframe_options(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().xframe_options) }
    }

    pub fn x_content_type_options(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().x_content_type_options) }
    }

    pub fn error(&self) -> Option<&CStr> {
        unsafe { c_str(self.get().error) }
    }

    pub fn body(&self) -> Option<&[u8]> {
        let body = unsafe { self.get().body.as_ref() }?;
        Some(unsafe { glib::slice(body.data, body.len as usize) })
    }
}

pub(crate) fn monotonic_time() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn node_in_page(js: Js, node: Node<'_>) -> bool {
    api::node_in_page(js.ptr(), Some(node))
}

pub(crate) fn page_url(js: Js) -> Option<CString> {
    api::page_url(js.ptr())
}

pub(crate) fn document_origin(js: Js) -> Option<CString> {
    api::document_origin(js.ptr())
}

pub(crate) fn top_url(js: Js) -> CString {
    api::top_url(js.ptr())
}

pub(crate) fn node_frame_url(node: Node<'_>) -> Option<CString> {
    api::node_frame_url(node)
}

pub(crate) fn node_doc_base(js: Js, node: Node<'_>) -> Option<CString> {
    api::node_doc_base(js.ptr(), Some(node))
}

pub(crate) fn same_origin(a: &CStr, b: &CStr) -> bool {
    southstar_url::bytes::same_origin(a.to_bytes(), b.to_bytes())
}

pub(crate) fn is_http_or_https(url: Option<&CStr>) -> bool {
    url.is_some_and(|url| southstar_url::bytes::is_http_or_https(url.to_bytes()))
}

pub(crate) fn origin_from(url: Option<&CStr>) -> Option<CString> {
    url.and_then(|url| southstar_url::bytes::origin_from(url.to_bytes()))
        .map(c_string)
}

pub(crate) fn referer_for(
    url: Option<&CStr>,
    referrer_url: Option<&CStr>,
    policy: c_int,
) -> Option<CString> {
    owned_text(unsafe { ns_net_referer_for(opt_ptr(url), opt_ptr(referrer_url), policy) })
}

#[derive(Clone, Copy)]
pub(crate) enum FrameKind {
    Frame,
    Object,
}

pub(crate) fn page_csp_allows(js: Js, kind: FrameKind, url: &CStr, page: &CStr) -> bool {
    let csp = api::page_csp(js.ptr()).cast::<NsCsp>();
    if csp.is_null() {
        return true;
    }
    let kind = match kind {
        FrameKind::Frame => CSP_FRAME,
        FrameKind::Object => CSP_OBJECT,
    };
    unsafe { ns_csp_allows(csp, kind, url.as_ptr(), page.as_ptr()) != 0 }
}

pub(crate) fn csp_header_allows_framing(header: &CStr, embedder: &CStr, framed: &CStr) -> bool {
    let csp = unsafe { ns_csp_parse(header.as_ptr()) };
    let allowed =
        unsafe { ns_csp_allows(csp, CSP_FRAME_ANCESTORS, embedder.as_ptr(), framed.as_ptr()) != 0 };
    unsafe { ns_csp_free(csp) };
    allowed
}

fn trimmed_at_nul(line: &[u8]) -> CString {
    let end = line.iter().position(|&b| b == 0).unwrap_or(line.len());
    CString::new(&line[..end]).unwrap_or_default()
}

pub(crate) fn log_enabled(js: Js) -> bool {
    api::log_enabled(js.ptr())
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    api::log_line(js.ptr(), &trimmed_at_nul(line));
}

pub(crate) fn start_frame_clock(js: Js, frame: Node<'_>) {
    unsafe { ns_js_start_frame_clock(js.ptr(), frame.as_ptr().cast()) };
}

pub(crate) fn fetch_resource(
    js: Js,
    url: &CStr,
    top_url: &CStr,
    header: &CStr,
    form_body: Option<&[u8]>,
) -> (Option<Response>, Option<CString>) {
    let upload = form_body.map(|body| Upload {
        method: c"POST",
        body,
        content_type: Some(c"application/x-www-form-urlencoded"),
    });
    let fetched = api::fetch_resource(js.ptr(), url, Some(top_url), &[header], upload);
    (Response::take(fetched.response.cast()), fetched.error)
}

pub(crate) fn fetch_script(
    js: Js,
    script: Node<'_>,
    url: &CStr,
    top_url: &CStr,
) -> Option<Response> {
    let info = element_perf_info(js, script);
    let fetched = api::fetch_subresource(js.ptr(), url, Some(top_url), &[], Some(c"script"), &info);
    Response::take(fetched.response.cast())
}

pub(crate) fn element_perf_info(js: Js, element: Node<'_>) -> ResourceInfo {
    let mut info = ResourceInfo::default();
    unsafe { ns_js_element_perf_info(js.ptr(), element.as_ptr(), &mut info) };
    info
}

pub(crate) fn add_resource_timing(
    js: Js,
    info: &ResourceInfo,
    url: &CStr,
    initiator: &CStr,
    span: (i64, i64),
    resp: Option<&Response>,
) {
    unsafe {
        ns_perf_add_resource_timed(
            js.ptr(),
            info,
            url.as_ptr(),
            initiator.as_ptr(),
            span.0,
            span.1,
            resp.map_or(core::ptr::null(), Response::as_ptr),
        )
    };
}

pub(crate) fn header_is_nosniff(value: Option<&CStr>) -> bool {
    unsafe { ns_net_header_is_nosniff(opt_ptr(value)) != 0 }
}

pub(crate) fn content_type_is_javascript(content_type: Option<&CStr>) -> bool {
    unsafe { ns_content_type_is_javascript(opt_ptr(content_type)) != 0 }
}

pub(crate) fn image_document(url: &CStr) -> Option<CString> {
    Some(c_string(southstar_html_util::image_document(
        url.to_bytes(),
    )))
}

pub(crate) fn decode_body(body: &[u8], content_type: Option<&CStr>) -> Vec<u8> {
    southstar_html_util::decode_body(body, content_type.map(CStr::to_bytes)).0
}

pub(crate) fn declared_charset(body: &[u8], content_type: Option<&CStr>) -> Option<CString> {
    southstar_html_util::declared_charset(Some(body), content_type.map(CStr::to_bytes))
        .map(|name| c_string(name.as_bytes().to_vec()))
}

pub(crate) fn html_parse(input: &[u8]) -> Tree {
    southstar_html_parser::parse_document(input, true)
}

pub(crate) fn xml_parse(input: &[u8]) -> Option<Tree> {
    southstar_xml::parse_document(input).ok()
}

pub(crate) fn xml_well_formed(input: &[u8]) -> (bool, Option<CString>) {
    let mut root_ns: *mut c_char = core::ptr::null_mut();
    let len = isize::try_from(input.len()).unwrap_or(isize::MAX);
    let well_formed = unsafe { ns_xml_well_formed(input.as_ptr().cast(), len, &mut root_ns) != 0 };
    (well_formed, owned_text(root_ns))
}

pub(crate) fn new_text(text: &[u8]) -> Node<'static> {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn mark_attr_dirty(node: Node<'_>, name: &CStr) {
    southstar_css::mark_attr_dirty(node, Some(name.to_bytes()), None);
}

pub(crate) fn dispatch_resource_event(js: Js, target: Node<'_>, kind: &CStr) {
    unsafe { ns_js_dispatch_resource_event(js.ptr(), target.as_ptr(), kind.as_ptr()) };
}

pub(crate) fn dispatch_event(js: Js, target: Node<'_>, kind: &CStr) {
    unsafe {
        ns_js_dispatch_event(
            js.ptr(),
            target.as_ptr(),
            kind.as_ptr(),
            core::ptr::null_mut(),
        )
    };
}

pub(crate) fn fire_page_transition(js: Js, kind: &CStr) {
    unsafe { ns_js_fire_page_transition(js.ptr(), kind.as_ptr(), glib::FALSE) };
}

pub(crate) fn mark_scripts_already_started(root: Node<'_>) {
    unsafe { ns_js_mark_scripts_already_started(root.as_mut_ptr()) };
}

struct Tasks(*mut GArray);

impl Tasks {
    fn collect(root: Node<'_>) -> Tasks {
        let tasks = unsafe {
            glib::g_array_new(glib::FALSE, glib::FALSE, size_of::<ScriptTask>() as c_uint)
        };
        unsafe { ns_js_collect_script_tasks(root.as_mut_ptr(), tasks) };
        Tasks(tasks)
    }

    fn nodes(&self) -> Vec<Node<'static>> {
        let array = unsafe { &*self.0 };
        if array.data.is_null() {
            return Vec::new();
        }
        let tasks = unsafe {
            core::slice::from_raw_parts(array.data.cast::<ScriptTask>(), array.len as usize)
        };
        tasks
            .iter()
            .filter_map(|task| unsafe { Node::from_ptr(task.node) })
            .collect()
    }
}

impl Drop for Tasks {
    fn drop(&mut self) {
        unsafe { g_array_free(self.0, glib::TRUE) };
    }
}

pub(crate) fn script_tasks(root: Node<'_>) -> Vec<Node<'static>> {
    Tasks::collect(root).nodes()
}

pub(crate) fn run_page_realm_scripts(js: Js, root: Node<'_>, origin: &CStr) {
    let tasks = Tasks::collect(root);
    unsafe {
        ns_js_run_parser_blocking_scripts(js.ptr(), tasks.0, origin.as_ptr());
        ns_js_run_script_schedule(js.ptr(), tasks.0, SCRIPT_DEFERRED, origin.as_ptr());
        ns_js_run_script_schedule(js.ptr(), tasks.0, SCRIPT_ASYNC, origin.as_ptr());
    }
}

pub(crate) fn schedule_pending_script_drain(js: Js) {
    unsafe { ns_js_schedule_pending_script_drain(js.ptr()) };
}

pub(crate) fn script_type_supported(script: Node<'_>) -> bool {
    unsafe { ns_script_type_supported(script.as_ptr()) != 0 }
}

pub(crate) fn script_skipped_by_nomodule(script: Node<'_>) -> bool {
    unsafe { ns_script_skipped_by_nomodule(script.as_ptr()) != 0 }
}

pub(crate) fn script_type_is_module(script: Node<'_>) -> bool {
    unsafe { ns_script_type_is_module(script.as_ptr()) != 0 }
}

pub(crate) fn deny_cookie(ctx: Ctx, doc: &Value) {
    ctx.try_enter(|scope| nsr_realmdoc_deny_cookie(scope, doc));
}

pub(crate) fn link_outward(js: Js, outward: &Value, realm_window: &Value) {
    unsafe { nsr_window_link_outward(js.ptr(), outward, realm_window) };
}

pub(crate) fn eval_budget_us() -> i64 {
    api::eval_budget_us()
}

pub(crate) fn in_frame_scope<R>(js: Js, realm: Ctx, frame: Node<'_>, f: impl FnOnce() -> R) -> R {
    api::in_frame_scope(js.ptr(), realm.handle(), frame, f)
}

pub(crate) fn during_frame_load<R>(
    js: Js,
    frame: Node<'_>,
    doc: Node<'_>,
    url: &CStr,
    realm_doc: &Value,
    f: impl FnOnce() -> R,
) -> R {
    api::during_frame_load(js.ptr(), frame, doc, url, realm_doc, f)
}

pub(crate) fn evaluating<R>(
    js: Js,
    script: Option<Node<'_>>,
    deadline_us: i64,
    f: impl FnOnce() -> R,
) -> R {
    api::frame_eval(js.ptr(), script, deadline_us, f)
}

pub(crate) fn purge_frame_callbacks(js: Js, frame: Node<'_>) {
    unsafe {
        ns_js_purge_frame_rafs(js.ptr(), frame.as_ptr());
        ns_services_purge_frame_timers(js.ptr(), frame.as_ptr());
    }
}

pub(crate) fn orphan_document(js: Js, doc: Node<'_>) {
    doc.detach();
    api::unpin_subtree(js.ptr(), doc);
    unsafe { ns_js_track_orphan(js.ptr(), doc.as_mut_ptr()) };
}

pub(crate) fn sweep_orphans(js: Js) {
    api::sweep_orphans(js.ptr());
}
