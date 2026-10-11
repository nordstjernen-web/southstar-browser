//! Southstar — the C ABI of script, module and stylesheet loading as declared in src/js_internal.h, and the js.c calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GError};
use southstar_js_engine::quickjs::{self, JSContext};
use southstar_js_engine::{Scope, Value};

use crate::{
    Schedule, Task, blocking, compile, eval, fetch, hold, import_map, page, scan, schedule,
};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
struct NsCsp {
    _private: [u8; 0],
}

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
pub struct PerfInfo {
    timeline: *const c_void,
    document_url: *const c_char,
    render_blocking: GBoolean,
    cors_mode: GBoolean,
    next_hop_protocol: *const c_char,
    timing_allow_origin: *const c_char,
    status: c_long,
    body_size: i64,
}

const _: () = assert!(core::mem::size_of::<PerfInfo>() == 56);

#[repr(C)]
struct RawTask {
    node: *mut NsNode,
    schedule: c_int,
}

pub(crate) const CSP_SCRIPT: c_uint = 1;
pub(crate) const CSP_STYLE: c_uint = 2;
const FETCH_DEST_SCRIPT: c_int = 1;

type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn ns_node_is_element_named(n: *const NsNode, tag: *const c_char) -> GBoolean;
    fn ns_node_is_embedded_doc(n: *const NsNode) -> GBoolean;
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_node_remove(child: *mut NsNode);
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_node_arm_js_invalidate(node: *mut NsNode);
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_eval_depth(js: *const NsJs) -> c_int;
    fn ns_js_callback_depth(js: *const NsJs) -> c_int;
    fn ns_js_dispatch_depth(js: *const NsJs) -> c_int;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_page_csp(js: *const NsJs) -> *const NsCsp;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_worker_host(js: *const NsJs) -> *mut NsWorkerHost;
    fn ns_worker_host_base_url(host: *const NsWorkerHost) -> *const c_char;
    fn ns_js_realm_document_url(js: *mut NsJs, realm: *mut JSContext) -> *const c_char;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_node_document_base_url(js: *mut NsJs, node: *const NsNode) -> *mut c_char;
    fn ns_js_dispatch_resource_event(
        js: *mut NsJs,
        target: *const NsNode,
        ty: *const c_char,
    ) -> GBoolean;
    fn ns_js_page_url(js: *const NsJs) -> *const c_char;
    fn ns_js_frame_context(js: *const NsJs, frame: *const NsNode) -> *mut JSContext;
    fn ns_js_frame_url(js: *const NsJs, frame: *const NsNode) -> *const c_char;
    fn ns_js_fetch_subresource(
        js: *mut NsJs,
        url: *const c_char,
        top_url: *const c_char,
        headers: *const *const c_char,
        error: *mut *mut GError,
        initiator: *const c_char,
        info: *const PerfInfo,
    ) -> *mut NsResponse;
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
    fn ns_js_has_log(js: *const NsJs) -> GBoolean;
    fn ns_js_eval_depth_add(js: *mut NsJs, delta: c_int) -> c_int;
    fn ns_js_ignore_destructive_writes_add(js: *mut NsJs, delta: c_int);
    fn ns_js_current_script(js: *const NsJs) -> *const NsNode;
    fn ns_js_swap_current_script(js: *mut NsJs, script: *mut NsNode) -> *mut NsNode;
    fn ns_js_swap_current_document(js: *mut NsJs, doc: *mut NsNode) -> *mut NsNode;
    fn ns_js_module_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_swap_module_context(js: *mut NsJs, ctx: *mut JSContext) -> *mut JSContext;
    fn ns_js_iframe_document(js: *const NsJs, out: *mut quickjs::JSValue) -> GBoolean;
    fn ns_js_frame_url_push(js: *mut NsJs, url: *const c_char) -> *mut c_void;
    fn ns_js_frame_url_pop(js: *mut NsJs, fu: *mut c_void);
    fn ns_js_apply_site_quirks(js: *mut NsJs);
    fn ns_js_budget_enter(js: *mut NsJs) -> i64;
    fn ns_js_budget_leave(js: *mut NsJs, saved: i64);
    fn ns_js_flush_document_write(js: *mut NsJs);
    fn ns_js_report_uncaught(js: *mut NsJs, ex: quickjs::JSValue, origin: *const c_char);
    fn ns_js_node_realm_context(js: *const NsJs, node: *const NsNode) -> *mut JSContext;
    fn ns_js_lifecycle_tasks(js: *const NsJs) -> *mut GArray;
    fn ns_ext_content_scripts_for_url(
        ctx: *mut JSContext,
        global: quickjs::JSValue,
        url: *const c_char,
        document_start: GBoolean,
    ) -> *mut c_char;
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
    fn ns_js_attach_timeout(
        js: *mut NsJs,
        ms: c_uint,
        func: GSourceFunc,
        data: *mut c_void,
    ) -> c_uint;
    fn ns_js_schedule_static_iframes(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_rescan_pending_images(js: *mut NsJs, root: *mut NsNode);
    fn ns_ce_upgrade_subtree_all(js: *mut NsJs, root: *mut NsNode);
    fn ns_ce_upgrading(js: *const NsJs) -> GBoolean;
    fn ns_drain_microtasks(js: *mut NsJs);
    fn ns_csp_allows_with_nonce(
        csp: *const NsCsp,
        kind: c_uint,
        resource_url: *const c_char,
        document_url: *const c_char,
        nonce: *const c_char,
        parser_inserted: GBoolean,
    ) -> GBoolean;
    fn ns_csp_inline_script_allowed(
        csp: *const NsCsp,
        body: *const c_char,
        body_len: usize,
        nonce: *const c_char,
    ) -> GBoolean;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean;
    fn ns_net_accept_headers_for(dest: c_int) -> *const *const c_char;
    fn ns_net_header_is_nosniff(value: *const c_char) -> GBoolean;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_security_sri_check(integrity: *const c_char, body: *const c_void, len: usize)
    -> GBoolean;
    fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
    fn g_source_remove(tag: c_uint) -> GBoolean;
    fn g_array_append_vals(array: *mut GArray, data: *const c_void, len: c_uint) -> *mut GArray;
    fn JS_ThrowRangeError(ctx: *mut JSContext, fmt: *const c_char, ...) -> quickjs::JSValue;
    fn JS_ThrowReferenceError(ctx: *mut JSContext, fmt: *const c_char, ...) -> quickjs::JSValue;
    fn js_strdup(ctx: *mut JSContext, s: *const c_char) -> *mut c_char;
    fn g_get_monotonic_time() -> i64;
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
    unsafe { ns_node_is_element_named(n.as_ptr(), tag.as_ptr()) != 0 }
}

