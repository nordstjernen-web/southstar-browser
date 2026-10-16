//! Southstar — the C ABI and nsr_ entry points of script, module and stylesheet loading, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::{
    Schedule, Task, blocking, compile, eval, fetch, hold, import_map, page, scan, schedule,
};

pub(crate) use southstar_js_runtime::api::NsJs;
pub(crate) use southstar_js_runtime::api::ResourceInfo as PerfInfo;

#[repr(C)]
struct NsWorkerHost {
    _private: [u8; 0],
}

#[repr(C)]
struct GBytes {
    _private: [u8; 0],
}

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

#[repr(C)]
struct RawTask {
    node: *mut NsNode,
    schedule: c_int,
}

pub(crate) const CSP_SCRIPT: usize = southstar_csp::SCRIPT;
pub(crate) const CSP_STYLE: usize = southstar_csp::STYLE;
const FETCH_DEST_SCRIPT: c_int = 1;

unsafe extern "C" {
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_worker_host_base_url(host: *const NsWorkerHost) -> *const c_char;
    fn ns_js_node_document_base_url(js: *mut NsJs, node: *const NsNode) -> *mut c_char;
    fn ns_js_dispatch_resource_event(
        js: *mut NsJs,
        target: *const NsNode,
        ty: *const c_char,
    ) -> GBoolean;
    fn ns_js_frame_context(js: *const NsJs, frame: *const NsNode) -> ContextHandle;
    fn ns_js_frame_url(js: *const NsJs, frame: *const NsNode) -> *const c_char;
    fn ns_js_blob_url_lookup(
        js: *mut NsJs,
        url: *const c_char,
        out_type: *mut *mut c_char,
    ) -> *mut GBytes;
    fn ns_data_url_decode(
        url: *const c_char,
        out: *mut GByteArray,
        budget: u64,
        out_content_type: *mut *mut c_char,
        too_large: *mut GBoolean,
    ) -> GBoolean;
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_free(array: *mut GByteArray, free_segment: GBoolean) -> *mut u8;
    fn g_array_remove_index(array: *mut GArray, index: c_uint) -> *mut GArray;
    fn ns_js_flush_document_write(js: *mut NsJs);
    fn ns_js_node_realm_context(js: *const NsJs, node: *const NsNode) -> ContextHandle;
    fn ns_js_lifecycle_tasks(js: *const NsJs) -> *mut GArray;
    fn ns_js_index_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
    );
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_js_schedule_static_iframes(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_rescan_pending_images(js: *mut NsJs, root: *mut NsNode);
    fn ns_net_accept_headers_for(dest: c_int) -> *const *const c_char;
    fn ns_net_header_is_nosniff(value: *const c_char) -> GBoolean;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_security_sri_check(integrity: *const c_char, body: *const c_void, len: usize)
    -> GBoolean;
    fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
    fn g_source_remove(tag: c_uint) -> GBoolean;
    fn g_array_append_vals(array: *mut GArray, data: *const c_void, len: c_uint) -> *mut GArray;
    fn g_get_monotonic_time() -> i64;
}

