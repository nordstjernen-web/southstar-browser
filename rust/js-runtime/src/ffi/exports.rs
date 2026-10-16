//! Southstar — the C ABI of the page's scheduling, realm scopes and runtime state, over the typed API binding crates call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_engine::ContextHandle;

use super::glue::{self, SourceFn};
use super::record;
use super::scopes;
use super::state::{
    DataFn, DelayFn, DownloadFn, FormSubmitFn, NavigateFn, NodeFn, TextQueryFn, UrlFn,
    ViewportScrollFn,
};
use super::{GMainContext, Js, NsJs, WorkerHost};
use crate::api::{self, TimerGate};
use crate::scheduling::{self, RunTasks};

const SOURCE_REMOVE: GBoolean = 0;
const SOURCE_CONTINUE: GBoolean = 1;

fn flag(value: bool) -> GBoolean {
    GBoolean::from(value)
}

fn text<'a>(raw: *const c_char) -> Option<&'a CStr> {
    glue::text(raw)
}

fn node<'a>(raw: *const NsNode) -> Option<Node<'a>> {
    record::node(raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_note_user_activation(js: *mut NsJs) {
    api::note_user_activation(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean {
    flag(api::has_transient_activation(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_user_activation_state(
    js: *mut NsJs,
    ever_activated: *mut GBoolean,
) -> GBoolean {
    let (transient, ever) = api::user_activation_state(js);
    unsafe { *ever_activated = flag(ever) };
    flag(transient)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_consume_user_activation(js: *mut NsJs) {
    api::consume_user_activation(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clipboard_write(js: *mut NsJs, text_ptr: *const c_char) -> c_int {
    match text(text_ptr).and_then(|text| api::clipboard_write(js, text)) {
        Some(written) => c_int::from(written),
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_copyable(js: *const NsJs) -> GBoolean {
    flag(api::selection_copyable(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_copy_selection(js: *mut NsJs) -> GBoolean {
    flag(api::copy_selection(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_selection_commands(js: *const NsJs) -> GBoolean {
    flag(api::has_selection_commands(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_command(js: *mut NsJs, cmd: *const c_char) -> GBoolean {
    flag(text(cmd).is_some_and(|cmd| api::selection_command(js, cmd)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_pump(js: *const NsJs) -> GBoolean {
    flag(api::in_pump(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_credit_pumped_time(js: *mut NsJs, pump_start_us: i64) {
    api::credit_pumped_time(js, pump_start_us);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_document_url(
    js: *mut NsJs,
    realm: ContextHandle,
) -> *const c_char {
    api::realm_document_url_ptr(js, realm)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_budget_enter(js: *mut NsJs) -> i64 {
    api::budget_enter(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_budget_leave(js: *mut NsJs, saved: i64) {
    api::budget_leave(js, saved);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_context_frame(js: *mut NsJs, ctx: ContextHandle) -> *mut NsNode {
    Node::ptr_or_null(api::context_frame(js, ctx)).cast_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_source_remove(js: *mut NsJs, id: c_uint) {
    api::source_remove(js, id);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_timeout(
    js: *mut NsJs,
    ms: c_uint,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    api::attach_timeout(js, ms, func, data)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_idle(
    js: *mut NsJs,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    api::attach_idle(js, func, data)
}

pub(crate) unsafe extern "C" fn run_message_tasks(data: *mut c_void) -> GBoolean {
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
    api::drop_message_tasks(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_message_tasks_pending(js: *const NsJs) -> GBoolean {
    flag(api::message_tasks_pending(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_in_page(js: *mut NsJs, n: *const NsNode) -> GBoolean {
    flag(api::node_in_page(js, node(n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_url(js: *mut NsJs, realm: ContextHandle) -> *const c_char {
    Js::of(js).map_or(ptr::null(), |js| scopes::realm_url(js, realm))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_microtask_checkpoint(js: *mut NsJs) {
    api::microtask_checkpoint(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_drain_microtasks(js: *mut NsJs) {
    api::drain_microtasks(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_due_timers_allowed(js: *const NsJs) -> GBoolean {
    flag(api::due_timers_allowed(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_drain_mutations(js: *mut NsJs) {
    api::drain_mutations(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_gate(
    js: *mut NsJs,
    frame: *mut NsNode,
    idle_expired: GBoolean,
) -> c_int {
    match api::timer_gate(js, node(frame), idle_expired != 0) {
        TimerGate::Run => scheduling::TIMER_RUN,
        TimerGate::Wait => scheduling::TIMER_WAIT,
        TimerGate::Drop => scheduling::TIMER_DROP,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_enter(
    js: *mut NsJs,
    ctx: ContextHandle,
    frame: *mut NsNode,
) -> *mut c_void {
    scopes::timer_scope_enter(Js(js), ctx, frame)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_context(scope: *const c_void) -> ContextHandle {
    unsafe { scopes::timer_scope_context(scope) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_worker_host(js: *const NsJs) -> *mut WorkerHost {
    api::worker_host(js).cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_halt(js: *mut NsJs) {
    api::halt(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_halted(js: *const NsJs) -> GBoolean {
    flag(api::halted(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_observer_schedule_tick(js: *mut NsJs) {
    api::observer_schedule_tick(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_observer_tick_pending(js: *const NsJs) -> GBoolean {
    flag(api::observer_tick_pending(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_finish(js: *mut NsJs) {
    api::dispatch_finish(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_scope_enter(
    js: *mut NsJs,
    ctx: ContextHandle,
    frame: *mut NsNode,
) -> *mut c_void {
    scopes::raf_scope_enter(Js(js), ctx, frame)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_scope_context(scope: *const c_void) -> ContextHandle {
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
    api::callback_depth_add(js, delta);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mutated(js: *const NsJs) -> GBoolean {
    flag(api::mutated(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mark_mutated(js: *mut NsJs) {
    api::mark_mutated(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_consume_mutated(js: *mut NsJs) -> GBoolean {
    flag(api::consume_mutated(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_forget_frame_context(js: *mut NsJs, frame: *const NsNode) {
    let js = Js(js);
    if js.frame().cast_const() == frame {
        js.set_frame(ptr::null_mut());
    }
}

pub(crate) fn snapshot_globals(js: Js) -> *mut GHashTable {
    let names = js
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
    if let Some(js) = Js::of(js) {
        js.st().style_table.set(styles);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable {
    api::style_table(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_anim(js: *mut NsJs, anim: *mut c_void) {
    if let Some(js) = Js::of(js) {
        js.st().anim.set(anim);
        glue::raf_set_host_driven(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_anim(js: *const NsJs) -> *mut c_void {
    Js::of(js).map_or(ptr::null_mut(), |js| js.st().anim.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_image_cache(js: *mut NsJs, cache: *mut c_void) {
    if let Some(js) = Js::of(js) {
        js.st().image_cache.set(cache);
        glue::blob_registry(js, !cache.is_null());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_cache(js: *const NsJs) -> *mut c_void {
    api::image_cache(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_repaint(js: *mut NsJs) {
    api::request_repaint(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_notify_scroll_to(js: *mut NsJs, target: *const NsNode) {
    if let Some(target) = node(target) {
        api::notify_scroll_to(js, target);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_window_action(js: *mut NsJs, action: *const c_char) {
    if let Some(action) = text(action) {
        api::window_action(js, action);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_has_window_action(js: *const NsJs) -> GBoolean {
    flag(api::has_window_action(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_scroll_viewport(js: *mut NsJs, x: f64, y: f64) {
    api::scroll_viewport(js, x, y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_process_pending_iframes(js: *mut NsJs) {
    api::process_pending_iframes(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_load_depth(js: *const NsJs) -> c_int {
    api::iframe_load_depth(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_frame_load(js: *const NsJs) -> GBoolean {
    flag(api::in_frame_load(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_load_delayed(js: *mut NsJs) -> GBoolean {
    flag(api::load_delayed(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_ready_state(js: *mut NsJs, state: c_int) {
    api::set_ready_state(js, state);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ready_state(js: *const NsJs) -> c_int {
    api::ready_state(js)
}

macro_rules! hook_setter {
    ($name:ident, $field:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(js: *mut NsJs, cb: Option<$ty>, user_data: *mut c_void) {
            if let Some(js) = Js::of(js) {
                js.st().$field.set(cb, user_data);
            }
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
    selection: *const c_char,
    has_range: GBoolean,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    api::set_selection(js, text(selection), has_range != 0, [x, y, w, h]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_state(
    js: *const NsJs,
    text_out: *mut *const c_char,
    rect: *mut f64,
) -> GBoolean {
    let state = Js(js.cast_mut()).st();
    unsafe {
        *text_out = state.selection_text.get();
        for (i, value) in state.selection_rect.get().into_iter().enumerate() {
            *rect.add(i) = value;
        }
    }
    flag(state.selection_has_range.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_early_inject_src(js: *mut NsJs, src: *const c_char) {
    if let Some(js) = Js::of(js) {
        let src = text(src).map_or(ptr::null_mut(), |src| glib::strdup(src.to_bytes()));
        super::state::PageState::replace_owned(&js.st().early_inject_src, src);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_early_inject_src(js: *const NsJs) -> *const c_char {
    Js(js.cast_mut()).st().early_inject_src.get()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wpt_hooks_enabled(js: *const NsJs) -> GBoolean {
    flag(api::wpt_hooks_enabled(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_layout(js: *mut NsJs) {
    api::flush_layout(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_style(js: *mut NsJs) {
    api::flush_layout(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_url(js: *const NsJs) -> *const c_char {
    let url = api::page_url_ptr(js);
    if url.is_null() { c"".as_ptr() } else { url }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_page_url(js: *const NsJs) -> *const c_char {
    api::page_url_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_current_url(js: *mut NsJs, url: *const c_char) {
    api::set_current_url(js, text(url));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_download(
    js: *mut NsJs,
    url: *const c_char,
    filename: *const c_char,
) -> GBoolean {
    flag(text(url).is_some_and(|url| api::request_download(js, url, text(filename))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_navigate(
    js: *mut NsJs,
    url: *const c_char,
    reload: GBoolean,
) -> GBoolean {
    flag(text(url).is_some_and(|url| api::navigate(js, url, reload != 0)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_submit_form(
    js: *mut NsJs,
    form: *const NsNode,
    submitter: *const NsNode,
) {
    if let Some(form) = node(form) {
        api::submit_form(js, form, node(submitter));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm_context(js: *const NsJs) -> ContextHandle {
    api::main_realm(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm(js: *const NsJs) -> ContextHandle {
    api::main_realm(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_context(js: *const NsJs) -> ContextHandle {
    api::main_context(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pattern_context() -> ContextHandle {
    api::pattern_context()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_document(js: *mut NsJs, doc: *mut NsNode, origin: *mut c_char) {
    api::set_document(js, node(doc), text(origin));
    unsafe { glib::g_free(origin.cast()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_events_suspended(js: *const NsJs) -> GBoolean {
    flag(api::events_suspended(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_main_realm(
    js: *mut NsJs,
    f: unsafe extern "C" fn(data: *mut c_void),
    data: *mut c_void,
) {
    api::in_main_realm(js, || unsafe { f(data) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_glib_context(js: *const NsJs) -> *mut GMainContext {
    api::glib_context(js).cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_is_worker(js: *const NsJs) -> GBoolean {
    flag(api::is_worker(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_autofocus_processed(js: *const NsJs) -> GBoolean {
    flag(api::autofocus_processed(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_autofocus_processed(js: *mut NsJs) {
    api::set_autofocus_processed(js);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_referrer(_js: *const NsJs) -> *const c_char {
    ptr::null()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ignore_destructive_writes(js: *const NsJs) -> c_int {
    api::ignore_destructive_writes(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_active_modal(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::active_modal(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_active_modal(js: *mut NsJs, modal: *const NsNode) {
    api::set_active_modal(js, node(modal));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_doc_ready_state(js: *const NsJs, doc: *const NsNode) -> c_int {
    node(doc).map_or(-1, |doc| api::doc_ready_state(js, doc))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clear_doc_ready_states(js: *mut NsJs) {
    api::clear_doc_ready_states(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_script(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::current_script(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_document(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::current_document(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_realm(js: *mut NsJs, ctx: ContextHandle, doc: *mut NsNode) {
    api::set_realm(js, ctx, node(doc));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_enabled(js: *const NsJs) -> GBoolean {
    flag(api::log_enabled(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_line(js: *mut NsJs, line: *const c_char) {
    if let Some(line) = text(line) {
        api::log_line(js, line);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_depth(js: *const NsJs) -> c_int {
    api::eval_depth(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_callback_depth(js: *const NsJs) -> c_int {
    api::callback_depth(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_depth(js: *const NsJs) -> c_int {
    api::dispatch_depth(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_depth_add(js: *mut NsJs, delta: c_int) {
    api::dispatch_depth_add(js, delta);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_scope_push(
    js: *mut NsJs,
    realm: ContextHandle,
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
    realm: ContextHandle,
    frame: *mut NsNode,
    buf: *mut c_void,
) {
    unsafe { scopes::frame_scope_push(Js(js), realm, frame, buf) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_scope_enter(
    js: *mut NsJs,
    ctx: ContextHandle,
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
    flag(api::pump_iteration(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_load_for(js: *mut NsJs, new_url: *const c_char) {
    api::storage_load_for(js, text(new_url));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_img_src_layout_neutral(n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(api::img_src_layout_neutral))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_src_recorded(
    js: *mut NsJs,
    n: *mut NsNode,
    s: *const c_char,
    slen: usize,
) {
    if let Some(el) = node(n) {
        api::set_src_recorded(js, el, unsafe { glib::slice(s.cast(), slen) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_in_template_content(n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(api::in_template_content))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_media_native_formats() -> c_uint {
    api::media_native_formats()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean {
    flag(text(url).is_some_and(|url| api::url_parses(js, url)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_allows_worker(js: *mut NsJs, url: *const c_char) -> GBoolean {
    flag(text(url).is_none_or(|url| api::csp_allows_worker(js, url)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_rafs(js: *mut NsJs, root: *mut NsNode) {
    if let Some(root) = node(root) {
        api::purge_subtree_rafs(js, root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_insert_before_single(
    js: *mut NsJs,
    parent: *mut NsNode,
    newc: *mut NsNode,
    reference: *mut NsNode,
) {
    if let (Some(_), Some(child), Some(reference)) = (node(parent), node(newc), node(reference)) {
        api::insert_before_single(js, child, reference);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_layout_root(js: *mut NsJs, root: *const c_void) {
    api::set_layout_root(js, root);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_disabled_form_control(el: *const NsNode) -> GBoolean {
    flag(node(el).is_some_and(api::is_disabled_form_control))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_reset(js: *mut NsJs, form: *mut NsNode) {
    if let Some(form) = node(form) {
        api::form_reset(js, form);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_load_realm(js: *mut NsJs) -> ContextHandle {
    api::image_load_realm(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char {
    if let Some(url) = node(el).and_then(record::node_frame_url) {
        return url.as_ptr();
    }
    api::page_url_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::ce_main_doc(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wrapper_pinned(js: *const NsJs, n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(|n| api::wrapper_pinned(js, n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attr_changed(
    js: *mut NsJs,
    n: *mut NsNode,
    attr_name: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    if let (Some(el), Some(name)) = (node(n), text(attr_name)) {
        api::ce_attr_changed(js, el, name, text(old_value), text(new_value));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_frame_contexts(js: *mut NsJs) {
    api::free_frame_contexts(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_context_count(js: *const NsJs) -> c_uint {
    api::frame_context_count(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_polyfills(js: *mut NsJs) {
    api::eval_polyfills(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_snapshot_platform_globals(js: *mut NsJs) {
    api::snapshot_platform_globals(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_add_csp_header(js: *mut NsJs, header_value: *const c_char) {
    api::add_csp_header(js, text(header_value));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_form_action_allowed(
    js: *const NsJs,
    action_url: *const c_char,
) -> GBoolean {
    flag(api::csp_form_action_allowed(js, text(action_url)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_storage_partition(js: *const NsJs) -> *const c_char {
    record::partition_key_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_partition_key(js: *const NsJs) -> *const c_char {
    record::partition_key_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_cookie_value(js: *const NsJs) -> *const c_char {
    record::cookie_value_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_cookie_value(js: *mut NsJs, value: *const c_char) {
    api::set_cookie_value(js, text(value));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_inline_handlers_allowed(js: *const NsJs) -> GBoolean {
    flag(api::inline_handlers_allowed(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_us(js: *const NsJs) -> i64 {
    api::time_origin_us(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64 {
    api::time_origin_real_ms(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_csp(js: *const NsJs) -> *const c_void {
    api::page_csp(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_csp_allows_connect(
    js: *const NsJs,
    url: *const c_char,
    page: *const c_char,
) -> GBoolean {
    flag(text(url).is_none_or(|url| api::csp_allows_connect(js, url, text(page))))
}

#[unsafe(no_mangle)]
pub fn nsr_make_element(
    scope: &mut southstar_js_engine::Scope<'_>,
    node: *const NsNode,
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::api::make_element(scope, node)
}

#[unsafe(no_mangle)]
pub fn nsr_unwrap_element(value: &southstar_js_engine::Value) -> *const NsNode {
    Node::ptr_or_null(crate::api::unwrap_element(value))
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_worker_js_eval(
    js: *mut NsJs,
    source: &core::ffi::CStr,
    len: usize,
    url: &core::ffi::CStr,
    module: bool,
) -> Result<(), (bool, southstar_js_engine::Value)> {
    let Some(js) = Js::of(js) else {
        return Err((false, southstar_js_engine::Value::undefined()));
    };
    match js.scope(|scope| crate::worker::eval(scope, source, len, url, module)) {
        None => Err((false, southstar_js_engine::Value::undefined())),
        Some(crate::worker::EvalStatus::Done) => Ok(()),
        Some(crate::worker::EvalStatus::ParseError(thrown)) => Err((true, thrown)),
        Some(crate::worker::EvalStatus::Thrown(thrown)) => Err((false, thrown)),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_queue_message_task(
    scope: &mut southstar_js_engine::Scope<'_>,
    f: southstar_js_engine::TaskFn,
    args: &[southstar_js_engine::Value],
) -> Result<(), southstar_js_engine::Value> {
    crate::api::queue_message_task(scope, f, args)
}

#[unsafe(no_mangle)]
pub fn nsr_js_exception_message(
    scope: &mut southstar_js_engine::Scope<'_>,
    exception: &southstar_js_engine::Value,
) -> Vec<u8> {
    crate::api::exception_message(scope, exception)
}

#[unsafe(no_mangle)]
pub fn nsr_bind_event_target_listeners(
    scope: &mut southstar_js_engine::Scope<'_>,
    object: &southstar_js_engine::Value,
) {
    crate::api::bind_event_target_listeners(scope, object);
}

#[unsafe(no_mangle)]
pub fn nsr_illegal_constructor(
    scope: &mut southstar_js_engine::Scope<'_>,
    this: &southstar_js_engine::Value,
    args: &[southstar_js_engine::Value],
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::api::illegal_constructor(scope, this, args)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_timer_scope_leave(
    token: *mut c_void,
    exception: Option<southstar_js_engine::Value>,
) {
    unsafe { crate::api::timer_scope_leave(token, exception) };
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_this_is_detached_window(
    js: *mut NsJs,
    scope: &mut southstar_js_engine::Scope<'_>,
    this: &southstar_js_engine::Value,
) -> bool {
    crate::api::this_is_detached_window(js, scope, this)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_new_document_object(
    js: *mut NsJs,
    scope: &mut southstar_js_engine::Scope<'_>,
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::api::new_document_object(js, scope)
}
