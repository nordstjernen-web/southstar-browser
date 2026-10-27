//! Southstar — the C ABI and Rust-ABI entry points of the animation-frame and lifecycle ticks, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_anim::AnimHandle;
use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use southstar_js_loader::{Schedule, ScriptTasks};

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
        southstar_js_top_layer::flush_autofocus(self.0);
    }

    pub fn promote_deferred_iframes(self) {
        southstar_js_frames::promote_deferred_iframes(self.0);
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
        if let Some(target) = node(target) {
            southstar_js_dispatch::dispatch_built_event(
                self.0,
                target,
                &kind.to_string_lossy(),
                event,
            );
        }
    }

    pub fn dispatch_window_only(self, document: *const NsNode, kind: &CStr, event: Value) {
        southstar_js_dispatch::dispatch_window_only_event(
            self.0,
            node(document),
            &kind.to_string_lossy(),
            event,
        );
    }

    pub fn dispatch(self, target: *const NsNode, kind: &CStr) {
        if let Some(target) = node(target) {
            southstar_js_dispatch::dispatch_event(self.0, target, &kind.to_string_lossy());
        }
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

    pub fn run_next_script_schedule(self, schedule: Schedule, origin: &CStr) -> bool {
        southstar_js_loader::run_next_lifecycle_schedule(self.0, schedule, Some(origin))
    }

    pub fn set_lifecycle_tasks(self, tasks: Option<ScriptTasks>) {
        southstar_js_loader::set_lifecycle_tasks(self.0, tasks);
    }

    pub fn has_lifecycle_tasks(self) -> bool {
        southstar_js_loader::has_lifecycle_tasks(self.0)
    }

    pub fn drain_load_event_scripts(self) {
        southstar_js_loader::drain_load_event_scripts(self.0);
    }

    pub fn schedule_pending_script_drain(self) {
        southstar_js_loader::schedule_pending_script_drain(self.0);
    }

    pub fn upgrade_custom_elements(self, root: *mut NsNode) {
        if let Some(root) = node(root) {
            southstar_js_custom_elements::upgrade_subtree_all(self.0, root);
        }
    }

    pub fn fire_parsed_stylesheet_loads(self, doc: *mut NsNode) {
        if let Some(doc) = node(doc) {
            southstar_js_loader::fire_parsed_stylesheet_loads(self.0, doc);
        }
    }

    pub fn fire_page_transition(self, kind: &CStr) {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_window::fire_page_transition(self.0.cast(), kind, false) };
    }

    pub fn run_content_scripts(self, url: Option<&CStr>) {
        southstar_js_loader::run_content_scripts(self.0, url, false);
    }

    pub fn load_blocked(self) -> bool {
        southstar_js_images::image_loads_pending(self.0)
            || southstar_js_frames::pending_iframe_count(self.0) > 0
            || southstar_js_loader::has_pending_script_roots(self.0)
            || api::load_delayed(self.0)
    }

    pub fn has_pending_work(self) -> bool {
        southstar_js_services::timers_pending(self.0, false)
            || frames::has_pending(self)
            || southstar_js_net::pending_fetch_count(self.0) > 0
            || southstar_js_net::pending_xhr_count(self.0) > 0
            || southstar_js_net::pending_socket_count(self.0) > 0
            || southstar_js_url::filereader_pending(self.0)
            || southstar_js_frames::pending_iframe_count(self.0) > 0
            || southstar_js_observers::mutation_drain_pending(self.0)
            || api::observer_tick_pending(self.0)
            || southstar_js_loader::async_scripts_pending(self.0)
            || southstar_js_custom_elements::has_pending(self.0)
            || (api::workers_hooks().pending)(self.0)
    }

    pub fn needs_tick(self) -> bool {
        self.has_pending_work()
            || southstar_js_services::timers_pending(self.0, true)
            || api::message_tasks_pending(self.0)
            || southstar_js_images::image_loads_pending(self.0)
    }
}

pub(crate) type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

pub(crate) const SOURCE_CONTINUE: GBoolean = glib::TRUE;
pub(crate) const SOURCE_REMOVE: GBoolean = glib::FALSE;

