//! Southstar — the engine, GLib and js.c calls the page's scheduling reaches: pending jobs, GLib sources, pumped fetches, byte views of buffers and the subsystem hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GPtrArray};
use southstar_js_engine::{ContextHandle, JobOutcome, ObjectKind, Scope, Value};

use super::imports;
use super::state::MessageTask;
use super::{GMainContext, Js, NsJs};

pub(crate) type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;
type ReadyFn = unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, data: *mut c_void);

#[repr(C)]
pub(crate) struct GSource {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct Response {
    _private: [u8; 0],
}

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

const NODE_FRAGMENT: u32 = 1 << 2;

unsafe extern "C" {
    fn g_timeout_source_new(interval: c_uint) -> *mut GSource;
    fn g_idle_source_new() -> *mut GSource;
    fn g_source_set_callback(
        source: *mut GSource,
        func: SourceFn,
        data: *mut c_void,
        notify: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn g_source_attach(source: *mut GSource, context: *mut GMainContext) -> c_uint;
    fn g_source_unref(source: *mut GSource);
    fn g_source_destroy(source: *mut GSource);
    fn g_main_context_find_source_by_id(context: *mut GMainContext, id: c_uint) -> *mut GSource;
    fn g_main_context_default() -> *mut GMainContext;
    fn g_main_context_iteration(context: *mut GMainContext, may_block: GBoolean) -> GBoolean;
    fn g_timeout_add(interval: c_uint, func: SourceFn, data: *mut c_void) -> c_uint;
    fn g_main_loop_new(context: *mut GMainContext, running: GBoolean) -> *mut GMainLoop;
    fn g_main_loop_run(lp: *mut GMainLoop);
    fn g_main_loop_quit(lp: *mut GMainLoop);
    fn g_main_loop_unref(lp: *mut GMainLoop);
    fn g_error_free(error: *mut GError);

    fn ns_net_request_blocking(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        headers: *const *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut Response;
    fn ns_net_request_async(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        headers: *const *const c_char,
        cancellable: *mut c_void,
        callback: ReadyFn,
        data: *mut c_void,
    );
    fn ns_net_fetch_finish(result: *mut c_void, error: *mut *mut GError) -> *mut Response;
    fn ns_perf_add_resource_timed(
        js: *mut NsJs,
        info: *const c_void,
        url: *const c_char,
        initiator: *const c_char,
        start_us: i64,
        end_us: i64,
        resp: *const Response,
    );
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_storage_drain_deferred_events(js: *mut NsJs);
    fn ns_js_report_pending_rejections(js: *mut NsJs);
    fn ns_storage_schedule_flush(js: *mut NsJs);
    fn ns_services_run_due_timers(js: *mut NsJs);
    fn ns_intersection_observers_tick(js: *mut NsJs);
    fn ns_resize_observers_tick(js: *mut NsJs);
    fn ns_js_raf_set_host_driven(js: *mut NsJs);
    fn ns_js_blob_registry_add(js: *mut NsJs);
    fn ns_js_blob_registry_remove(js: *mut NsJs);
    fn ns_js_note_viewport_scroll(js: *mut NsJs, x: f64, y: f64);
    fn ns_js_orphan_count(js: *mut NsJs) -> c_int;
    fn ns_js_orphans_collect(js: *mut NsJs) -> *mut GPtrArray;
    fn ns_js_purge_subtree_pending_iframes(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_purge_subtree_script_refs(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_purge_subtree_node_refs(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_free(node: *mut NsNode);
    fn ns_js_pending_iframe_count(js: *const NsJs) -> c_uint;
    fn ns_js_pending_iframe_first(js: *const NsJs) -> *mut NsNode;
    fn ns_js_pending_iframe_remove_first(js: *mut NsJs);
    fn ns_js_load_iframe_now(js: *mut NsJs, iframe: *mut NsNode);
    fn ns_dom_set_active_modal(modal: *const NsNode);
    fn ns_js_flush_document_write(js: *mut NsJs);
    fn ns_js_schedule_pending_script_drain(js: *mut NsJs);
}

fn context_of(js: Option<Js>) -> *mut GMainContext {
    js.map_or(ptr::null_mut(), |js| js.st().main_context.get())
}

pub(crate) fn attach_timeout(
    js: Option<Js>,
    ms: c_uint,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    unsafe {
        let source = g_timeout_source_new(ms);
        g_source_set_callback(source, func, data, None);
        let id = g_source_attach(source, context_of(js));
        g_source_unref(source);
        id
    }
}

pub(crate) fn attach_idle(js: Option<Js>, func: SourceFn, data: *mut c_void) -> c_uint {
    unsafe {
        let source = g_idle_source_new();
        g_source_set_callback(source, func, data, None);
        let id = g_source_attach(source, context_of(js));
        g_source_unref(source);
        id
    }
}

pub(crate) fn source_remove(js: Option<Js>, id: c_uint) {
    if id == 0 {
        return;
    }
    let mut context = context_of(js);
    if context.is_null() {
        context = unsafe { g_main_context_default() };
    }
    let source = unsafe { g_main_context_find_source_by_id(context, id) };
    if !source.is_null() {
        unsafe { g_source_destroy(source) };
    }
}

pub(crate) fn timeout_add(ms: c_uint, func: SourceFn, js: Js) -> c_uint {
    unsafe { g_timeout_add(ms, func, js.ptr().cast()) }
}

pub(crate) fn pump_iteration(js: Option<Js>) {
    unsafe { g_main_context_iteration(context_of(js), glib::TRUE) };
}

pub(crate) fn pending_job_realm(js: Js) -> ContextHandle {
    js.rt().pending_job_context()
}

pub(crate) fn execute_pending_job(js: Js) -> JobOutcome {
    js.rt().run_pending_job()
}

pub(crate) fn is_running_script(ctx: ContextHandle) -> bool {
    ctx.with(|scope| scope.is_running_script()).unwrap_or(false)
}

pub(crate) fn run_gc(js: Js) {
    js.rt().gc();
}

pub(crate) fn engine_in_blocking_fetch() -> bool {
    unsafe { ns_engine_in_blocking_fetch() != 0 }
}

pub(crate) fn exception_text(
    scope: &mut Scope<'_>,
    exception: &Value,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let message = scope.to_bytes(exception).ok();
    let stack = match scope.get(exception, "stack") {
        Ok(stack) if !stack.is_undefined() => scope.to_bytes(&stack).ok(),
        _ => None,
    };
    (message, stack)
}

pub(crate) fn exception_text_in(
    ctx: ContextHandle,
    exception: &Value,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    ctx.with(|scope| exception_text(scope, exception))
        .unwrap_or((None, None))
}

pub(crate) fn c_line(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn log_line(js: Js, line: *const c_char) {
    if let Some((log, data)) = js.st().log.get() {
        unsafe { log(line, data) };
    }
}

pub(crate) fn storage_drain_deferred_events(js: Js) {
    unsafe { ns_storage_drain_deferred_events(js.ptr()) };
}

pub(crate) fn report_pending_rejections(js: Js) {
    unsafe { ns_js_report_pending_rejections(js.ptr()) };
}

pub(crate) fn storage_schedule_flush(js: Js) {
    unsafe { ns_storage_schedule_flush(js.ptr()) };
}

pub(crate) fn run_due_timers(js: Js) {
    unsafe { ns_services_run_due_timers(js.ptr()) };
}

pub(crate) fn report_uncaught(js: Js, exception: &Value, origin: *const c_char) {
    let origin = text(origin).map(|origin| origin.to_string_lossy().into_owned());
    unsafe { imports::nsr_js_report_uncaught(js.ptr(), exception, origin.as_deref()) };
}

pub(crate) fn observers_tick(js: Js) {
    unsafe {
        ns_intersection_observers_tick(js.ptr());
        ns_resize_observers_tick(js.ptr());
    }
}

pub(crate) fn raf_set_host_driven(js: Js) {
    unsafe { ns_js_raf_set_host_driven(js.ptr()) };
}

pub(crate) fn blob_registry(js: Js, add: bool) {
    unsafe {
        if add {
            ns_js_blob_registry_add(js.ptr());
        } else {
            ns_js_blob_registry_remove(js.ptr());
        }
    }
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    unsafe { ns_js_note_viewport_scroll(js.ptr(), x, y) };
}

pub(crate) fn set_active_modal(modal: *const NsNode) {
    unsafe { ns_dom_set_active_modal(modal) };
}

pub(crate) fn finish_eval_depth(js: Js) {
    unsafe {
        ns_js_flush_document_write(js.ptr());
        ns_js_schedule_pending_script_drain(js.ptr());
    }
}

pub(crate) fn dispatch_window_only_event(
    js: Js,
    doc: *const NsNode,
    kind: *const c_char,
    event: Value,
) {
    let kind = text(kind).map_or_else(String::new, |kind| kind.to_string_lossy().into_owned());
    unsafe { imports::nsr_js_dispatch_window_only_event(js.ptr(), doc, &kind, event) };
}

pub(crate) fn active_js() -> Option<Js> {
    Js::of(super::record::ns_active_js())
}

pub(crate) fn unwrap_element(value: &Value) -> *const NsNode {
    southstar_dom::Node::ptr_or_null(super::wrapper::unwrap(value))
}

pub(crate) fn is_detached_document(doc: *const NsNode) -> bool {
    let Some(node) = (unsafe { southstar_dom::Node::from_ptr(doc) }) else {
        return false;
    };
    node.kind() != southstar_dom::Kind::Document || node.flags() & NODE_FRAGMENT != 0
}

pub(crate) fn handler_realm(scope: &mut Scope<'_>, obj: &Value, kind: &str) -> ContextHandle {
    imports::nsr_target_handler_realm(scope, obj, kind, "fn")
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> ContextHandle {
    let ctx = scope.handle();
    if !scope.is_function(function) {
        return ctx;
    }
    scope.function_handle(function).unwrap_or(ctx)
}

pub(crate) fn call_observer(
    js: Option<Js>,
    ctx: ContextHandle,
    cb: &Value,
    this: &Value,
    args: &[Value],
    report_type: Option<&str>,
) -> Result<Value, Value> {
    let realm = ctx.with(|scope| function_realm(scope, cb)).unwrap_or(ctx);
    let returned =
        super::scopes::in_realm(js, realm, || ctx.with(|scope| scope.call(cb, this, args)));
    let returned = returned.unwrap_or_else(|| Ok(Value::undefined()));
    match (returned, report_type) {
        (Err(exception), Some(kind)) => {
            let page = js.map_or(ptr::null_mut(), Js::ptr);
            ctx.with(|scope| unsafe {
                imports::nsr_target_report_exception(page, scope, Some(kind), &exception)
            });
            Ok(Value::undefined())
        }
        (returned, _) => returned,
    }
}

pub(crate) fn message_task(ctx: ContextHandle, run: super::state::TaskRun) -> MessageTask {
    MessageTask { ctx, run }
}

pub(crate) fn run_message_task(task: MessageTask) {
    let MessageTask { ctx, run } = task;
    let _ = ctx.with(|scope| drop(run(scope)));
}

pub(crate) fn orphan_count(js: Js) -> c_int {
    unsafe { ns_js_orphan_count(js.ptr()) }
}

pub(crate) fn collect_orphans(js: Js) -> Vec<*mut NsNode> {
    let array = unsafe { ns_js_orphans_collect(js.ptr()) };
    if array.is_null() {
        return Vec::new();
    }
    let len = unsafe { (*array).len } as usize;
    let nodes = (0..len)
        .map(|i| unsafe { *(*array).pdata.add(i) }.cast::<NsNode>())
        .collect();
    unsafe { glib::g_ptr_array_free(array, glib::TRUE) };
    nodes
}

pub(crate) fn free_orphan_subtree(js: Js, root: *mut NsNode) {
    unsafe {
        super::record::ns_js_purge_subtree_rafs(js.ptr(), root);
        ns_js_purge_subtree_pending_iframes(js.ptr(), root);
        ns_js_purge_subtree_script_refs(js.ptr(), root);
        ns_js_purge_subtree_node_refs(js.ptr(), root);
        ns_node_free(root);
    }
}

pub(crate) fn pending_iframe_count(js: Js) -> c_uint {
    unsafe { ns_js_pending_iframe_count(js.ptr()) }
}

pub(crate) fn pending_iframe_first(js: Js) -> *mut NsNode {
    unsafe { ns_js_pending_iframe_first(js.ptr()) }
}

pub(crate) fn load_first_pending_iframe(js: Js, iframe: *mut NsNode) {
    unsafe {
        ns_js_pending_iframe_remove_first(js.ptr());
        ns_js_load_iframe_now(js.ptr(), iframe);
    }
}

pub(crate) fn unpin(js: Js, node: *mut NsNode) -> bool {
    let set = js.pinned_set();
    if unsafe { glib::g_hash_table_remove(set, node.cast()) } != 0 {
        unsafe { super::record::release_wrapper(js.ptr(), node) };
        return true;
    }
    false
}

struct PumpedFetch {
    main_loop: *mut GMainLoop,
    response: *mut Response,
    error: *mut GError,
    done: bool,
}

unsafe extern "C" fn pumped_fetch_done(
    _source: *mut c_void,
    result: *mut c_void,
    data: *mut c_void,
) {
    let fetch = unsafe { &mut *data.cast::<PumpedFetch>() };
    fetch.response = unsafe { ns_net_fetch_finish(result, &mut fetch.error) };
    fetch.done = true;
    if !fetch.main_loop.is_null() {
        unsafe { g_main_loop_quit(fetch.main_loop) };
    }
}

pub(crate) struct RequestBody {
    pub method: *const c_char,
    pub data: *const c_void,
    pub len: usize,
    pub content_type: *const c_char,
}

impl RequestBody {
    pub(crate) const GET: RequestBody = RequestBody {
        method: c"GET".as_ptr(),
        data: ptr::null(),
        len: 0,
        content_type: ptr::null(),
    };
}

pub(crate) unsafe fn fetch_resource(
    js: Option<Js>,
    url: *const c_char,
    top_url: *const c_char,
    headers: *const *const c_char,
    body: &RequestBody,
    error: *mut *mut GError,
) -> *mut Response {
    let js = match js {
        Some(js) if js.st().worker_host.get().is_null() => js,
        _ => unsafe {
            return ns_net_request_blocking(
                url,
                top_url,
                body.method,
                body.data,
                body.len,
                body.content_type,
                headers,
                ptr::null_mut(),
                error,
            );
        },
    };
    let mut fetch = Box::new(PumpedFetch {
        main_loop: unsafe { g_main_loop_new(ptr::null_mut(), glib::FALSE) },
        response: ptr::null_mut(),
        error: ptr::null_mut(),
        done: false,
    });
    let data: *mut PumpedFetch = &mut *fetch;
    unsafe {
        ns_net_request_async(
            url,
            top_url,
            body.method,
            body.data,
            body.len,
            body.content_type,
            headers,
            ptr::null_mut(),
            pumped_fetch_done,
            data.cast(),
        )
    };
    let state = js.st();
    let saved = state.in_pump.replace(true);
    let start = super::monotonic_us();
    if !unsafe { (*data).done } {
        unsafe { g_main_loop_run((*data).main_loop) };
    }
    crate::budget::credit_pumped_time(js, start);
    state.in_pump.set(saved);
    unsafe { g_main_loop_unref((*data).main_loop) };
    let fetched_error = unsafe { (*data).error };
    if !fetched_error.is_null() {
        if error.is_null() {
            unsafe { g_error_free(fetched_error) };
        } else {
            unsafe { *error = fetched_error };
        }
    }
    unsafe { (*data).response }
}

pub(crate) fn add_resource_timing(
    js: Js,
    info: *const c_void,
    url: *const c_char,
    initiator: *const c_char,
    start_us: i64,
    response: *const Response,
) {
    unsafe {
        ns_perf_add_resource_timed(
            js.ptr(),
            info,
            url,
            initiator,
            start_us,
            super::monotonic_us(),
            response,
        )
    };
}

pub struct BytesView {
    pub data: *const u8,
    pub len: usize,
    pub holder: Value,
}

pub(crate) fn bytes_view(scope: &mut Scope<'_>, value: &Value) -> Option<BytesView> {
    if scope.object_kind(value) == Some(ObjectKind::ArrayBuffer) {
        let (data, len) = scope.array_buffer_ptr(value)?;
        return Some(BytesView {
            data,
            len,
            holder: value.clone(),
        });
    }
    if !value.is_object() {
        return None;
    }
    let buffer = scope.get(value, "buffer").ok()?;
    if scope.object_kind(&buffer) != Some(ObjectKind::ArrayBuffer) {
        return None;
    }
    let offset = scope.get(value, "byteOffset");
    let length = scope.get(value, "byteLength");
    let (Ok(offset), Ok(length)) = (offset, length) else {
        return None;
    };
    if !offset.is_number() || !length.is_number() {
        return None;
    }
    let offset = scope.to_index(&offset).ok()?;
    let length = scope.to_index(&length).ok()?;
    let (base, total) = scope.array_buffer_ptr(&buffer)?;
    let total = total as u64;
    if offset > total || length > total - offset {
        return None;
    }
    Some(BytesView {
        data: unsafe { base.add(offset as usize) },
        len: length as usize,
        holder: buffer,
    })
}

pub(crate) fn bytes_of(view: &BytesView) -> &[u8] {
    if view.data.is_null() || view.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(view.data, view.len) }
    }
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    imports::nsr_blob_bytes(scope, value)
}

pub(crate) fn names_table(names: Vec<Vec<u8>>) -> *mut GHashTable {
    let table = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            None,
        )
    };
    for name in names {
        let key = c_line(&name);
        unsafe { glib::g_hash_table_add(table, glib::g_strdup(key.as_ptr()).cast()) };
    }
    table
}

pub(crate) fn text<'a>(raw: *const c_char) -> Option<&'a CStr> {
    (!raw.is_null()).then(|| unsafe { CStr::from_ptr(raw) })
}