unsafe extern "Rust" {
    unsafe fn nsr_js_report_uncaught(js: *mut NsJs, exception: &Value, origin: Option<&str>);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

fn owned(p: *mut c_char) -> Option<CString> {
    if p.is_null() {
        return None;
    }
    let out = unsafe { CStr::from_ptr(p) }.to_owned();
    unsafe { glib::g_free(p.cast()) };
    Some(out)
}

fn c_string(bytes: &[u8]) -> CString {
    CString::new(
        bytes
            .iter()
            .copied()
            .filter(|&b| b != 0)
            .collect::<Vec<u8>>(),
    )
    .unwrap_or_default()
}

fn opt_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub(crate) fn is_named(n: Node, tag: &CStr) -> bool {
    n.element_name() == Some(tag.to_bytes())
}

pub(crate) fn hidden_child(n: Node) -> bool {
    southstar_dom::serialize::is_embedded_doc(n)
        || unsafe { ns_node_is_shadow_root(n.as_ptr()) != 0 }
}

pub(crate) fn node_remove(n: Node) {
    n.detach()
}

pub(crate) fn append_child(parent: Node, child: Node) {
    parent.append(child)
}

pub(crate) fn arm_invalidate(n: Option<Node>) {
    if let Some(n) = n {
        api::arm_js_invalidate(n);
    }
}

pub(crate) fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    let mut resolved = southstar_url::bytes::resolve(base.map(CStr::to_bytes), href.to_bytes())?;
    if let Some(nul) = resolved.iter().position(|&b| b == 0) {
        resolved.truncate(nul);
    }
    Some(CString::new(resolved).unwrap_or_default())
}

pub(crate) fn url_is_http_or_https(url: &CStr) -> bool {
    southstar_url::bytes::is_http_or_https(url.to_bytes())
}

pub(crate) fn decode_data_url(url: &CStr) -> Option<Vec<u8>> {
    unsafe {
        let body = g_byte_array_new();
        let decoded = ns_data_url_decode(
            url.as_ptr(),
            body,
            crate::MAX_SCRIPT_BYTES as u64,
            ptr::null_mut(),
            ptr::null_mut(),
        ) != 0;
        let out = decoded.then(|| glib::slice((*body).data, (*body).len as usize).to_vec());
        g_byte_array_free(body, glib::TRUE);
        out
    }
}

pub(crate) fn sri_check(integrity: Option<&CStr>, body: &[u8]) -> bool {
    unsafe { ns_security_sri_check(opt_ptr(integrity), body.as_ptr().cast(), body.len()) != 0 }
}

pub(crate) fn source_remove(tag: u32) {
    unsafe { g_source_remove(tag) };
}

pub(crate) struct Response(*mut NsResponse);

impl Response {
    fn get(&self) -> &NsResponse {
        unsafe { &*self.0 }
    }

    pub(crate) fn status(&self) -> c_long {
        self.get().status
    }

    pub(crate) fn body(&self) -> Option<&[u8]> {
        let body = self.get().body;
        (!body.is_null()).then(|| unsafe { glib::slice((*body).data, (*body).len as usize) })
    }

    pub(crate) fn error(&self) -> Option<&CStr> {
        unsafe { cstr(self.get().error) }
    }

    pub(crate) fn content_type(&self) -> Option<&CStr> {
        unsafe { cstr(self.get().content_type) }
    }

    pub(crate) fn nosniff(&self) -> bool {
        unsafe { ns_net_header_is_nosniff(self.get().x_content_type_options) != 0 }
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { ns_response_free(self.0) }
    }
}

pub(crate) struct Fetched {
    pub response: Option<Response>,
    pub error: Option<CString>,
}

impl Js {
    pub(crate) fn of(js: *mut NsJs) -> Js {
        Js(js)
    }

    pub(crate) fn key(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub(crate) fn halted(self) -> bool {
        api::halted(self.0)
    }

    pub(crate) fn in_pump(self) -> bool {
        api::in_pump(self.0)
    }

    pub(crate) fn eval_depth(self) -> i32 {
        api::eval_depth(self.0)
    }

    pub(crate) fn callback_depth(self) -> i32 {
        api::callback_depth(self.0)
    }

    pub(crate) fn dispatch_depth(self) -> i32 {
        api::dispatch_depth(self.0)
    }

    pub(crate) fn current_doc(self) -> Option<Node<'static>> {
        api::current_document(self.0)
    }

    pub(crate) fn log(self, line: &[u8]) {
        api::log_line(self.0, &c_string(line));
    }

    pub(crate) fn mark_mutated(self) {
        api::mark_mutated(self.0);
    }

    pub(crate) fn in_page(self, n: Node) -> bool {
        api::node_in_page(self.0, Some(n))
    }

