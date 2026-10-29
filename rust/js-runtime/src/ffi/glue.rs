//! Southstar — the engine, GLib and js.c calls the page's scheduling reaches: pending jobs, GLib sources, pumped fetches, byte views of buffers and the subsystem hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_char};
use core::ops::ControlFlow;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};

use southstar_glib::{MainContext, SourceId};
use southstar_js_engine::{ContextHandle, JobOutcome, ObjectKind, Scope, Value};

use super::Js;
use super::imports;
use super::state::MessageTask;
use crate::api::{self, ResourceInfo};

pub(crate) type Fetch = Result<southstar_net::Response, Vec<u8>>;

const NODE_FRAGMENT: u32 = 1 << 2;

pub(crate) fn context_of(js: Option<Js>) -> MainContext {
    js.and_then(|js| js.st().main_context.borrow().clone())
        .unwrap_or_else(MainContext::global)
}

pub(crate) fn attach_timeout(
    js: Option<Js>,
    ms: u32,
    func: impl FnMut() -> ControlFlow<()> + 'static,
) -> SourceId {
    context_of(js).timeout_add_local(ms, func)
}

pub(crate) fn attach_idle(js: Option<Js>, func: impl FnOnce() + 'static) -> SourceId {
    context_of(js).idle_add_local_once(func)
}

pub(crate) fn source_remove(js: Option<Js>, id: SourceId) {
    context_of(js).remove(id);
}