unsafe extern "C" {
    fn g_timeout_add(interval: c_uint, func: SourceFunc, data: *mut c_void) -> c_uint;
    fn g_source_remove(id: c_uint) -> GBoolean;
    fn g_get_monotonic_time() -> i64;
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) use southstar_engine::in_blocking_fetch;

pub(crate) fn profile_enabled() -> bool {
    southstar_js_loader::profile_enabled()
}

pub(crate) fn source_remove_default(id: c_uint) {
    unsafe { g_source_remove(id) };
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: *const NsNode) -> Value {
    southstar_js_events::make_event(scope, &kind.to_string_lossy(), node(target))
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Value {
    api::make_element(scope, node).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn unwrap_element(value: &Value) -> *mut NsNode {
    Node::ptr_or_null(api::unwrap_element(value)).cast_mut()
}

pub(crate) fn image_settled(image: southstar_layout::MediaHandle) -> Option<bool> {
    let image = southstar_image::ImageRef::from_media(image)?;
    (image.loaded() || image.failed()).then_some(image.failed())
}

pub(crate) fn node<'a>(node: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(node) }
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

#[allow(non_snake_case)]
pub fn nsr_window_requestAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    frames::request_animation_frame(scope, this, args)
}

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

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_anim_events(js: *mut c_void, anim: AnimHandle) {
    events::dispatch_anim_events(Js::from_data(js), anim);
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

pub fn raf_count(js: *const NsJs) -> usize {
    frames::count(Js(js.cast_mut()))
}

pub fn clear_lifecycle(js: *const NsJs) {
    lifecycle::clear(Js(js.cast_mut()));
}

pub fn reset_lifecycle(js: *const NsJs) {
    crate::reset(Js(js.cast_mut()));
}

pub fn start_lifecycle(
    js: *const NsJs,
    doc: Node<'_>,
    tasks: ScriptTasks,
    origin: Option<&CStr>,
    start_us: i64,
) {
    lifecycle::start(Js(js.cast_mut()), doc.as_mut_ptr(), tasks, origin, start_us);
}

fn raf_set_host_driven(js: *const NsJs) {
    frames::set_host_driven(Js(js.cast_mut()));
}

pub fn purge_frame_rafs(js: *const NsJs, frame: Node<'_>) {
    frames::purge_frame(Js(js.cast_mut()), frame.as_ptr());
}

fn teardown(js: *const NsJs) {
    crate::teardown(Js(js.cast_mut()));
}

pub fn install_runtime_hooks() {
    api::set_lifecycle_hooks(api::LifecycleHooks {
        raf_set_host_driven,
        purge_frame_rafs,
        teardown,
        idle_frame_end,
        purge_media_rafs,
        queue_scrollend,
        queue_scrollend_document,
    });
    southstar_js_runtime::links::set_js_lifecycle_links(
        southstar_js_runtime::links::JsLifecycleLinks {
            window_cancel_animation_frame: crate::ffi::nsr_window_cancelAnimationFrame,
            window_request_animation_frame: crate::ffi::nsr_window_requestAnimationFrame,
        },
    );
    southstar_js_frames::links::set_js_lifecycle_links(
        southstar_js_frames::links::JsLifecycleLinks {
            window_cancel_animation_frame: crate::ffi::nsr_window_cancelAnimationFrame,
            window_request_animation_frame: crate::ffi::nsr_window_requestAnimationFrame,
        },
    );
}

fn idle_frame_end(js: *const NsJs, now: i64, end: i64) -> i64 {
    frames::idle_frame_end(Js(js.cast_mut()), now, end)
}

fn purge_media_rafs(js: *const NsJs, root: Node<'_>) {
    frames::purge_media(Js(js.cast_mut()), root.as_ptr());
}

fn queue_scrollend(js: *const NsJs, el: Node<'_>) {
    scrollend::queue(Js(js.cast_mut()), el.as_ptr());
}

fn queue_scrollend_document(js: *const NsJs) {
    scrollend::queue_document(Js(js.cast_mut()));
}