    pub(crate) fn base_url(self, n: Node) -> Option<CString> {
        owned(unsafe { ns_js_node_document_base_url(self.0, n.as_ptr()) })
    }

    pub(crate) fn dispatch_resource_event(self, n: Node, ty: &CStr) {
        unsafe { ns_js_dispatch_resource_event(self.0, n.as_ptr(), ty.as_ptr()) };
    }

    pub(crate) fn upgrade_all(self) {
        if let Some(doc) = self.current_doc() {
            southstar_js_custom_elements::upgrade_subtree_all(self.0, doc);
        }
    }

    pub(crate) fn ce_upgrading(self) -> bool {
        southstar_js_custom_elements::upgrading(self.0)
    }

    pub(crate) fn drain_microtasks(self) {
        api::drain_microtasks(self.0);
    }

    pub(crate) fn schedule_static_iframes(self, root: Node) {
        unsafe { ns_js_schedule_static_iframes(self.0, root.as_mut_ptr()) }
    }

    pub(crate) fn rescan_images(self, root: Node) {
        unsafe { ns_js_rescan_pending_images(self.0, root.as_mut_ptr()) }
    }

    pub(crate) fn attach_drain_timer(self) -> u32 {
        api::attach_timeout(self.0, 4, async_script_timer, self.0.cast())
    }

    pub(crate) fn index_child_removed(self, parent: Option<Node>, removed: Node) {
        unsafe {
            ns_js_index_child_change(
                self.0,
                Node::ptr_or_null(parent).cast_mut(),
                ptr::null_mut(),
                removed.as_mut_ptr(),
            )
        }
    }

    pub(crate) fn record_child_added(self, parent: Node, added: Node, prev: Option<Node>) {
        unsafe {
            ns_js_record_child_change(
                self.0,
                parent.as_mut_ptr(),
                added.as_mut_ptr(),
                ptr::null_mut(),
                Node::ptr_or_null(prev).cast_mut(),
                ptr::null_mut(),
            )
        }
    }

    pub(crate) fn eval_script_source(
        self,
        script: Node,
        source: &[u8],
        origin: Option<&CStr>,
        kind: crate::eval::ScriptKind,
    ) {
        crate::eval::eval_script_source(self, script, source, origin, kind);
    }

    pub(crate) fn forget_script_roots_in(self, root: Node) {
        page::forget_roots_in(self, root.as_ptr() as usize);
    }

    pub(crate) fn has_log(self) -> bool {
        api::log_enabled(self.0)
    }

    pub(crate) fn eval_depth_add(self, delta: i32) -> i32 {
        api::eval_depth_add(self.0, delta)
    }

    pub(crate) fn ignore_destructive_writes_add(self, delta: i32) {
        api::ignore_destructive_writes_add(self.0, delta);
    }

