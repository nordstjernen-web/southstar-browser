//! Southstar — the C ABI and Rust-ABI entry points of the animation-frame and lifecycle ticks, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_anim::AnimHandle;
use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::BoxRef;

use southstar_js_loader::{Schedule, ScriptTasks};

use crate::{events, frames, lifecycle, scrollend};

use southstar_js_runtime::api::Page;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub fn from_data(data: *mut c_void) -> Js {
        Js(Page::from_raw(data))
    }

    pub fn of_page(page: Page) -> Js {
        Js(Some(page))
    }

    fn raw(self) -> *mut c_void {
        self.0.map_or(core::ptr::null_mut(), Page::as_raw)
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }

    pub fn key(self) -> usize {
        self.0.map_or(0, Page::addr)
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

    pub fn current_document(self) -> Option<Node<'static>> {
        api::current_document(self.0)
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn log(self, line: &str) {
        if let Ok(line) = CString::new(line.replace('\0', "")) {
            api::log_line(self.0, &line);
        }
    }

    pub fn context_frame(self, scope: &Scope<'_>) -> Option<Node<'static>> {
        api::context_frame(self.0, scope.handle())
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

    pub fn forget_frame_context(self, frame: Node<'_>) {
        api::forget_frame_context(self.0, frame);
    }

    pub fn node_in_page(self, node: Option<Node<'_>>) -> bool {
        api::node_in_page(self.0, node)
    }

    pub fn dispatch_built(self, target: Option<Node<'_>>, kind: &CStr, event: Value) {
        if let Some(target) = target {
            southstar_js_dispatch::dispatch_built_event(
                self.0,
                target,
                &kind.to_string_lossy(),
                event,
            );
        }
    }

    pub fn dispatch_window_only(self, document: Option<Node<'_>>, kind: &CStr, event: Value) {
        southstar_js_dispatch::dispatch_window_only_event(
            self.0,
            document,
            &kind.to_string_lossy(),
            event,
        );
    }

    pub fn dispatch(self, target: Option<Node<'_>>, kind: &CStr) {
        if let Some(target) = target {
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
        api::attach_timeout(self.0, ms, func, self.raw())
    }

    pub fn source_remove(self, id: c_uint) {
        api::source_remove(self.0, id);
    }

    pub fn timeout_add(self, ms: c_uint, func: SourceFunc) -> c_uint {
        unsafe { g_timeout_add(ms, func, self.raw()) }
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

    pub fn upgrade_custom_elements(self, root: Option<Node<'_>>) {
        if let Some(root) = root {
            southstar_js_custom_elements::upgrade_subtree_all(self.0, root);
        }
    }

    pub fn fire_parsed_stylesheet_loads(self, doc: Option<Node<'_>>) {
        if let Some(doc) = doc {
            southstar_js_loader::fire_parsed_stylesheet_loads(self.0, doc);
        }
    }

    pub fn fire_page_transition(self, kind: &CStr) {
        if let Some(page) = self.0.filter(|page| page.is_live()) {
            southstar_js_window::fire_page_transition(page, kind, false);
        }
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
    Js(Page::current(scope))
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: Option<Node<'_>>) -> Value {
    southstar_js_events::make_event(scope, &kind.to_string_lossy(), target)
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: Option<Node<'_>>) -> Value {
    api::make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    api::unwrap_element(value)
}

pub(crate) fn image_settled(image: southstar_layout::MediaHandle) -> Option<bool> {
    let image = southstar_image::ImageRef::from_media(image)?;
    (image.loaded() || image.failed()).then_some(image.failed())
}

pub(crate) struct CallbackScope(api::RafScope);

impl CallbackScope {
    pub fn enter(js: Js, realm: ContextHandle, frame: Option<Node<'_>>) -> Option<CallbackScope> {
        api::enter_raf_scope(js.0, realm, frame).map(CallbackScope)
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

pub fn run_animation_frame(js: Page) -> bool {
    frames::run(Js::of_page(js))
}

pub fn has_pending_animation_frame(js: Page) -> bool {
    frames::has_pending(Js::of_page(js))
}

pub fn has_pending_work(js: Page) -> bool {
    Js::of_page(js).has_pending_work()
}

pub fn needs_tick(js: Page) -> bool {
    Js::of_page(js).needs_tick()
}

pub fn dispatch_anim_events(js: Page, anim: AnimHandle) {
    events::dispatch_anim_events(Js::of_page(js), anim);
}

pub fn fire_media_load_events(js: Page, layout: Option<BoxRef<'_>>) {
    events::fire_media_load_events(Js::of_page(js), layout);
}

pub fn raf_count(js: Option<Page>) -> usize {
    frames::count(Js(js))
}

pub fn clear_lifecycle(js: Option<Page>) {
    lifecycle::clear(Js(js));
}

pub fn reset_lifecycle(js: Option<Page>) {
    crate::reset(Js(js));
}

pub fn start_lifecycle(
    js: Option<Page>,
    doc: Node<'_>,
    tasks: ScriptTasks,
    origin: Option<&CStr>,
    start_us: i64,
) {
    lifecycle::start(Js(js), doc, tasks, origin, start_us);
}

fn raf_set_host_driven(js: Option<Page>) {
    frames::set_host_driven(Js(js));
}

pub fn purge_frame_rafs(js: Option<Page>, frame: Node<'_>) {
    frames::purge_frame(Js(js), frame);
}

fn teardown(js: Option<Page>) {
    crate::teardown(Js(js));
}

pub fn install_runtime_hooks() {
    api::set_lifecycle_hooks(api::LifecycleHooks {
        raf_set_host_driven: |js| raf_set_host_driven(Some(js)),
        purge_frame_rafs,
        teardown: |js| teardown(Some(js)),
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

fn idle_frame_end(js: Option<Page>, now: i64, end: i64) -> i64 {
    frames::idle_frame_end(Js(js), now, end)
}

fn purge_media_rafs(js: Option<Page>, root: Node<'_>) {
    frames::purge_media(Js(js), root);
}

fn queue_scrollend(js: Option<Page>, el: Node<'_>) {
    scrollend::queue(Js(js), el);
}

fn queue_scrollend_document(js: Option<Page>) {
    scrollend::queue_document(Js(js));
}
