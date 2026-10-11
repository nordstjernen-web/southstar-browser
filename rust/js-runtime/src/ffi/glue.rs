//! Southstar — the engine, GLib and js.c calls the page's scheduling reaches: pending jobs, GLib sources, pumped fetches, byte views of buffers and the subsystem hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::helpers::ns_js_purge_subtree_rafs;
use super::page_object::{ns_active_js, ns_js_release_wrapper};
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::state::{JobFn, MessageTask};
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

    fn JS_GetPendingJobRealm(rt: *mut c_void) -> *mut JSContext;
    fn JS_ExecutePendingJob(rt: *mut c_void, pctx: *mut *mut JSContext) -> c_int;
    fn JS_IsRunningScript(ctx: *mut JSContext) -> bool;
    fn JS_RunGC(rt: *mut c_void);
    fn JS_EnqueueJob(ctx: *mut JSContext, func: JobFn, argc: c_int, argv: *mut JSValue) -> c_int;

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
    fn ns_blob_bytes_as_string(
        ctx: *mut JSContext,
        blob: JSValue,
        out_len: *mut usize,
    ) -> *mut c_char;
    fn ns_unwrap_element(val: JSValue) -> *const NsNode;
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_storage_drain_deferred_events(js: *mut NsJs);
    fn ns_js_report_pending_rejections(js: *mut NsJs);
    fn ns_storage_schedule_flush(js: *mut NsJs);
    fn ns_services_run_due_timers(js: *mut NsJs);
    fn ns_js_report_uncaught(js: *mut NsJs, ex: JSValue, origin: *const c_char);
    fn ns_target_report_exception(js: *mut NsJs, ctx: *mut JSContext, kind: *const c_char);
    fn ns_target_handler_realm(
        ctx: *mut JSContext,
        obj: JSValue,
        kind: *const c_char,
        listener_key: *const c_char,
    ) -> *mut JSContext;
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
    fn ns_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: *const c_char,
        event: JSValue,
        default_prevented: *mut GBoolean,
    );
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

pub(crate) fn pending_job_realm(js: Js) -> *mut JSContext {
    unsafe { JS_GetPendingJobRealm(js.rt()) }
}

pub(crate) fn execute_pending_job(js: Js, failed: &mut *mut JSContext) -> c_int {
    unsafe { JS_ExecutePendingJob(js.rt(), failed) }
}

pub(crate) fn is_running_script(ctx: *mut JSContext) -> bool {
    unsafe { JS_IsRunningScript(ctx) }
}

pub(crate) fn run_gc(js: Js) {
    unsafe { JS_RunGC(js.rt()) };
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

pub(crate) fn pending_exception_text(ctx: *mut JSContext) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let exception = quickjs::take_exception(scope);
            exception_text(scope, &exception)
        })
    }
}

pub(crate) fn borrowed_exception_text(
    ctx: *mut JSContext,
    exception: JSValue,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let exception = quickjs::borrow_value(scope, exception);
            exception_text(scope, &exception)
        })
    }
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

pub(crate) fn report_uncaught(js: Js, exception: JSValue, origin: *const c_char) {
    unsafe { ns_js_report_uncaught(js.ptr(), exception, origin) };
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
    event: JSValue,
) {
    unsafe { ns_js_dispatch_window_only_event(js.ptr(), doc, kind, event, ptr::null_mut()) };
}

pub(crate) fn active_js() -> Option<Js> {
    Js::of(unsafe { ns_active_js() })
}

pub(crate) fn unwrap_element(value: JSValue) -> *const NsNode {
    unsafe { ns_unwrap_element(value) }
}

pub(crate) fn is_detached_document(doc: *const NsNode) -> bool {
    let Some(node) = (unsafe { southstar_dom::Node::from_ptr(doc) }) else {
        return false;
    };
    node.kind() != southstar_dom::Kind::Document || node.flags() & NODE_FRAGMENT != 0
}