    pub(crate) fn current_script(self) -> Option<Node<'static>> {
        api::current_script(self.0)
    }

    pub(crate) fn swap_current_script(self, script: Option<Node>) -> Option<Node<'static>> {
        api::swap_current_script(self.0, script)
    }

    pub(crate) fn swap_current_doc(self, doc: Option<Node>) -> Option<Node<'static>> {
        api::swap_current_document(self.0, doc)
    }

    pub(crate) fn main_realm(self) -> Realm {
        Realm(api::main_context(self.0))
    }

    pub(crate) fn module_realm(self) -> Realm {
        Realm(api::module_context(self.0))
    }

    pub(crate) fn swap_module_realm(self, realm: Option<Realm>) -> Option<Realm> {
        let ctx = realm.map_or(ContextHandle::default(), |realm| realm.0);
        api::swap_module_context(self.0, ctx).non_null().map(Realm)
    }

    pub(crate) fn node_realm(self, n: Node) -> Option<Realm> {
        unsafe { ns_js_node_realm_context(self.0, n.as_ptr()) }
            .non_null()
            .map(Realm)
    }

    pub(crate) fn iframe_document(self) -> Option<Value> {
        api::iframe_document(self.0)
    }

    pub(crate) fn frame_url_push(self, url: Option<&CStr>) -> FrameUrl {
        FrameUrl(api::enter_frame_url(self.0, url))
    }

    pub(crate) fn frame_url_pop(self, saved: FrameUrl) {
        if let Some(scope) = saved.0 {
            scope.leave();
        }
    }

    pub(crate) fn apply_site_quirks(self) {
        api::apply_site_quirks(self.0);
    }

    pub(crate) fn budget_enter(self) -> i64 {
        api::budget_enter(self.0)
    }

    pub(crate) fn budget_leave(self, saved: i64) {
        api::budget_leave(self.0, saved);
    }

    pub(crate) fn flush_document_write(self) {
        unsafe { ns_js_flush_document_write(self.0) }
    }

    pub(crate) fn report_uncaught(self, exception: &Value, origin: Option<&CStr>) {
        let origin = origin.map(CStr::to_string_lossy);
        unsafe { nsr_js_report_uncaught(self.0, exception, origin.as_deref()) }
    }

    pub(crate) fn content_scripts_for_url(
        self,
        url: Option<&CStr>,
        document_start: bool,
    ) -> Option<CString> {
        let script = self.with_main_scope(|scope| {
            let global = scope.global();
            southstar_extensions::content_scripts_for_url(
                scope,
                &global,
                url.map(CStr::to_bytes),
                document_start,
            )
        })??;
        Some(c_string(&script))
    }

    pub(crate) fn remove_lifecycle_tasks(self, mut doomed: impl FnMut(Option<Node>) -> bool) {
        let tasks = unsafe { ns_js_lifecycle_tasks(self.0) };
        if tasks.is_null() {
            return;
        }
        let mut i = 0u32;
        while i < unsafe { (*tasks).len } {
            let task = unsafe { &*(*tasks).data.cast::<RawTask>().add(i as usize) };
            if doomed(node(task.node)) {
                unsafe { g_array_remove_index(tasks, i) };
            } else {
                i += 1;
            }
        }
    }

    pub(crate) fn with_blob<R>(self, url: &CStr, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let blob = unsafe { ns_js_blob_url_lookup(self.0, url.as_ptr(), ptr::null_mut()) };
        if blob.is_null() {
            return None;
        }
        let mut len = 0usize;
        let data = unsafe { g_bytes_get_data(blob, &mut len) };
        let bytes = if data.is_null() {
            &[][..]
        } else {
            unsafe { glib::slice(data.cast(), len) }
        };
        Some(f(bytes))
    }

    pub(crate) fn csp_allows(
        self,
        kind: usize,
        url: &CStr,
        origin: Option<&CStr>,
        nonce: Option<&CStr>,
        parser_inserted: bool,
    ) -> bool {
        let nonce = nonce.map(CStr::to_bytes);
        api::with_page_csp(self.0, |csp| {
            csp.is_none_or(|csp| csp.allows(kind, url, origin, nonce, parser_inserted))
        })
    }

    pub(crate) fn inline_script_allowed(self, text: &[u8], nonce: Option<&CStr>) -> bool {
        let nonce = nonce.map(CStr::to_bytes);
        api::with_page_csp(self.0, |csp| {
            csp.is_none_or(|csp| csp.inline_script_allowed(text, nonce))
        })
    }

    pub(crate) fn element_perf_info(self, n: Node) -> PerfInfo {
        let mut info = PerfInfo::default();
        fill_element_perf_info(self.0, Some(n), &mut info);
        info
    }

    pub(crate) fn module_perf_info(self, ctx: ContextHandle) -> PerfInfo {
        PerfInfo {
            timeline: ctx.as_ptr().cast_const(),
            document_url: api::realm_document_url_ptr(self.0, ctx),
            cors_mode: glib::TRUE,
            ..PerfInfo::default()
        }
    }

    pub(crate) fn module_top_url(self) -> Option<CString> {
        if self.is_null() {
            return None;
        }
        let host = api::worker_host(self.0).cast::<NsWorkerHost>();
        if host.is_null() {
            return Some(api::current_url(self.0));
        }
        unsafe { cstr(ns_worker_host_base_url(host)) }.map(CStr::to_owned)
    }

    pub(crate) fn fetch(
        self,
        url: &CStr,
        top_url: Option<&CStr>,
        script_accept: bool,
        initiator: &CStr,
        info: &PerfInfo,
    ) -> Fetched {
        let headers = if script_accept {
            script_accept_headers()
        } else {
            Vec::new()
        };
        let fetched = api::fetch_subresource(self.0, url, top_url, &headers, Some(initiator), info);
        let resp = fetched.response.cast::<NsResponse>();
        Fetched {
            response: (!resp.is_null()).then(|| Response(resp)),
            error: fetched.error,
        }
    }

    pub(crate) fn with_main_scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        api::main_context(self.0).with(f)
    }
}