pub(crate) fn hidden_child(n: Node) -> bool {
    unsafe { ns_node_is_embedded_doc(n.as_ptr()) != 0 || ns_node_is_shadow_root(n.as_ptr()) != 0 }
}

pub(crate) fn node_remove(n: Node) {
    unsafe { ns_node_remove(n.as_mut_ptr()) }
}

pub(crate) fn append_child(parent: Node, child: Node) {
    unsafe { ns_node_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) }
}

pub(crate) fn arm_invalidate(n: Option<Node>) {
    unsafe { ns_node_arm_js_invalidate(Node::ptr_or_null(n).cast_mut()) }
}

pub(crate) fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    owned(unsafe { ns_url_resolve(opt_ptr(base), href.as_ptr()) })
}

pub(crate) fn url_is_http_or_https(url: &CStr) -> bool {
    unsafe { ns_url_is_http_or_https(url.as_ptr()) != 0 }
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

impl PerfInfo {
    fn empty() -> PerfInfo {
        PerfInfo {
            timeline: ptr::null(),
            document_url: ptr::null(),
            render_blocking: 0,
            cors_mode: 0,
            next_hop_protocol: ptr::null(),
            timing_allow_origin: ptr::null(),
            status: 0,
            body_size: 0,
        }
    }
}

impl Js {
    pub(crate) fn key(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub(crate) fn halted(self) -> bool {
        unsafe { ns_js_halted(self.0) != 0 }
    }

    pub(crate) fn in_pump(self) -> bool {
        unsafe { ns_js_in_pump(self.0) != 0 }
    }

    pub(crate) fn eval_depth(self) -> i32 {
        unsafe { ns_js_eval_depth(self.0) }
    }

    pub(crate) fn callback_depth(self) -> i32 {
        unsafe { ns_js_callback_depth(self.0) }
    }

    pub(crate) fn dispatch_depth(self) -> i32 {
        unsafe { ns_js_dispatch_depth(self.0) }
    }

    pub(crate) fn current_doc(self) -> Option<Node<'static>> {
        node(unsafe { ns_js_current_document(self.0) })
    }

    pub(crate) fn log(self, line: &[u8]) {
        let line = c_string(line);
        unsafe { ns_js_log_line(self.0, line.as_ptr()) }
    }

    pub(crate) fn mark_mutated(self) {
        unsafe { ns_js_mark_mutated(self.0) }
    }

    pub(crate) fn in_page(self, n: Node) -> bool {
        unsafe { ns_js_node_in_page(self.0, n.as_ptr()) != 0 }
    }

    pub(crate) fn base_url(self, n: Node) -> Option<CString> {
        owned(unsafe { ns_js_node_document_base_url(self.0, n.as_ptr()) })
    }

    pub(crate) fn dispatch_resource_event(self, n: Node, ty: &CStr) {
        unsafe { ns_js_dispatch_resource_event(self.0, n.as_ptr(), ty.as_ptr()) };
    }

    pub(crate) fn upgrade_all(self) {
        let doc = Node::ptr_or_null(self.current_doc()).cast_mut();
        unsafe { ns_ce_upgrade_subtree_all(self.0, doc) }
    }

    pub(crate) fn ce_upgrading(self) -> bool {
        unsafe { ns_ce_upgrading(self.0) != 0 }
    }

    pub(crate) fn drain_microtasks(self) {
        unsafe { ns_drain_microtasks(self.0) }
    }

    pub(crate) fn schedule_static_iframes(self, root: Node) {
        unsafe { ns_js_schedule_static_iframes(self.0, root.as_mut_ptr()) }
    }

    pub(crate) fn rescan_images(self, root: Node) {
        unsafe { ns_js_rescan_pending_images(self.0, root.as_mut_ptr()) }
    }

    pub(crate) fn attach_drain_timer(self) -> u32 {
        unsafe { ns_js_attach_timeout(self.0, 4, async_script_timer, self.0.cast()) }
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
        is_module: bool,
    ) {
        crate::eval::eval_script_source(self, script, source, origin, is_module);
    }

    pub(crate) fn forget_script_roots_in(self, root: Node) {
        page::forget_roots_in(self, root.as_ptr() as usize);
    }

    pub(crate) fn has_log(self) -> bool {
        unsafe { ns_js_has_log(self.0) != 0 }
    }

    pub(crate) fn eval_depth_add(self, delta: i32) -> i32 {
        unsafe { ns_js_eval_depth_add(self.0, delta) }
    }

    pub(crate) fn ignore_destructive_writes_add(self, delta: i32) {
        unsafe { ns_js_ignore_destructive_writes_add(self.0, delta) }
    }

    pub(crate) fn current_script(self) -> Option<Node<'static>> {
        node(unsafe { ns_js_current_script(self.0) })
    }

    pub(crate) fn swap_current_script(self, script: Option<Node>) -> Option<Node<'static>> {
        node(unsafe { ns_js_swap_current_script(self.0, Node::ptr_or_null(script).cast_mut()) })
    }

    pub(crate) fn swap_current_doc(self, doc: Option<Node>) -> Option<Node<'static>> {
        node(unsafe { ns_js_swap_current_document(self.0, Node::ptr_or_null(doc).cast_mut()) })
    }

    pub(crate) fn main_realm(self) -> Realm {
        Realm(unsafe { ns_js_main_context(self.0) })
    }

    pub(crate) fn module_realm(self) -> Realm {
        Realm(unsafe { ns_js_module_context(self.0) })
    }

    pub(crate) fn swap_module_realm(self, realm: Option<Realm>) -> Option<Realm> {
        let ctx = realm.map_or(ptr::null_mut(), |realm| realm.0);
        let previous = unsafe { ns_js_swap_module_context(self.0, ctx) };
        (!previous.is_null()).then_some(Realm(previous))
    }

    pub(crate) fn node_realm(self, n: Node) -> Option<Realm> {
        let ctx = unsafe { ns_js_node_realm_context(self.0, n.as_ptr()) };
        (!ctx.is_null()).then_some(Realm(ctx))
    }

    pub(crate) fn iframe_document(self, scope: &Scope<'_>) -> Option<Value> {
        let mut raw = quickjs::UNDEFINED;
        let set = unsafe { ns_js_iframe_document(self.0, &mut raw) } != 0;
        set.then(|| unsafe { quickjs::borrow_value(scope, raw) })
    }

    pub(crate) fn frame_url_push(self, url: Option<&CStr>) -> FrameUrl {
        FrameUrl(unsafe { ns_js_frame_url_push(self.0, opt_ptr(url)) })
    }

    pub(crate) fn frame_url_pop(self, saved: FrameUrl) {
        unsafe { ns_js_frame_url_pop(self.0, saved.0) }
    }

    pub(crate) fn apply_site_quirks(self) {
        unsafe { ns_js_apply_site_quirks(self.0) }
    }

    pub(crate) fn budget_enter(self) -> i64 {
        unsafe { ns_js_budget_enter(self.0) }
    }

    pub(crate) fn budget_leave(self, saved: i64) {
        unsafe { ns_js_budget_leave(self.0, saved) }
    }

    pub(crate) fn flush_document_write(self) {
        unsafe { ns_js_flush_document_write(self.0) }
    }

    pub(crate) fn report_uncaught(self, exception: &Value, origin: Option<&CStr>) {
        unsafe { ns_js_report_uncaught(self.0, quickjs::raw(exception), opt_ptr(origin)) }
    }

    pub(crate) fn content_scripts_for_url(
        self,
        url: Option<&CStr>,
        document_start: bool,
    ) -> Option<CString> {
        let ctx = unsafe { ns_js_main_context(self.0) };
        let script = unsafe {
            quickjs::with_context(ctx, |scope| {
                let global = scope.global();
                ns_ext_content_scripts_for_url(
                    ctx,
                    quickjs::raw(&global),
                    opt_ptr(url),
                    glib::boolean(document_start),
                )
            })
        };
        owned(script)
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
        kind: c_uint,
        url: &CStr,
        origin: Option<&CStr>,
        nonce: Option<&CStr>,
        parser_inserted: bool,
    ) -> bool {
        unsafe {
            ns_csp_allows_with_nonce(
                ns_js_page_csp(self.0),
                kind,
                url.as_ptr(),
                opt_ptr(origin),
                opt_ptr(nonce),
                glib::boolean(parser_inserted),
            ) != 0
        }
    }

    pub(crate) fn inline_script_allowed(self, text: &[u8], nonce: Option<&CStr>) -> bool {
        unsafe {
            ns_csp_inline_script_allowed(
                ns_js_page_csp(self.0),
                text.as_ptr().cast(),
                text.len(),
                opt_ptr(nonce),
            ) != 0
        }
    }

    pub(crate) fn element_perf_info(self, n: Node) -> PerfInfo {
        let mut info = PerfInfo::empty();
        fill_element_perf_info(self.0, Some(n), &mut info);
        info
    }

    pub(crate) fn module_perf_info(self, ctx: Realm) -> PerfInfo {
        let mut info = PerfInfo::empty();
        info.timeline = ctx.0.cast();
        info.document_url = unsafe { ns_js_realm_document_url(self.0, ctx.0) };
        info.cors_mode = glib::TRUE;
        info
    }

    pub(crate) fn module_top_url(self) -> Option<&'static CStr> {
        if self.is_null() {
            return None;
        }
        unsafe {
            let host = ns_js_worker_host(self.0);
            cstr(if host.is_null() {
                ns_js_current_url(self.0)
            } else {
                ns_worker_host_base_url(host)
            })
        }
    }

    pub(crate) fn fetch(
        self,
        url: &CStr,
        top_url: Option<&CStr>,
        script_accept: bool,
        initiator: &CStr,
        info: &PerfInfo,
    ) -> Fetched {
        let mut err: *mut GError = ptr::null_mut();
        let headers = if script_accept {
            unsafe { ns_net_accept_headers_for(FETCH_DEST_SCRIPT) }
        } else {
            ptr::null()
        };
        let resp = unsafe {
            ns_js_fetch_subresource(
                self.0,
                url.as_ptr(),
                opt_ptr(top_url),
                headers,
                &mut err,
                initiator.as_ptr(),
                info,
            )
        };
        let error = (!err.is_null()).then(|| {
            let message = unsafe { cstr((*err).message) }
                .map(CStr::to_owned)
                .unwrap_or_default();
            unsafe { glib::g_error_free(err) };
            message
        });
        Fetched {
            response: (!resp.is_null()).then_some(Response(resp)),
            error,
        }
    }

    pub(crate) fn with_main_scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let ctx = unsafe { ns_js_main_context(self.0) };
        (!ctx.is_null()).then(|| unsafe { quickjs::with_context(ctx, f) })
    }
}

