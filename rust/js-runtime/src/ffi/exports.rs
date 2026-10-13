//! Southstar — the C ABI of the page's scheduling, realm scopes and runtime state as declared in src/js.h and src/js_internal.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GHashTable};
use southstar_js_engine::ContextHandle;

use super::glue::{self, Response, SourceFn};
use super::scopes;
use super::state::{
    DataFn, DelayFn, DownloadFn, FormSubmitFn, NavigateFn, NodeFn, PageState, TextQueryFn, UrlFn,
    ViewportScrollFn,
};
use super::{GMainContext, Js, NsJs, WorkerHost};
use crate::scheduling::{self, RunTasks};
use crate::{budget, orphans};

const TRANSIENT_ACTIVATION_US: i64 = 5_000_000;
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
    realm: ContextHandle,
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
pub unsafe extern "C" fn ns_js_context_frame(js: *mut NsJs, ctx: ContextHandle) -> *mut NsNode {
    with(js, ptr::null_mut(), |js| {
        if ctx.is_null() || ctx == js.main_realm() {
            return ptr::null_mut();
        }
        let frame = scopes::frame_of_realm(js, ctx);
        if frame.is_null() { js.frame() } else { frame }
    })
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
    with(js, (), scheduling::drop_message_tasks);
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
pub unsafe extern "C" fn ns_js_realm_url(js: *mut NsJs, realm: ContextHandle) -> *const c_char {
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
pub unsafe extern "C" fn ns_js_main_realm_context(js: *const NsJs) -> ContextHandle {
    Js(js.cast_mut()).main_realm()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm(js: *const NsJs) -> ContextHandle {
    Js(js.cast_mut()).main_realm()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_context(js: *const NsJs) -> ContextHandle {
    Js(js.cast_mut()).ctx()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pattern_context() -> ContextHandle {
    glue::active_js().map_or(ContextHandle::NULL, Js::main_or_current)
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
pub unsafe extern "C" fn ns_js_realm_url_enter(js: *mut NsJs, realm: ContextHandle) -> *mut c_void {
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
pub unsafe extern "C" fn ns_js_in_main_realm(
    js: *mut NsJs,
    f: unsafe extern "C" fn(data: *mut c_void),
    data: *mut c_void,
) {
    let js = Js::of(js);
    let realm = js.map_or(ContextHandle::NULL, Js::main_realm);
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
pub unsafe extern "C" fn ns_js_set_realm(js: *mut NsJs, ctx: ContextHandle, doc: *mut NsNode) {
    let js = Js(js);
    js.set_ctx(ctx);
    js.set_current_doc(doc);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_module_context(js: *const NsJs) -> ContextHandle {
    let js = Js(js.cast_mut());
    js.st().module_ctx.get().or(js.ctx())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_swap_module_context(
    js: *mut NsJs,
    ctx: ContextHandle,
) -> ContextHandle {
    Js(js).st().module_ctx.replace(ctx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_document_node(js: *const NsJs) -> *const NsNode {
    match &*Js(js.cast_mut()).st().iframe_doc.borrow() {
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
pub unsafe extern "C" fn ns_js_net_leave_handler_realm(ctx: ContextHandle, scope: *mut c_void) {
    unsafe {
        scopes::realm_scope_pop(Js::of_context(ctx), scope);
        scopes::realm_scope_free(scope);
    }
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
pub unsafe extern "C" fn ns_js_frame_load_leave(js: *mut NsJs, token: *mut c_void) {
    unsafe { scopes::frame_load_leave(Js(js), token) };
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
    let js = Js::of(js);
    if js.is_some_and(Js::halted) {
        return 0;
    }
    glue::pump_iteration(js);
    1
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
