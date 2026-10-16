//! Southstar — the C ABI and Rust-ABI entry points of the animation-frame and lifecycle ticks, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_anim::AnimHandle;
use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use crate::{events, frames, lifecycle, scrollend};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn from_data(data: *mut c_void) -> Js {
        Js(data.cast())
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn key(self) -> usize {
        self.0 as usize
    }

    pub fn halted(self) -> bool {
        api::halted(self.0)
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.0)
    }

    pub fn eval_depth(self) -> c_int {
        api::eval_depth(self.0)
    }

    pub fn callback_depth(self) -> c_int {
        api::callback_depth(self.0)
    }

    pub fn dispatch_depth(self) -> c_int {
        api::dispatch_depth(self.0)
    }

    pub fn iframe_load_depth(self) -> c_int {
        api::iframe_load_depth(self.0)
    }

    pub fn is_worker(self) -> bool {
        api::is_worker(self.0)
    }

    pub fn ctx(self) -> ContextHandle {
        api::main_context(self.0)
    }

    pub fn has_ctx(self) -> bool {
        !self.ctx().is_null()
    }

    pub fn mutated(self) -> bool {
        api::mutated(self.0)
    }

    pub fn current_document(self) -> *const NsNode {
        Node::ptr_or_null(api::current_document(self.0))
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn log(self, line: &str) {
        if let Ok(line) = CString::new(line.replace('\0', "")) {
            api::log_line(self.0, &line);
        }
    }

    pub fn context_frame(self, scope: &Scope<'_>) -> *mut NsNode {
        Node::ptr_or_null(api::context_frame(self.0, scope.handle())).cast_mut()
    }

    pub fn budget_enter(self) -> i64 {
        api::budget_enter(self.0)
    }

    pub fn budget_leave(self, saved: i64) {
        api::budget_leave(self.0, saved);
    }

    pub fn callback_depth_add(self, delta: c_int) {
        api::callback_depth_add(self.0, delta);
    }

    pub fn drain_microtasks(self) {
        api::drain_microtasks(self.0);
    }

    pub fn drain_mutations(self) {
        api::drain_mutations(self.0);
    }

    pub fn flush_ready_images(self) {
        southstar_js_images::flush_ready_images(self.0);
    }

    pub fn flush_autofocus(self) {
        unsafe { ns_js_flush_autofocus(self.0) };
    }

    pub fn promote_deferred_iframes(self) {
        unsafe { ns_js_promote_deferred_iframes(self.0) };
    }

    pub fn process_pending_iframes(self) {
        api::process_pending_iframes(self.0);
    }

    pub fn forget_frame_context(self, frame: *const NsNode) {
        if let Some(frame) = node(frame) {
            api::forget_frame_context(self.0, frame);
        }
    }

    pub fn node_in_page(self, node_ptr: *const NsNode) -> bool {
        api::node_in_page(self.0, node(node_ptr))
    }

    pub fn dispatch_built(self, target: *const NsNode, kind: &CStr, event: Value) {
        unsafe { nsr_js_dispatch_built_event(self.0, target, &kind.to_string_lossy(), event) };
    }

    pub fn dispatch_window_only(self, document: *const NsNode, kind: &CStr, event: Value) {
        unsafe {
            nsr_js_dispatch_window_only_event(self.0, document, &kind.to_string_lossy(), event)
        };
    }

    pub fn dispatch(self, target: *const NsNode, kind: &CStr) {
        unsafe { ns_js_dispatch_event(self.0, target, kind.as_ptr(), ptr::null_mut()) };
    }

    pub fn fire_img_load_once(self, node: Node<'_>, failed: bool) {
        southstar_js_images::fire_img_load_once(self.0, node, failed);
    }

    pub fn time_origin_ms(self, realm: ContextHandle, now_us: i64) -> f64 {
        southstar_js_perf::relative_ms(now_us, southstar_js_perf::time_origin_us(self.0, realm))
    }

    pub fn attach_timeout(self, ms: c_uint, func: SourceFunc) -> c_uint {
        api::attach_timeout(self.0, ms, func, self.0.cast())
    }

    pub fn source_remove(self, id: c_uint) {
        api::source_remove(self.0, id);
    }

    pub fn timeout_add(self, ms: c_uint, func: SourceFunc) -> c_uint {
        unsafe { g_timeout_add(ms, func, self.0.cast()) }
    }

    pub fn set_ready_state(self, state: c_int) {
        api::set_ready_state(self.0, state);
    }

    pub fn mark_navigation(self, key: &str) {
        southstar_js_perf::mark_navigation(self.0, key);
    }

    pub fn run_next_script_schedule(
        self,
        tasks: *mut GArray,
        schedule: c_int,
        origin: &CStr,
    ) -> bool {
        unsafe { ns_js_run_next_script_schedule(self.0, tasks, schedule, origin.as_ptr()) != 0 }
    }

    pub fn drain_load_event_scripts(self) {
        unsafe { ns_js_drain_load_event_scripts(self.0) };
    }

    pub fn schedule_pending_script_drain(self) {
        unsafe { ns_js_schedule_pending_script_drain(self.0) };
    }

    pub fn upgrade_custom_elements(self, root: *mut NsNode) {
        unsafe { ns_ce_upgrade_subtree_all(self.0, root) };
    }

    pub fn fire_parsed_stylesheet_loads(self, doc: *mut NsNode) {
        if let Some(doc) = node(doc) {
            southstar_js_loader::fire_parsed_stylesheet_loads(self.0, doc);
        }
    }

    pub fn fire_page_transition(self, kind: &CStr) {
        unsafe { ns_js_fire_page_transition(self.0, kind.as_ptr(), glib::FALSE) };
    }

    pub fn run_content_scripts(self, url: Option<&CStr>) {
        unsafe {
            ns_js_run_content_scripts(self.0, url.map_or(ptr::null(), CStr::as_ptr), glib::FALSE)
        };
    }

    pub fn load_blocked(self) -> bool {
        southstar_js_images::image_loads_pending(self.0)
            || unsafe {
                ns_js_pending_iframe_count(self.0) > 0
                    || ns_js_has_pending_script_roots(self.0) != 0
                    || api::load_delayed(self.0)
            }
    }

    pub fn has_pending_work(self) -> bool {
        unsafe {
            ns_services_timers_pending(self.0, glib::FALSE) != 0
                || frames::has_pending(self)
                || ns_js_net_pending_fetches(self.0) > 0
                || ns_js_net_pending_xhrs(self.0) > 0
                || ns_js_net_pending_sockets(self.0) > 0
                || ns_js_filereader_pending(self.0) != 0
                || ns_js_pending_iframe_count(self.0) > 0
                || ns_mutation_drain_pending(self.0) != 0
                || api::observer_tick_pending(self.0)
                || ns_js_async_scripts_pending(self.0) != 0
                || ns_ce_has_pending(self.0) != 0
                || ns_workers_pending(self.0) != 0
        }
    }

    pub fn needs_tick(self) -> bool {
        self.has_pending_work()
            || unsafe {
                ns_services_timers_pending(self.0, glib::TRUE) != 0
                    || api::message_tasks_pending(self.0)
            }
            || southstar_js_images::image_loads_pending(self.0)
    }
}

