//! Southstar — the C ABI of the page's scheduling, realm scopes and runtime state as declared in src/js.h and src/js_internal.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GHashTable};
use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use super::glue::{self, Response, SourceFn};
use super::scopes;
use super::state::{
    DataFn, DelayFn, DownloadFn, FormSubmitFn, JobFn, NavigateFn, NodeFn, PageState, TextQueryFn,
    UrlFn, ViewportScrollFn,
};
use super::{GMainContext, Js, NsJs, WorkerHost};
use crate::scheduling::{self, RunTasks};
use crate::{budget, orphans};

const TRANSIENT_ACTIVATION_US: i64 = 5_000_000;
const BODY_BYTES_MAX: usize = 32 * 1024 * 1024;
const SOURCE_REMOVE: GBoolean = 0;
const SOURCE_CONTINUE: GBoolean = 1;

fn flag(value: bool) -> GBoolean {
    GBoolean::from(value)
}

fn with<R>(js: *const NsJs, fallback: R, f: impl FnOnce(Js) -> R) -> R {
    Js::of(js).map_or(fallback, f)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_note_user_activation(js: *mut NsJs) {
    with(js, (), |js| {
        js.st().user_activation_us.set(super::monotonic_us());
        js.st().user_ever_activated.set(true);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean {
    with(js, 0, |js| {
        let at = js.st().user_activation_us.get();
        flag(at != 0 && super::monotonic_us() - at <= TRANSIENT_ACTIVATION_US)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_user_activation_state(
    js: *mut NsJs,
    ever_activated: *mut GBoolean,
) -> GBoolean {
    let ever = with(js, false, |js| js.st().user_ever_activated.get());
    unsafe { *ever_activated = flag(ever) };
    unsafe { ns_js_has_transient_activation(js) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_consume_user_activation(js: *mut NsJs) {
    with(js, (), |js| js.st().user_activation_us.set(0));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clipboard_write(js: *mut NsJs, text: *const c_char) -> c_int {
    with(js, -1, |js| match js.st().clipboard_write.get() {
        Some((write, data)) => c_int::from(unsafe { write(text, data) } != 0),
        None => -1,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_copyable(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| {
        flag(js.st().selection_has_range.get() && js.st().clipboard_write.is_set())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_copy_selection(js: *mut NsJs) -> GBoolean {
    if unsafe { ns_js_selection_copyable(js) } == 0 {
        return 0;
    }
    let js = Js(js);
    let Some((write, data)) = js.st().clipboard_write.get() else {
        return 0;
    };
    let text = js.st().selection_text.get();
    let text = if text.is_null() {
        c"".as_ptr()
    } else {
        text.cast_const()
    };
    unsafe { write(text, data) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_selection_commands(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.st().selection_cmd.is_set()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_command(js: *mut NsJs, cmd: *const c_char) -> GBoolean {
    with(js, 0, |js| match js.st().selection_cmd.get() {
        Some((command, data)) => flag(unsafe { command(cmd, data) } != 0),
        None => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_eval_budget_us() -> i64 {
    budget::eval_budget_us()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_pump(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.st().in_pump.get()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_credit_pumped_time(js: *mut NsJs, pump_start_us: i64) {
    with(js, (), |js| budget::credit_pumped_time(js, pump_start_us));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fetch_resource(
    js: *mut NsJs,
    url: *const c_char,
    top_url: *const c_char,
    headers: *const *const c_char,
    error: *mut *mut GError,
) -> *mut Response {
    unsafe { glue::fetch_resource(Js::of(js), url, top_url, headers, error) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fetch_subresource(
    js: *mut NsJs,
    url: *const c_char,
    top_url: *const c_char,
    headers: *const *const c_char,
    error: *mut *mut GError,
    initiator: *const c_char,
    info: *const c_void,
) -> *mut Response {
    let start = super::monotonic_us();
    let response = unsafe { ns_js_fetch_resource(js, url, top_url, headers, error) };
    if let Some(js) = Js::of(js)
        && js.st().worker_host.get().is_null()
        && !initiator.is_null()
    {
        glue::add_resource_timing(js, info, url, initiator, start, response);
    }
    response
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_document_url(
    js: *mut NsJs,
    realm: *mut JSContext,
) -> *const c_char {
    unsafe extern "C" {
        fn ns_js_frame_url(js: *const NsJs, frame: *const NsNode) -> *const c_char;
    }
    with(js, ptr::null(), |js| {
        if !realm.is_null() && realm != js.main_realm() {
            let url = unsafe { ns_js_frame_url(js.ptr(), scopes::frame_of_realm(js, realm)) };
            if !url.is_null() {
                return url;
            }
        }
        js.current_url_raw()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_budget_enter(js: *mut NsJs) -> i64 {
    with(js, 0, budget::push)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_budget_leave(js: *mut NsJs, saved: i64) {
    with(js, (), |js| budget::pop(js, saved));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_context_frame(js: *mut NsJs, ctx: *mut JSContext) -> *mut NsNode {
    with(js, ptr::null_mut(), |js| {
        if ctx.is_null() || ctx == js.main_realm() {
            return ptr::null_mut();
        }
        let frame = scopes::frame_of_realm(js, ctx);
        if frame.is_null() { js.frame() } else { frame }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_bytes_view(
    ctx: *mut JSContext,
    value: JSValue,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_holder: *mut JSValue,
) -> GBoolean {
    unsafe {
        if !out_data.is_null() {
            *out_data = ptr::null();
        }
        if !out_len.is_null() {
            *out_len = 0;
        }
        if !out_holder.is_null() {
            *out_holder = quickjs::UNDEFINED;
        }
    }
    let view = unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            glue::bytes_view(scope, &value)
        })
    };
    let Some(view) = view else {
        return 0;
    };
    unsafe {
        if !out_data.is_null() {
            *out_data = view.data;
        }
        if !out_len.is_null() {
            *out_len = view.len;
        }
        if !out_holder.is_null() {
            *out_holder = quickjs::into_raw(view.holder);
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_body_bytes(
    ctx: *mut JSContext,
    value: JSValue,
    out_len: *mut usize,
) -> *mut c_char {
    if !out_len.is_null() {
        unsafe { *out_len = 0 };
    }
    enum Body {
        Bytes(Vec<u8>),
        Blob,
        Nothing,
    }
    let body = unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            if value.is_undefined() || value.is_null() {
                return Body::Nothing;
            }
            if value.is_string() {
                return scope.to_bytes(&value).map_or(Body::Nothing, Body::Bytes);
            }
            if let Some(view) = glue::bytes_view(scope, &value) {
                return Body::Bytes(glue::bytes_of(&view).to_vec());
            }
            if value.is_object() {
                return match scope.get(&value, "__ndBlobBytes") {
                    Ok(bytes) if !bytes.is_undefined() && !bytes.is_null() => Body::Blob,
                    _ => Body::Nothing,
                };
            }
            Body::Nothing
        })
    };
    match body {
        Body::Bytes(bytes) => {
            if bytes.len() > BODY_BYTES_MAX {
                return ptr::null_mut();
            }
            if !out_len.is_null() {
                unsafe { *out_len = bytes.len() };
            }
            glue::g_bytes(&bytes)
        }
        Body::Blob => glue::blob_bytes(ctx, value, out_len),
        Body::Nothing => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_value_is_url_search_params(
    ctx: *mut JSContext,
    value: JSValue,
) -> GBoolean {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            if !value.is_object() || scope.is_function(&value) {
                return 0;
            }
            let pairs = scope
                .get(&value, "__ndPairs")
                .unwrap_or_else(|_| Value::undefined());
            if !pairs.is_array() {
                return 0;
            }
            let append = scope
                .get(&value, "append")
                .unwrap_or_else(|_| Value::undefined());
            let get_all = scope
                .get(&value, "getAll")
                .unwrap_or_else(|_| Value::undefined());
            flag(scope.is_function(&append) && scope.is_function(&get_all))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_usp_serialize(
    ctx: *mut JSContext,
    usp: JSValue,
    out_len: *mut usize,
    out_content_type: *mut *mut c_char,
) -> *mut c_char {
    if !out_content_type.is_null() {
        unsafe {
            *out_content_type =
                glib::g_strdup(c"application/x-www-form-urlencoded;charset=UTF-8".as_ptr())
        };
    }
    let text = unsafe {
        quickjs::with_context(ctx, |scope| {
            let usp = quickjs::borrow_value(scope, usp);
            let to_string = scope
                .get(&usp, "toString")
                .unwrap_or_else(|_| Value::undefined());
            let text = if scope.is_function(&to_string) {
                scope.call(&to_string, &usp, &[])
            } else {
                Ok(scope.string(""))
            };
            text.map(|text| scope.to_bytes(&text).unwrap_or_default())
        })
    };
    let bytes = text.unwrap_or_default();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    if !out_len.is_null() {
        unsafe { *out_len = end };
    }
    glue::g_bytes(&bytes[..end])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_promise_reject(
    ctx: *mut JSContext,
    resolvers: *mut JSValue,
    message: *const c_char,
) {
    let message = glue::text(message).map_or(&b""[..], |m| m.to_bytes());
    let name_len = message
        .iter()
        .position(|&b| b == b':')
        .unwrap_or(message.len());
    let colon = name_len < message.len();
    let is_dom_name = name_len > 5
        && name_len < 64
        && message[name_len - 5..name_len] == *b"Error"
        && message[..name_len].iter().all(u8::is_ascii_alphanumeric);
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let resolve = quickjs::take_value(scope, *resolvers);
            let reject = quickjs::take_value(scope, *resolvers.add(1));
            let error = scope.new_error();
            let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
            if is_dom_name {
                let name = scope.string(&lossy(&message[..name_len]));
                let _ = scope.set(&error, "name", name);
                let mut rest = if colon {
                    &message[name_len + 1..]
                } else {
                    message
                };
                while let Some((b' ', tail)) = rest.split_first() {
                    rest = tail;
                }
                let text = scope.string(&lossy(rest));
                let _ = scope.set(&error, "message", text);
            } else {
                let text = scope.string(&lossy(message));
                let _ = scope.set(&error, "message", text);
            }
            let _ = scope.call(&reject, &Value::undefined(), &[error]);
            drop(resolve);
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_source_remove(js: *mut NsJs, id: c_uint) {
    glue::source_remove(Js::of(js), id);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_timeout(
    js: *mut NsJs,
    ms: c_uint,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    glue::attach_timeout(Js::of(js), ms, func, data)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_idle(
    js: *mut NsJs,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    glue::attach_idle(Js::of(js), func, data)
}

unsafe extern "C" fn run_message_tasks(data: *mut c_void) -> GBoolean {
    let js = Js(data.cast());
    match scheduling::run_message_tasks(js) {
        RunTasks::Done => SOURCE_REMOVE,
        RunTasks::Continue => SOURCE_CONTINUE,
        RunTasks::Retry => {
            let source = glue::attach_timeout(Some(js), 4, run_message_tasks, data);
            js.st().message_task_source.set(source);
            SOURCE_REMOVE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drop_message_tasks(js: *mut NsJs) {
    with(js, (), scheduling::drop_message_tasks);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_queue_message_task(
    ctx: *mut JSContext,
    func: JobFn,
    argc: c_int,
    argv: *mut JSValue,
) {
    let js = Js::of_context(ctx);
    let Some(js) = js.filter(|_| argc <= 3) else {
        glue::enqueue_job(ctx, func, argc, argv);
        return;
    };
    let args = if argc > 0 && !argv.is_null() {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    } else {
        &[]
    };
    let task = glue::message_task(ctx, func, args);
    if scheduling::queue_message_task(js, task) {
        let source = glue::attach_timeout(Some(js), 0, run_message_tasks, js.ptr().cast());
        js.st().message_task_source.set(source);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_message_tasks_pending(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(!js.st().message_tasks.borrow().is_empty()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_top_url(js: *mut NsJs) -> *const c_char {
    scopes::top_url(Js(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_top_url(js: *mut NsJs, url: *const c_char) {
    scopes::set_top_url(Js(js), url);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean {
    with(js, 0, |js| {
        let top = scopes::top_document(js.current_doc());
        flag(!top.is_null() && !node.is_null() && scopes::top_document(node.cast_mut()) == top)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_url(js: *mut NsJs, realm: *mut JSContext) -> *const c_char {
    with(js, ptr::null(), |js| scopes::realm_url(js, realm))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_microtask_checkpoint(js: *mut NsJs) {
    with(js, (), scheduling::microtask_checkpoint);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_drain_microtasks(js: *mut NsJs) {
    with(js, (), scheduling::drain_microtasks);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_due_timers_allowed(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(scheduling::due_timers_allowed(js)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_drain_mutations(js: *mut NsJs) {
    scheduling::drain_mutations(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_gate(
    js: *mut NsJs,
    frame: *mut NsNode,
    idle_expired: GBoolean,
) -> c_int {
    scheduling::timer_gate(Js(js), frame, idle_expired != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_enter(
    js: *mut NsJs,
    ctx: *mut JSContext,
    frame: *mut NsNode,
) -> *mut c_void {
    scopes::timer_scope_enter(Js(js), ctx, frame)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_context(scope: *const c_void) -> *mut JSContext {
    unsafe { scopes::timer_scope_context(scope) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_leave(
    scope: *mut c_void,
    threw: GBoolean,
    exception: JSValue,
) {
    let mut scope = unsafe { scopes::timer_scope_take(scope) };
    let js = scope.js();
    let ctx = scope.ctx();
    scheduling::add_depth(&js.st().callback_depth, -1);
    budget::pop(js, scope.budget());
    if threw != 0 {
        let (message, stack) = glue::borrowed_exception_text(ctx, exception);
        if let Some(message) = message
            && js.st().log.is_set()
        {
            let mut line = b"JS error in timer: ".to_vec();
            line.extend_from_slice(&message);
            if let Some(stack) = stack {
                line.push(b'\n');
                line.extend_from_slice(&stack);
            }
            let line = glue::c_line(&line);
            glue::log_line(js, line.as_ptr());
        }
        glue::report_uncaught(js, exception, js.current_url_raw());
    }
    scheduling::drain_mutations(js);
    scope.leave();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_timer_this_is_detached_window(
    js: *mut NsJs,
    ctx: *mut JSContext,
    this_val: JSValue,
) -> GBoolean {
    let Some(js) = Js::of(js) else {
        return 0;
    };
    if !quickjs::raw_is_object(this_val) {
        return 0;
    }
    let doc = unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            match scope.get(&this, "document") {
                Ok(doc) => glue::unwrap_element(quickjs::raw(&doc)),
                Err(_) => ptr::null(),
            }
        })
    };
    if doc.is_null() || doc == js.current_doc().cast_const() || glue::is_detached_document(doc) {
        return 0;
    }
    let top = scopes::top_document(js.current_doc());
    flag(!top.is_null() && scopes::top_document(doc.cast_mut()) != top)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_worker_host(js: *const NsJs) -> *mut WorkerHost {
    with(js, ptr::null_mut(), |js| js.st().worker_host.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_halt(js: *mut NsJs) {
    with(js, (), Js::halt);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_halted(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.halted()))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_js_call_observer(
    js: *mut NsJs,
    ctx: *mut JSContext,
    cb: JSValue,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    report_type: *const c_char,
    fresh_budget: GBoolean,
) -> JSValue {
    let js = Js::of(js);
    let saved = js.map_or(0, |js| js.st().eval_deadline_us.get());
    if let Some(js) = js
        && fresh_budget != 0
    {
        js.st()
            .eval_deadline_us
            .set(super::monotonic_us() + budget::eval_budget_us());
    }
    let args = if argc > 0 && !argv.is_null() {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    } else {
        &[]
    };
    let result = glue::call_observer(js, ctx, cb, this_val, args, report_type);
    if let Some(js) = js
        && fresh_budget != 0
    {
        js.st().eval_deadline_us.set(saved);
    }
    result
}

unsafe extern "C" fn observer_tick(data: *mut c_void) -> GBoolean {
    let Some(js) = Js::of(data.cast()) else {
        return SOURCE_REMOVE;
    };
    if scheduling::observer_tick(js) {
        SOURCE_CONTINUE
    } else {
        SOURCE_REMOVE
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_observer_schedule_tick(js: *mut NsJs) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let state = js.st();
    if js.ctx().is_null()
        || !state.worker_host.get().is_null()
        || state.observer_tick_source.get() != 0
    {
        return;
    }
    state
        .observer_tick_source
        .set(glue::timeout_add(4, observer_tick, js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_observer_tick_pending(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().observer_tick_source.get() != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_finish(js: *mut NsJs) {
    scheduling::dispatch_finish(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_scope_enter(
    js: *mut NsJs,
    ctx: *mut JSContext,
    frame: *mut NsNode,
) -> *mut c_void {
    scopes::raf_scope_enter(Js(js), ctx, frame)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_scope_context(scope: *const c_void) -> *mut JSContext {
    unsafe { scopes::raf_scope_js(scope) }.ctx()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_scope_leave(scope: *mut c_void) {
    let js = unsafe { scopes::raf_scope_js(scope) };
    scheduling::drain_microtasks(js);
    unsafe { scopes::raf_scope_finish(scope) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_callback_depth_add(js: *mut NsJs, delta: c_int) {
    scheduling::add_depth(&Js(js).st().callback_depth, delta);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mutated(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().mutated.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mark_mutated(js: *mut NsJs) {
    with(js, (), |js| js.st().mutated.set(true));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_consume_mutated(js: *mut NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.st().mutated.replace(false)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_forget_frame_context(js: *mut NsJs, frame: *const NsNode) {
    let js = Js(js);
    if js.frame().cast_const() == frame {
        js.set_frame(ptr::null_mut());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_snapshot_globals(js: *mut NsJs) -> *mut GHashTable {
    let names = Js(js)
        .scope(|scope| {
            let global = scope.global();
            let keys = scope.own_property_keys(&global, false).unwrap_or_default();
            keys.iter()
                .filter_map(|key| scope.to_bytes(key).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    glue::names_table(names)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_style_table(js: *mut NsJs, styles: *mut GHashTable) {
    with(js, (), |js| js.st().style_table.set(styles));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable {
    with(js, ptr::null_mut(), |js| js.st().style_table.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_anim(js: *mut NsJs, anim: *mut c_void) {
    with(js, (), |js| {
        js.st().anim.set(anim);
        glue::raf_set_host_driven(js);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_anim(js: *const NsJs) -> *mut c_void {
    with(js, ptr::null_mut(), |js| js.st().anim.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_image_cache(js: *mut NsJs, cache: *mut c_void) {
    with(js, (), |js| {
        js.st().image_cache.set(cache);
        glue::blob_registry(js, !cache.is_null());
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_cache(js: *const NsJs) -> *mut c_void {
    Js(js.cast_mut()).st().image_cache.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_repaint(js: *mut NsJs) {
    with(js, (), |js| {
        if let Some((repaint, data)) = js.st().repaint.get() {
            unsafe { repaint(data) };
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_notify_scroll_to(js: *mut NsJs, target: *const NsNode) {
    with(js, (), |js| {
        if let Some((scroll, data)) = js.st().scroll_to.get() {
            unsafe { scroll(target, data) };
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_window_action(js: *mut NsJs, action: *const c_char) {
    with(js, (), |js| {
        if let Some((window_action, data)) = js.st().window_action.get() {
            unsafe { window_action(action, data) };
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_window_action(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.st().window_action.is_set()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_scroll_viewport(js: *mut NsJs, x: f64, y: f64) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let mut x = if !x.is_finite() || x < 0.0 { 0.0 } else { x };
    let mut y = if !y.is_finite() || y < 0.0 { 0.0 } else { y };
    if let Some((scroll, data)) = js.st().viewport_scroll.get() {
        unsafe { scroll(&mut x, &mut y, data) };
    }
    glue::note_viewport_scroll(js, x, y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_sweep_orphans(js: *mut NsJs) {
    with(js, (), orphans::sweep);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_unpin_subtree(js: *mut NsJs, root: *mut NsNode) {
    let (Some(js), Some(root)) = (Js::of(js), unsafe { Node::from_ptr(root) }) else {
        return;
    };
    orphans::unpin_subtree(js, root);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_process_pending_iframes(js: *mut NsJs) {
    with(js, (), orphans::process_pending_iframes);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_load_depth(js: *const NsJs) -> c_int {
    Js(js.cast_mut()).st().iframe_load_depth.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_frame_load(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().iframe_load_depth.get() > 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_load_delayed(js: *mut NsJs) -> GBoolean {
    match Js(js).st().load_delay.get() {
        Some((delayed, data)) => flag(unsafe { delayed(data) } != 0),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_ready_state(js: *mut NsJs, state: c_int) {
    Js(js).st().ready_state.set(state);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ready_state(js: *const NsJs) -> c_int {
    with(js, 0, |js| js.st().ready_state.get())
}

macro_rules! hook_setter {
    ($name:ident, $field:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(js: *mut NsJs, cb: Option<$ty>, user_data: *mut c_void) {
            with(js, (), |js| js.st().$field.set(cb, user_data));
        }
    };
}

hook_setter!(ns_js_set_form_submit_cb, form_submit, FormSubmitFn);
hook_setter!(ns_js_set_download_cb, download, DownloadFn);
hook_setter!(ns_js_set_clipboard_write_cb, clipboard_write, TextQueryFn);
hook_setter!(ns_js_set_selection_cmd_cb, selection_cmd, TextQueryFn);
hook_setter!(ns_js_set_scroll_to_cb, scroll_to, NodeFn);
hook_setter!(ns_js_set_fragment_nav_cb, fragment_nav, UrlFn);
hook_setter!(ns_js_set_soft_nav_cb, soft_nav, NavigateFn);
hook_setter!(ns_js_set_window_action_cb, window_action, UrlFn);
hook_setter!(
    ns_js_set_viewport_scroll_cb,
    viewport_scroll,
    ViewportScrollFn
);
hook_setter!(ns_js_set_layout_flush_cb, layout_flush, DataFn);
hook_setter!(ns_js_set_load_delay_cb, load_delay, DelayFn);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_selection(
    js: *mut NsJs,
    text: *const c_char,
    has_range: GBoolean,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let state = js.st();
    let text = if text.is_null() { c"".as_ptr() } else { text };
    PageState::replace_owned(&state.selection_text, unsafe { glib::g_strdup(text) });
    state.selection_has_range.set(has_range != 0);
    state.selection_rect.set([x, y, w, h]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_state(
    js: *const NsJs,
    text: *mut *const c_char,
    rect: *mut f64,
) -> GBoolean {
    let state = Js(js.cast_mut()).st();
    unsafe {
        *text = state.selection_text.get();
        for (i, value) in state.selection_rect.get().into_iter().enumerate() {
            *rect.add(i) = value;
        }
    }
    flag(state.selection_has_range.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_early_inject_src(js: *mut NsJs, src: *const c_char) {
    with(js, (), |js| {
        PageState::replace_owned(&js.st().early_inject_src, unsafe { glib::g_strdup(src) });
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_early_inject_src(js: *const NsJs) -> *const c_char {
    Js(js.cast_mut()).st().early_inject_src.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wpt_hooks_enabled(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(!js.st().early_inject_src.get().is_null()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_layout(js: *mut NsJs) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let state = js.st();
    let Some((flush, data)) = state.layout_flush.get() else {
        return;
    };
    if state.in_layout_flush.get() {
        return;
    }
    state.in_layout_flush.set(true);
    unsafe { flush(data) };
    state.in_layout_flush.set(false);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_style(js: *mut NsJs) {
    unsafe { ns_js_flush_layout(js) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_url(js: *const NsJs) -> *const c_char {
    Js(js.cast_mut()).current_url_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_url(js: *const NsJs) -> *const c_char {
    let url = with(js, ptr::null_mut(), Js::current_url_raw);
    if url.is_null() { c"".as_ptr() } else { url }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_page_url(js: *const NsJs) -> *const c_char {
    with(js, ptr::null_mut(), Js::current_url_raw).cast_const()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_current_url(js: *mut NsJs, url: *const c_char) {
    let js = Js(js);
    let copy = unsafe { glib::g_strdup(url) };
    unsafe { glib::g_free(js.current_url_raw().cast()) };
    js.set_current_url_raw(copy);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_soft_navigate(js: *mut NsJs, url: *const c_char, replace: GBoolean) {
    if let Some((navigate, data)) = Js(js).st().soft_nav.get() {
        unsafe { navigate(url, replace, data) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_download(
    js: *mut NsJs,
    url: *const c_char,
    filename: *const c_char,
) -> GBoolean {
    match Js(js).st().download.get() {
        Some((download, data)) => {
            unsafe { download(url, filename, data) };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_navigate(
    js: *mut NsJs,
    url: *const c_char,
    reload: GBoolean,
) -> GBoolean {
    match Js(js).st().navigate.get() {
        Some((navigate, data)) => {
            unsafe { navigate(url, reload, data) };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_can_navigate(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().navigate.is_set())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fragment_navigated(js: *mut NsJs, url: *const c_char) {
    if let Some((navigated, data)) = Js(js).st().fragment_nav.get() {
        unsafe { navigated(url, data) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_submit_form(
    js: *mut NsJs,
    form: *const NsNode,
    submitter: *const NsNode,
) {
    if let Some((submit, data)) = Js(js).st().form_submit.get() {
        unsafe { submit(form, submitter, data) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm_context(js: *const NsJs) -> *mut JSContext {
    Js(js.cast_mut()).main_realm()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm(js: *const NsJs) -> *mut JSContext {
    Js(js.cast_mut()).main_realm()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_context(js: *const NsJs) -> *mut JSContext {
    Js(js.cast_mut()).ctx()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pattern_context() -> *mut JSContext {
    glue::active_js().map_or(ptr::null_mut(), |js| {
        let main = js.main_realm();
        if main.is_null() { js.ctx() } else { main }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_document_origin(js: *const NsJs) -> *const c_char {
    Js(js.cast_mut()).st().document_origin.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_document(js: *mut NsJs, doc: *mut NsNode, origin: *mut c_char) {
    let js = Js(js);
    js.set_current_doc(doc);
    js.set_ce_main_doc(doc);
    let state = js.st();
    state.autofocus_processed.set(false);
    state.active_modal.set(ptr::null());
    PageState::replace_owned(&state.document_origin, origin);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_url_enter(
    js: *mut NsJs,
    realm: *mut JSContext,
) -> *mut c_void {
    with(js, ptr::null_mut(), |js| scopes::realm_url_enter(js, realm))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_url_leave(js: *mut NsJs, token: *mut c_void) {
    unsafe { scopes::frame_url_pop(Js(js), token) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_url_push(js: *mut NsJs, url: *const c_char) -> *mut c_void {
    scopes::frame_url_push(Js(js), url)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_url_pop(js: *mut NsJs, token: *mut c_void) {
    unsafe { scopes::frame_url_pop(Js(js), token) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_window_events_blocked(js: *const NsJs) -> GBoolean {
    let js = Js(js.cast_mut());
    flag(js.current_doc().is_null() || js.halted() || js.st().in_pump.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_events_suspended(js: *const NsJs) -> GBoolean {
    let js = Js(js.cast_mut());
    flag(js.halted() || js.st().in_pump.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_document_window_event(
    js: *mut NsJs,
    kind: *const c_char,
    event: JSValue,
) {
    let js = Js(js);
    glue::dispatch_window_only_event(js, js.current_doc(), kind, event);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_main_realm(
    js: *mut NsJs,
    f: unsafe extern "C" fn(data: *mut c_void),
    data: *mut c_void,
) {
    let js = Js::of(js);
    let realm = js.map_or(ptr::null_mut(), Js::main_realm);
    scopes::in_realm(js, realm, || unsafe { f(data) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_glib_context(js: *const NsJs) -> *mut GMainContext {
    with(js, ptr::null_mut(), |js| js.st().main_context.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_is_worker(js: *const NsJs) -> GBoolean {
    flag(!Js(js.cast_mut()).st().worker_host.get().is_null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_autofocus_processed(js: *const NsJs) -> GBoolean {
    with(js, 0, |js| flag(js.st().autofocus_processed.get()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_autofocus_processed(js: *mut NsJs) {
    with(js, (), |js| js.st().autofocus_processed.set(true));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_referrer(_js: *const NsJs) -> *const c_char {
    ptr::null()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ignore_destructive_writes(js: *const NsJs) -> c_int {
    with(js, 0, |js| js.st().ignore_destructive_writes.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ignore_destructive_writes_add(js: *mut NsJs, delta: c_int) {
    scheduling::add_depth(&Js(js).st().ignore_destructive_writes, delta);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_active_modal(js: *const NsJs) -> *const NsNode {
    with(js, ptr::null(), |js| js.st().active_modal.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_active_modal(js: *mut NsJs, modal: *const NsNode) {
    with(js, (), |js| {
        js.st().active_modal.set(modal);
        glue::set_active_modal(modal);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_doc_ready_state(
    js: *mut NsJs,
    doc: *const NsNode,
    state: c_int,
) {
    let Some(js) = Js::of(js) else {
        return;
    };
    if doc.is_null() {
        return;
    }
    js.st()
        .doc_ready_states
        .borrow_mut()
        .insert(doc as usize, state);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_doc_ready_state(js: *const NsJs, doc: *const NsNode) -> c_int {
    let states = Js(js.cast_mut()).st().doc_ready_states.borrow();
    states.get(&(doc as usize)).copied().unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clear_doc_ready_states(js: *mut NsJs) {
    Js(js).st().doc_ready_states.borrow_mut().clear();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_script(js: *const NsJs) -> *const NsNode {
    Js(js.cast_mut()).st().current_script.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_swap_current_script(
    js: *mut NsJs,
    script: *mut NsNode,
) -> *mut NsNode {
    Js(js).st().current_script.replace(script)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_swap_current_document(
    js: *mut NsJs,
    doc: *mut NsNode,
) -> *mut NsNode {
    let js = Js(js);
    let previous = js.current_doc();
    js.set_current_doc(doc);
    previous
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_document(js: *const NsJs) -> *const NsNode {
    Js(js.cast_mut()).current_doc()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_realm(js: *mut NsJs, ctx: *mut JSContext, doc: *mut NsNode) {
    let js = Js(js);
    js.set_ctx(ctx);
    js.set_current_doc(doc);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_context(js: *const NsJs) -> *mut JSContext {
    let js = Js(js.cast_mut());
    let module = js.st().module_ctx.get();
    if module.is_null() { js.ctx() } else { module }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_swap_module_context(
    js: *mut NsJs,
    ctx: *mut JSContext,
) -> *mut JSContext {
    Js(js).st().module_ctx.replace(ctx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_document(js: *const NsJs, out: *mut JSValue) -> GBoolean {
    match Js(js.cast_mut()).st().iframe_doc.get() {
        Some(doc) => {
            unsafe { *out = doc };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_document_node(js: *const NsJs) -> *const NsNode {
    match Js(js.cast_mut()).st().iframe_doc.get() {
        Some(doc) => glue::unwrap_element(doc),
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_enabled(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().log.is_set())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_log(js: *const NsJs) -> GBoolean {
    flag(Js(js.cast_mut()).st().log.is_set())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_line(js: *mut NsJs, line: *const c_char) {
    glue::log_line(Js(js), line);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_depth(js: *const NsJs) -> c_int {
    with(js, 0, |js| js.st().eval_depth.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_depth_add(js: *mut NsJs, delta: c_int) -> c_int {
    scheduling::add_depth(&Js(js).st().eval_depth, delta)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_callback_depth(js: *const NsJs) -> c_int {
    with(js, 0, |js| js.st().callback_depth.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_depth(js: *const NsJs) -> c_int {
    with(js, 0, |js| js.st().dispatch_depth.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_depth_add(js: *mut NsJs, delta: c_int) {
    scheduling::add_depth(&Js(js).st().dispatch_depth, delta);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_eval_begin(
    js: *mut NsJs,
    script: *mut NsNode,
    deadline_us: i64,
) -> *mut NsNode {
    let state = Js(js).st();
    let previous = state.current_script.replace(script);
    state.eval_deadline_us.set(deadline_us);
    scheduling::add_depth(&state.eval_depth, 1);
    previous
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_eval_end(js: *mut NsJs, previous_script: *mut NsNode) {
    let js = Js(js);
    let state = js.st();
    let depth = scheduling::add_depth(&state.eval_depth, -1);
    state.eval_deadline_us.set(0);
    state.current_script.set(previous_script);
    if depth == 0 {
        glue::finish_eval_depth(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_enter_handler_realm(
    ctx: *mut JSContext,
    obj: JSValue,
    kind: *const c_char,
) -> *mut c_void {
    let scope = scopes::realm_scope_new();
    let realm = glue::handler_realm(ctx, obj, kind);
    unsafe { scopes::realm_scope_push(Js::of_context(ctx), realm, scope) };
    scope
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_leave_handler_realm(ctx: *mut JSContext, scope: *mut c_void) {
    unsafe {
        scopes::realm_scope_pop(Js::of_context(ctx), scope);
        scopes::realm_scope_free(scope);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_scope_push(
    js: *mut NsJs,
    realm: *mut JSContext,
    buf: *mut c_void,
) {
    unsafe { scopes::realm_scope_push(Js::of(js), realm, buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_scope_pop(js: *mut NsJs, buf: *mut c_void) {
    unsafe { scopes::realm_scope_pop(Js::of(js), buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_scope_push(
    js: *mut NsJs,
    realm: *mut JSContext,
    frame: *mut NsNode,
    buf: *mut c_void,
) {
    unsafe { scopes::frame_scope_push(Js(js), realm, frame, buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_load_enter(
    js: *mut NsJs,
    frame: *mut NsNode,
    doc: *mut NsNode,
    url: *const c_char,
    realm_doc: JSValue,
) -> *mut c_void {
    let realm_doc = quickjs::raw_is_object(realm_doc).then_some(realm_doc);
    scopes::frame_load_enter(Js(js), frame, doc, url, realm_doc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_load_leave(js: *mut NsJs, token: *mut c_void) {
    unsafe { scopes::frame_load_leave(Js(js), token) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_scope_enter(
    js: *mut NsJs,
    ctx: *mut JSContext,
    doc: *mut NsNode,
    frame: *mut NsNode,
    buf: *mut c_void,
) {
    unsafe { scopes::dispatch_scope_enter(Js(js), ctx, doc, frame, buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_scope_leave(js: *mut NsJs, buf: *mut c_void) {
    unsafe { scopes::dispatch_scope_leave(Js(js), buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_pump_iteration(js: *mut NsJs) -> GBoolean {
    let js = Js::of(js);
    if js.is_some_and(Js::halted) {
        return 0;
    }
    glue::pump_iteration(js);
    1
}