pub(crate) struct FrameUrl(*mut c_void);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(*mut JSContext);

impl Realm {
    pub(crate) fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        unsafe { quickjs::with_context(self.0, f) }
    }

    pub(crate) fn throw_range_error(self, message: &[u8]) {
        let message = c_string(message);
        unsafe { JS_ThrowRangeError(self.0, c"%s".as_ptr(), message.as_ptr()) };
    }

    pub(crate) fn throw_reference_error(self, message: &[u8]) {
        let message = c_string(message);
        unsafe { JS_ThrowReferenceError(self.0, c"%s".as_ptr(), message.as_ptr()) };
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
pub unsafe extern "C" fn ns_js_forget_script_roots_in(js: *mut NsJs, root: *const NsNode) {
    if !js.is_null() && !root.is_null() {
        page::forget_roots_in(Js(js), root as usize);
    }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_normalize(
    ctx: *mut JSContext,
    base: *const c_char,
    name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char {
    let Some(name) = (unsafe { cstr(name) }) else {
        return ptr::null_mut();
    };
    let base = unsafe { cstr(base) }.filter(|base| !base.is_empty());
    let resolved = import_map::resolve(Js(opaque.cast()), name.to_bytes())
        .or_else(|| url_resolve(base, name).map(CString::into_bytes))
        .unwrap_or_else(|| name.to_bytes().to_vec());
    let end = resolved
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(resolved.len());
    let resolved = CString::new(&resolved[..end]).unwrap_or_default();
    unsafe { js_strdup(ctx, resolved.as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_fetch(
    js: *mut NsJs,
    ctx: *mut JSContext,
    name: *const c_char,
    json: GBoolean,
    out_len: *mut usize,
) -> *mut c_char {
    let Some(name) = (unsafe { cstr(name) }) else {
        return ptr::null_mut();
    };
    match fetch::module_source(Js(js), Realm(ctx), name, json != 0) {
        Some(body) => {
            if !out_len.is_null() {
                unsafe { *out_len = body.len() };
            }
            glib::strdup(&body)
        }
        None => ptr::null_mut(),
    }
}

unsafe fn bytes<'a>(p: *const c_char, len: usize) -> &'a [u8] {
    if p.is_null() || len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(p.cast(), len) }
    }
}

unsafe fn name_bytes<'a>(p: *const c_char) -> &'a [u8] {
    unsafe { cstr(p) }.map_or(&[], CStr::to_bytes)
}

fn rethrow(scope: &mut Scope<'_>, error: Option<Value>) -> quickjs::JSValue {
    match error {
        Some(error) => quickjs::result_raw(scope, Err(error)),
        None => quickjs::EXCEPTION,
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
pub extern "C" fn ns_js_profile_begin() -> i64 {
    compile::profile_begin()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_profile_eval(
    start_us: i64,
    len: usize,
    cached: GBoolean,
    origin: *const c_char,
) {
    compile::profile_eval(start_us, len, cached != 0, unsafe { cstr(origin) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_profile_module(start_us: i64, len: usize, origin: *const c_char) {
    compile::profile_module(start_us, len, unsafe { cstr(origin) });
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
pub unsafe extern "C" fn ns_js_eval_script_source(
    js: *mut NsJs,
    script: *mut NsNode,
    source: *const c_char,
    length: usize,
    origin: *const c_char,
    is_module: GBoolean,
) {
    let source = unsafe { bytes(source, length) };
    let origin = unsafe { cstr(origin) };
    match node(script) {
        Some(script) => eval::eval_script_source(Js(js), script, source, origin, is_module != 0),
        None if is_module != 0 => eval::eval_module(Js(js), source, origin),
        None => {
            let previous = Js(js).swap_current_script(None);
            eval::eval(Js(js), source, origin);
            Js(js).swap_current_script(previous);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_compile_module_cached(
    ctx: *mut JSContext,
    src: *const c_char,
    len: usize,
    name: *const c_char,
) -> quickjs::JSValue {
    let src = unsafe { bytes(src, len) };
    let name = unsafe { name_bytes(name) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let module = eval::compile_module_cached(scope, src, name);
            quickjs::result_raw(scope, module)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_loader(
    ctx: *mut JSContext,
    name: *const c_char,
    opaque: *mut c_void,
    attributes: quickjs::JSValue,
) -> *mut c_void {
    let js = Js(opaque.cast());
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let attributes = quickjs::borrow_value(scope, attributes);
            let json = compile::attributes_type_is(scope, &attributes, b"json");
            let Some(name) = cstr(name) else {
                return ptr::null_mut();
            };
            match eval::load_module(js, Realm(ctx), scope, name, json) {
                None => ptr::null_mut(),
                Some(Ok(module)) => quickjs::module_def(module),
                Some(Err(exception)) => {
                    quickjs::result_raw(scope, Err(exception));
                    ptr::null_mut()
                }
            }
        })
    }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_bytecode_cache_lookup(
    ctx: *mut JSContext,
    src: *const c_char,
    len: usize,
) -> quickjs::JSValue {
    let src = unsafe { bytes(src, len) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            compile::cached_script(scope, src).map_or(quickjs::UNDEFINED, quickjs::into_raw)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_bytecode_cache_store(
    ctx: *mut JSContext,
    compiled: quickjs::JSValue,
    src: *const c_char,
    len: usize,
) {
    let src = unsafe { bytes(src, len) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let compiled = quickjs::borrow_value(scope, compiled);
            compile::store_script(scope, &compiled, src);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_bytecode_lookup(
    ctx: *mut JSContext,
    name: *const c_char,
    src: *const c_char,
    len: usize,
) -> quickjs::JSValue {
    let name = unsafe { name_bytes(name) };
    let src = unsafe { bytes(src, len) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            match compile::cached_module(scope, name, src) {
                None => quickjs::UNDEFINED,
                Some(Ok(module)) => quickjs::into_raw(module),
                Some(Err(error)) => rethrow(scope, error),
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_bytecode_store(
    ctx: *mut JSContext,
    module: quickjs::JSValue,
    name: *const c_char,
    src: *const c_char,
    len: usize,
) {
    let name = unsafe { name_bytes(name) };
    let src = unsafe { bytes(src, len) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let module = quickjs::borrow_value(scope, module);
            compile::store_module(scope, &module, name, src);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_set_import_meta(
    ctx: *mut JSContext,
    module: quickjs::JSValue,
    is_main: GBoolean,
) -> c_int {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let module = quickjs::borrow_value(scope, module);
            match compile::set_import_meta(scope, &module, is_main != 0) {
                Ok(()) => 0,
                Err(error) => {
                    rethrow(scope, error);
                    -1
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attrs_type_is(
    ctx: *mut JSContext,
    attributes: quickjs::JSValue,
    want: *const c_char,
) -> GBoolean {
    let want = unsafe { name_bytes(want) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let attributes = quickjs::borrow_value(scope, attributes);
            GBoolean::from(compile::attributes_type_is(scope, &attributes, want))
        })
    }
}

fn fill_element_perf_info(js: *mut NsJs, el: Option<Node>, info: &mut PerfInfo) {
    let frame = el.and_then(blocking::content_frame);
    let frame_ptr = Node::ptr_or_null(frame);
    let has_js = !js.is_null();
    let frame_ctx = if frame.is_some() && has_js {
        unsafe { ns_js_frame_context(js, frame_ptr) }
    } else {
        ptr::null_mut()
    };
    info.timeline = if frame_ctx.is_null() {
        frame_ptr.cast()
    } else {
        frame_ctx.cast_const().cast()
    };
    let frame_url = if frame.is_some() && has_js {
        unsafe { ns_js_frame_url(js, frame_ptr) }
    } else {
        ptr::null()
    };
    info.document_url = if !frame_url.is_null() {
        frame_url
    } else if has_js {
        unsafe { ns_js_page_url(js) }
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