pub(crate) type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

pub(crate) const SOURCE_CONTINUE: GBoolean = glib::TRUE;
pub(crate) const SOURCE_REMOVE: GBoolean = glib::FALSE;

unsafe extern "C" {
    fn ns_js_flush_autofocus(js: *mut NsJs);
    fn ns_js_promote_deferred_iframes(js: *mut NsJs);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_image_settled(image: *const c_void, failed: *mut GBoolean) -> GBoolean;
    fn ns_js_video_take_load_events(video: *mut c_void) -> GBoolean;
    fn ns_anim_drain_events(anim: *mut c_void, cb: Option<AnimEventCb>, user: *mut c_void);
    fn g_timeout_add(interval: c_uint, func: SourceFunc, data: *mut c_void) -> c_uint;
    fn g_source_remove(id: c_uint) -> GBoolean;
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn g_get_monotonic_time() -> i64;
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_js_run_next_script_schedule(
        js: *mut NsJs,
        tasks: *mut GArray,
        schedule: c_int,
        origin: *const c_char,
    ) -> GBoolean;
    fn ns_js_drain_load_event_scripts(js: *mut NsJs);
    fn ns_js_schedule_pending_script_drain(js: *mut NsJs);
    fn ns_js_has_pending_script_roots(js: *const NsJs) -> GBoolean;
    fn ns_ce_upgrade_subtree_all(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_fire_page_transition(js: *mut NsJs, kind: *const c_char, persisted: GBoolean);
    fn ns_js_run_content_scripts(js: *mut NsJs, url: *const c_char, document_start: GBoolean);
    fn ns_js_profile_enabled() -> GBoolean;
    fn ns_js_pending_iframe_count(js: *const NsJs) -> c_uint;
    fn ns_services_timers_pending(js: *const NsJs, include_idle: GBoolean) -> GBoolean;
    fn ns_js_net_pending_fetches(js: *const NsJs) -> c_uint;
    fn ns_js_net_pending_xhrs(js: *const NsJs) -> c_uint;
    fn ns_js_net_pending_sockets(js: *const NsJs) -> c_uint;
    fn ns_js_filereader_pending(js: *const NsJs) -> GBoolean;
    fn ns_mutation_drain_pending(js: *const NsJs) -> GBoolean;
    fn ns_js_async_scripts_pending(js: *const NsJs) -> GBoolean;
    fn ns_ce_has_pending(js: *const NsJs) -> GBoolean;
    fn ns_workers_pending(js: *const NsJs) -> GBoolean;
}

unsafe extern "Rust" {
    unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value;
    unsafe fn nsr_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: &str,
        event: Value,
    ) -> (bool, bool);
    unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
}