pub(crate) fn handler_realm(
    ctx: *mut JSContext,
    obj: JSValue,
    kind: *const c_char,
) -> *mut JSContext {
    unsafe { ns_target_handler_realm(ctx, obj, kind, c"fn".as_ptr()) }
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> *mut JSContext {
    let ctx = quickjs::raw_context(scope);
    if !scope.is_function(function) {
        return ctx;
    }
    quickjs::function_realm(scope, function).unwrap_or(ctx)
}

pub(crate) fn call_observer(
    js: Option<Js>,
    ctx: *mut JSContext,
    cb: JSValue,
    this_val: JSValue,
    args: &[JSValue],
    report_type: *const c_char,
) -> JSValue {
    let realm = unsafe {
        quickjs::with_context(ctx, |scope| {
            let cb = quickjs::borrow_value(scope, cb);
            function_realm(scope, &cb)
        })
    };
    let mut returned = super::scopes::in_realm(js, realm, || unsafe {
        quickjs::with_context(ctx, |scope| {
            let cb = quickjs::borrow_value(scope, cb);
            let this = quickjs::borrow_value(scope, this_val);
            let args: Vec<Value> = args
                .iter()
                .map(|&a| quickjs::borrow_value(scope, a))
                .collect();
            let result = scope.call(&cb, &this, &args);
            quickjs::result_raw(scope, result)
        })
    });
    if !report_type.is_null() && quickjs::raw_is_exception(returned) {
        if let Some(js) = js {
            unsafe { ns_target_report_exception(js.ptr(), ctx, report_type) };
        } else {
            unsafe { ns_target_report_exception(ptr::null_mut(), ctx, report_type) };
        }
        returned = quickjs::UNDEFINED;
    }
    returned
}

pub(crate) fn enqueue_job(ctx: *mut JSContext, func: JobFn, argc: c_int, argv: *mut JSValue) {
    unsafe { JS_EnqueueJob(ctx, func, argc, argv) };
}

pub(crate) fn message_task(ctx: *mut JSContext, func: JobFn, args: &[JSValue]) -> MessageTask {
    let args = unsafe {
        quickjs::with_context(ctx, |scope| {
            args.iter()
                .map(|&a| quickjs::borrow_value(scope, a))
                .collect()
        })
    };
    MessageTask { ctx, func, args }
}

pub(crate) fn run_message_task(task: MessageTask) {
    let mut raw: Vec<JSValue> = task.args.iter().map(quickjs::raw).collect();
    let result = unsafe { (task.func)(task.ctx, raw.len() as c_int, raw.as_mut_ptr()) };
    unsafe {
        quickjs::with_context(task.ctx, |scope| {
            let value = quickjs::take_value(scope, result);
            let _ = quickjs::checked(scope, value);
        })
    };
    drop(task);
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
        ns_js_purge_subtree_rafs(js.ptr(), root);
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
        unsafe { ns_js_release_wrapper(js.ptr(), node) };
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

pub(crate) unsafe fn fetch_resource(
    js: Option<Js>,
    url: *const c_char,
    top_url: *const c_char,
    headers: *const *const c_char,
    error: *mut *mut GError,
) -> *mut Response {
    let js = match js {
        Some(js) if js.st().worker_host.get().is_null() => js,
        _ => unsafe {
            return ns_net_request_blocking(
                url,
                top_url,
                c"GET".as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
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
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
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

pub(crate) struct BytesView {
    pub data: *const u8,
    pub len: usize,
    pub holder: Value,
}

pub(crate) fn bytes_view(scope: &mut Scope<'_>, value: &Value) -> Option<BytesView> {
    if quickjs::is_array_buffer(value) {
        let (data, len) = quickjs::array_buffer_data(scope, value)?;
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
    if !quickjs::is_array_buffer(&buffer) {
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
    let offset = quickjs::to_index(scope, &offset).ok()?;
    let length = quickjs::to_index(scope, &length).ok()?;
    let (base, total) = quickjs::array_buffer_data(scope, &buffer)?;
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

pub(crate) fn blob_bytes(ctx: *mut JSContext, value: JSValue, out_len: *mut usize) -> *mut c_char {
    unsafe { ns_blob_bytes_as_string(ctx, value, out_len) }
}

pub(crate) fn g_bytes(bytes: &[u8]) -> *mut c_char {
    let out = unsafe { glib::g_malloc(bytes.len() + 1) }.cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    out.cast()
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