fn script_accept_headers() -> Vec<&'static CStr> {
    let mut headers = Vec::new();
    let mut at = unsafe { ns_net_accept_headers_for(FETCH_DEST_SCRIPT) };
    if at.is_null() {
        return headers;
    }
    while let Some(header) = unsafe { cstr(*at) } {
        headers.push(header);
        at = unsafe { at.add(1) };
    }
    headers
}

pub(crate) struct FrameUrl(Option<api::UrlScope>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(ContextHandle);

impl Realm {
    pub(crate) fn enter<R: Default>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        self.0.with(f).unwrap_or_default()
    }
}

unsafe extern "C" fn async_script_timer(data: *mut c_void) -> GBoolean {
    let js = Js(data.cast());
    if !js.is_null() {
        schedule::drain_timer_fired(js);
    }
    glib::FALSE
}

unsafe fn tasks_of<'a>(tasks: *const GArray) -> Vec<Task<'a>> {
    if tasks.is_null() || unsafe { (*tasks).data.is_null() || (*tasks).len == 0 } {
        return Vec::new();
    }
    let raw = unsafe {
        core::slice::from_raw_parts((*tasks).data.cast::<RawTask>(), (*tasks).len as usize)
    };
    raw.iter()
        .filter_map(|task| {
            Some(Task {
                node: node(task.node)?,
                schedule: Schedule::from_raw(task.schedule)?,
            })
        })
        .collect()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_script_type_is_module(n: *const NsNode) -> GBoolean {
    glib::boolean(node(n).is_some_and(crate::type_is_module))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_script_type_supported(n: *const NsNode) -> GBoolean {
    glib::boolean(node(n).is_some_and(crate::type_supported))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_script_skipped_by_nomodule(n: *const NsNode) -> GBoolean {
    glib::boolean(node(n).is_some_and(crate::skipped_by_nomodule))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_content_type_is_javascript(ct: *const c_char) -> GBoolean {
    let ct = unsafe { glib::bytes(ct) };
    glib::boolean(ct.is_some_and(crate::content_type_is_javascript))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mark_scripts_already_started(n: *mut NsNode) {
    if let Some(n) = node(n) {
        scan::mark_scripts_already_started(n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_collect_script_tasks(n: *mut NsNode, tasks: *mut GArray) {
    let (Some(n), false) = (node(n), tasks.is_null()) else {
        return;
    };
    for task in scan::collect_script_tasks(n) {
        let raw = RawTask {
            node: task.node.as_mut_ptr(),
            schedule: task.schedule.raw(),
        };
        unsafe { g_array_append_vals(tasks, (&raw as *const RawTask).cast(), 1) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_register_import_maps(js: *mut NsJs, root: *mut NsNode) {
    if let (false, Some(root)) = (js.is_null(), node(root)) {
        import_map::register_in(Js(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_script_element(
    js: *mut NsJs,
    n: *mut NsNode,
    origin: *const c_char,
) {
    if let (false, Some(n)) = (js.is_null(), node(n)) {
        fetch::run_script_element(Js(js), n, unsafe { cstr(origin) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_parser_blocking_scripts(
    js: *mut NsJs,
    tasks: *mut GArray,
    origin: *const c_char,
) {
    if js.is_null() || tasks.is_null() {
        return;
    }
    let tasks = unsafe { tasks_of(tasks) };
    hold::run_parser_blocking_scripts(Js(js), &tasks, unsafe { cstr(origin) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_script_schedule(
    js: *mut NsJs,
    tasks: *mut GArray,
    which: c_int,
    origin: *const c_char,
) {
    if js.is_null() || tasks.is_null() {
        return;
    }
    let Some(which) = Schedule::from_raw(which) else {
        return;
    };
    let tasks = unsafe { tasks_of(tasks) };
    schedule::run_schedule(Js(js), &tasks, which, unsafe { cstr(origin) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_next_script_schedule(
    js: *mut NsJs,
    tasks: *mut GArray,
    which: c_int,
    origin: *const c_char,
) -> GBoolean {
    if js.is_null() || tasks.is_null() {
        return glib::FALSE;
    }
    let Some(which) = Schedule::from_raw(which) else {
        return glib::FALSE;
    };
    let tasks = unsafe { tasks_of(tasks) };
    glib::boolean(schedule::run_next(Js(js), &tasks, which, unsafe {
        cstr(origin)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_schedule_pending_script_drain(js: *mut NsJs) {
    if !js.is_null() {
        schedule::schedule_pending_drain(Js(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drain_load_event_scripts(js: *mut NsJs) {
    if !js.is_null() {
        schedule::drain_load_event_scripts(Js(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_pending_script_roots(js: *const NsJs) -> GBoolean {
    glib::boolean(!js.is_null() && page::has_pending_roots(Js(js.cast_mut())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_async_scripts_pending(js: *const NsJs) -> GBoolean {
    glib::boolean(!js.is_null() && page::async_pending(Js(js.cast_mut())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_script_needs_prepare(js: *mut NsJs, script: *mut NsNode) {
    if let (false, Some(script)) = (js.is_null(), node(script)) {
        schedule::script_needs_prepare(Js(js), script);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_inserted_scripts(js: *mut NsJs, root: *mut NsNode) {
    if let (false, Some(root)) = (js.is_null(), node(root)) {
        schedule::run_inserted_scripts(Js(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_parser_hold_forget(js: *mut NsJs, n: *const NsNode) {
    if !js.is_null() && !n.is_null() {
        hold::forget(Js(js), n as usize);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_cancel_async_scripts(js: *mut NsJs) {
    if !js.is_null() {
        page::cancel_async(Js(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_loader_teardown(js: *mut NsJs) {
    if !js.is_null() {
        page::teardown(Js(js));
    }
}

unsafe fn bytes<'a>(p: *const c_char, len: usize) -> &'a [u8] {
    if p.is_null() || len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(p.cast(), len) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_profile_enabled() -> GBoolean {
    GBoolean::from(compile::profile_enabled())
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval(
    js: *mut NsJs,
    src: *const c_char,
    len: usize,
    origin: *const c_char,
) {
    let src = unsafe { bytes(src, len) };
    eval::eval(Js(js), src, unsafe { cstr(origin) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_decode_data_url(
    url: *const c_char,
    out_len: *mut usize,
) -> *mut c_char {
    let Some(body) = (unsafe { cstr(url) }).and_then(decode_data_url) else {
        return ptr::null_mut();
    };
    if !out_len.is_null() {
        unsafe { *out_len = body.len() };
    }
    glib::strdup(&body)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_script_refs(js: *mut NsJs, root: *mut NsNode) {
    if let (false, Some(root)) = (js.is_null(), node(root)) {
        eval::purge_subtree_script_refs(Js(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_content_scripts(
    js: *mut NsJs,
    url: *const c_char,
    document_start: GBoolean,
) {
    eval::run_content_scripts(Js(js), unsafe { cstr(url) }, document_start != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_source(
    js: *mut NsJs,
    src: *const c_char,
    origin: *const c_char,
) -> *mut c_char {
    let Some(src) = (unsafe { cstr(src) }).filter(|_| !js.is_null()) else {
        return ptr::null_mut();
    };
    eval::eval_source(Js(js), src, unsafe { cstr(origin) })
        .map_or(ptr::null_mut(), |out| glib::strdup(&out))
}

fn fill_element_perf_info(js: *mut NsJs, el: Option<Node>, info: &mut PerfInfo) {
    let frame = el.and_then(blocking::content_frame);
    let frame_ptr = Node::ptr_or_null(frame);
    let has_js = !js.is_null();
    let frame_ctx = if frame.is_some() && has_js {
        unsafe { ns_js_frame_context(js, frame_ptr) }
    } else {
        ContextHandle::default()
    };
    info.timeline = if frame_ctx.is_null() {
        frame_ptr.cast()
    } else {
        frame_ctx.as_ptr().cast_const().cast()
    };
    let frame_url = if frame.is_some() && has_js {
        unsafe { ns_js_frame_url(js, frame_ptr) }
    } else {
        ptr::null()
    };
    info.document_url = if !frame_url.is_null() {
        frame_url
    } else if has_js {
        api::page_url_ptr(js)
    } else {
        ptr::null()
    };
    info.render_blocking = glib::boolean(el.is_some_and(blocking::render_blocking));
    info.cors_mode = glib::boolean(el.is_some_and(|el| el.attr(c"crossorigin").is_some()));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_element_perf_info(
    js: *mut NsJs,
    el: *const NsNode,
    info: *mut PerfInfo,
) {
    if let Some(info) = unsafe { info.as_mut() } {
        fill_element_perf_info(js, node(el), info);
    }
}

#[unsafe(no_mangle)]
pub fn nsr_js_compile_module_cached(
    scope: &mut Scope<'_>,
    src: &[u8],
    name: &[u8],
) -> Result<Value, Value> {
    eval::compile_module_cached(scope, src, name)
}

#[unsafe(no_mangle)]
pub fn nsr_js_module_set_import_meta(
    scope: &mut Scope<'_>,
    module: &Value,
    is_main: bool,
) -> Result<(), Value> {
    compile::set_import_meta(scope, module, is_main)
}

#[unsafe(no_mangle)]
pub fn nsr_js_module_normalize(
    scope: &mut Scope<'_>,
    base: &str,
    name: &str,
) -> Result<String, Value> {
    let js = Js(scope.context_opaque().cast());
    let base = CString::new(base.replace('\0', "")).unwrap_or_default();
    let base = Some(base.as_c_str()).filter(|base| !base.is_empty());
    let name = CString::new(name.replace('\0', "")).unwrap_or_default();
    let resolved = import_map::resolve(js, name.to_bytes())
        .or_else(|| url_resolve(base, &name).map(CString::into_bytes))
        .unwrap_or_else(|| name.to_bytes().to_vec());
    Ok(String::from_utf8_lossy(&resolved).into_owned())
}

#[unsafe(no_mangle)]
pub fn nsr_js_module_load(
    scope: &mut Scope<'_>,
    name: &str,
    attributes: &Value,
) -> Result<Value, Value> {
    let js = Js(scope.context_opaque().cast());
    let json = compile::attributes_type_is(scope, attributes, b"json");
    let name = CString::new(name.replace('\0', "")).unwrap_or_default();
    eval::load_module(js, scope, &name, json)
}