pub(crate) fn pump_iteration(js: Option<Js>) {
    context_of(js).iteration(true);
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
    southstar_net::in_blocking_fetch()
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

pub(crate) fn log_line(js: Js, line: &CStr) {
    if let Some(log) = js.st().log.get() {
        log(line);
    }
}

pub(crate) fn storage_drain_deferred_events(js: Js) {
    (api::storage_hooks().drain_deferred_events)(js.page());
}

pub(crate) fn report_pending_rejections(js: Js) {
    (api::dispatch_hooks().report_pending_rejections)(js.page());
}

pub(crate) fn storage_schedule_flush(js: Js) {
    (api::storage_hooks().schedule_flush)(js.page());
}

pub(crate) fn run_due_timers(js: Js) {
    (api::services_hooks().run_due_timers)(js.page());
}

pub(crate) fn report_uncaught(js: Js, exception: &Value, origin: Option<&CStr>) {
    let origin = origin.map(|origin| origin.to_string_lossy().into_owned());
    crate::events::report_uncaught(js.some(), exception, origin.as_deref());
}

pub(crate) fn observers_tick(js: Js) {
    let observers = api::observers_hooks();
    (observers.intersection_tick)(js.page());
    (observers.resize_tick)(js.page());
}

pub(crate) fn raf_set_host_driven(js: Js) {
    (api::lifecycle_hooks().raf_set_host_driven)(js.page());
}

pub(crate) fn blob_registry(js: Js, add: bool) {
    let url = api::url_hooks();
    if add {
        (url.blob_registry_add)(js.page());
    } else {
        (url.blob_registry_remove)(js.page());
    }
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    (api::window_hooks().note_viewport_scroll)(js.page(), x, y);
}

pub(crate) fn set_active_modal(modal: *const NsNode) {
    southstar_dom::tree::set_active_modal(super::record::node(modal));
}

pub(crate) fn finish_eval_depth(js: Js) {
    (api::document_hooks().flush_write)(js.page());
    (api::loader_hooks().schedule_pending_script_drain)(js.page());
}

pub(crate) fn dispatch_window_only_event(
    js: Js,
    doc: *const NsNode,
    kind: *const c_char,
    event: Value,
) {
    let kind = text(kind).map_or_else(String::new, |kind| kind.to_string_lossy().into_owned());
    crate::events::dispatch_window_only_event(js.some(), super::record::node(doc), &kind, event);
}

pub(crate) fn active_js() -> Option<Js> {
    Js::of(super::record::active())
}

pub(crate) fn unwrap_element(value: &Value) -> *const NsNode {
    southstar_dom::Node::ptr_or_null(super::wrapper::unwrap(value))
}

pub(crate) fn is_detached_document(doc: *const NsNode) -> bool {
    let Some(node) = super::record::node(doc) else {
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
            let page = js.map(Js::page);
            ctx.with(|scope| {
                crate::events::report_handler_exception(page, scope, Some(kind), &exception)
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

pub(crate) fn orphan_count(js: Js) -> usize {
    (api::mutation_hooks().orphan_count)(js.page())
}

pub(crate) fn collect_orphans(js: Js) -> Vec<Node<'static>> {
    (api::mutation_hooks().collect_orphans)(js.page())
}

pub(crate) fn free_orphan_subtree(js: Js, root: Node<'static>) {
    super::record::purge_subtree_rafs(js.some(), root.as_mut_ptr());
    (api::frames_hooks().purge_subtree_pending)(js.page(), root);
    (api::loader_hooks().purge_subtree_script_refs)(js.page(), root);
    (api::mutation_hooks().purge_subtree_node_refs)(js.page(), root);
    root.free_tree();
}

pub(crate) fn pending_iframe_count(js: Js) -> u32 {
    (api::frames_hooks().pending_count)(js.page())
}

pub(crate) fn pending_iframe_first(js: Js) -> Option<Node<'static>> {
    (api::frames_hooks().pending_first)(js.page())
}

pub(crate) fn load_first_pending_iframe(js: Js, iframe: Option<Node<'static>>) {
    let frames = api::frames_hooks();
    (frames.pending_remove_first)(js.page());
    if let Some(iframe) = iframe {
        (frames.load_now)(js.page(), iframe);
    }
}

pub(crate) fn unpin(js: Js, node: *mut NsNode) -> bool {
    if super::record::unpin_wrapper(js.some(), node) {
        if let (Some(record), Some(node)) =
            (super::record::record(js.some()), super::record::node(node))
        {
            super::record::release_wrapper(record, node);
        }
        return true;
    }
    false
}

pub(crate) fn fetch_resource(js: Option<Js>, req: &southstar_net::FetchRequest<'_>) -> Fetch {
    let js = match js {
        Some(js) if js.st().worker_host.borrow().is_none() => js,
        _ => {
            return southstar_net::request_blocking(req)
                .map_err(|failure| failure.message().to_vec());
        }
    };
    let main_loop = southstar_glib::MainLoop::new();
    let quit = main_loop.clone();
    let outcome: Rc<RefCell<Option<Fetch>>> = Rc::default();
    let delivered = Rc::clone(&outcome);
    southstar_net::request_with(req, None, move |result| {
        *delivered.borrow_mut() = Some(result);
        quit.quit();
    });
    let state = js.st();
    let saved = state.in_pump.replace(true);
    let start = super::monotonic_us();
    if outcome.borrow().is_none() {
        main_loop.run();
    }
    crate::budget::credit_pumped_time(js, start);
    state.in_pump.set(saved);
    outcome.take().unwrap_or_else(|| Err(Vec::new()))
}

pub(crate) fn add_resource_timing(
    js: Js,
    info: &ResourceInfo,
    url: &CStr,
    initiator: &CStr,
    start_us: i64,
    response: Option<&southstar_net::Response>,
) {
    let span = (start_us, super::monotonic_us());
    let add = crate::api::perf_hooks().add_resource_timed;
    add(js.page(), Some(info), url, Some(initiator), span, response);
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

pub(crate) fn names_table(names: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut seen = std::collections::HashSet::new();
    names
        .iter()
        .map(|name| c_line(name).into_bytes())
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

pub(crate) fn text<'a>(raw: *const c_char) -> Option<&'a CStr> {
    (!raw.is_null()).then(|| unsafe { CStr::from_ptr(raw) })
}