pub(crate) type AnimEventCb = unsafe extern "C" fn(
    node: *const NsNode,
    kind: *const c_char,
    name: *const c_char,
    elapsed_ms: f64,
    user: *mut c_void,
);

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn in_blocking_fetch() -> bool {
    unsafe { ns_engine_in_blocking_fetch() != 0 }
}

pub(crate) fn profile_enabled() -> bool {
    unsafe { ns_js_profile_enabled() != 0 }
}

pub(crate) fn source_remove_default(id: c_uint) {
    unsafe { g_source_remove(id) };
}

pub(crate) fn array_free(tasks: *mut GArray) {
    if !tasks.is_null() {
        unsafe { g_array_free(tasks, glib::TRUE) };
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: *const NsNode) -> Value {
    unsafe { nsr_make_event(scope, &kind.to_string_lossy(), target) }
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Value {
    api::make_element(scope, node).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn unwrap_element(value: &Value) -> *mut NsNode {
    Node::ptr_or_null(api::unwrap_element(value)).cast_mut()
}

pub(crate) fn image_settled(image: *const c_void) -> Option<bool> {
    let mut failed = glib::FALSE;
    (unsafe { ns_js_image_settled(image, &mut failed) } != 0).then_some(failed != 0)
}

pub(crate) fn video_take_load_events(video: *mut c_void) -> bool {
    unsafe { ns_js_video_take_load_events(video) != 0 }
}

pub(crate) fn drain_anim_events(js: Js, anim: *mut c_void, cb: AnimEventCb) {
    unsafe { ns_anim_drain_events(anim, Some(cb), js.0.cast()) };
}

pub(crate) fn node<'a>(node: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(node) }
}

pub(crate) fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

pub(crate) fn layout_box<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

pub(crate) struct CallbackScope(api::RafScope);

impl CallbackScope {
    pub fn enter(js: Js, realm: ContextHandle, frame: *mut NsNode) -> Option<CallbackScope> {
        api::enter_raf_scope(js.0, realm, node(frame)).map(CallbackScope)
    }

    pub fn context(&self) -> ContextHandle {
        self.0.context()
    }

    pub fn run<R>(&self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.context().with(f)
    }

    pub fn leave(self) {
        self.0.leave();
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_window_requestAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    frames::request_animation_frame(scope, this, args)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_window_cancelAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    frames::cancel_animation_frame(scope, this, args)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn run_animation_frame(js: *mut c_void) -> bool {
    let js = Js::from_data(js);
    !js.is_null() && frames::run(js)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn has_pending_animation_frame(js: *mut c_void) -> bool {
    frames::has_pending(Js::from_data(js))
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn has_pending_work(js: *mut c_void) -> bool {
    let js = Js::from_data(js);
    !js.is_null() && js.has_pending_work()
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn needs_tick(js: *mut c_void) -> bool {
    let js = Js::from_data(js);
    !js.is_null() && js.needs_tick()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_count(js: *const NsJs) -> c_uint {
    frames::count(Js(js.cast_mut())) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_raf_set_host_driven(js: *mut NsJs) {
    frames::set_host_driven(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_idle_frame_end(js: *const NsJs, now: i64, end: i64) -> i64 {
    frames::idle_frame_end(Js(js.cast_mut()), now, end)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_frame_rafs(js: *mut NsJs, frame: *const NsNode) {
    frames::purge_frame(Js(js), frame);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_media_rafs(js: *mut NsJs, root: *const NsNode) {
    frames::purge_media(Js(js), root);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_queue_scrollend(js: *mut NsJs, el: *const NsNode) {
    scrollend::queue(Js(js), el);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_queue_scrollend_document(js: *mut NsJs) {
    scrollend::queue_document(Js(js));
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_anim_events(js: *mut c_void, anim: AnimHandle) {
    events::dispatch_anim_events(Js::from_data(js), anim.raw());
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn fire_media_load_events(js: *mut c_void, layout: Option<BoxRef<'_>>) {
    events::fire_media_load_events(
        Js::from_data(js),
        layout.map_or(ptr::null(), BoxRef::as_ptr),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_lifecycle_clear(js: *mut NsJs) {
    lifecycle::clear(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_lifecycle_start(
    js: *mut NsJs,
    doc: *mut NsNode,
    tasks: *mut GArray,
    origin: *const c_char,
    start_us: i64,
) {
    lifecycle::start(Js(js), doc, tasks, text(origin), start_us);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_lifecycle_tasks(js: *const NsJs) -> *mut GArray {
    lifecycle::tasks(Js(js.cast_mut()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_lifecycle_reset(js: *mut NsJs) {
    crate::reset(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_lifecycle_teardown(js: *mut NsJs) {
    crate::teardown(Js(js));
}
