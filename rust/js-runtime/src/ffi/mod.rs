//! Southstar — the C ABI of realm creation and teardown as declared in src/js.h and src/js_internal.h, and the js.c accessors it reaches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod exports;
pub(crate) mod glue;
pub(crate) mod natives;
pub(crate) mod scopes;
pub(crate) mod state;
pub(crate) mod wrapper;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable, GPtrArray};
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::worker::{self, EvalStatus};
use crate::{Element, interrupt, page, teardown};
use state::{DataFn, LogFn, NavigateFn, PageState, Slots};

pub(crate) type Native = JSCFunction;
pub(crate) type Installer = unsafe extern "C" fn(ctx: *mut JSContext, target: JSValue);

type InterruptHandler = unsafe extern "C" fn(rt: *mut c_void, opaque: *mut c_void) -> c_int;
type RejectionTrackerFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    opaque: *mut c_void,
);

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct WorkerHost {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct GMainContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct NavigationTiming {
    pub origin_us: i64,
    pub origin_real_ms: f64,
    milestones: [f64; 15],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<NavigationTiming>() == 136);

impl NavigationTiming {
    pub fn starting_at(origin_us: i64, origin_real_ms: f64) -> NavigationTiming {
        NavigationTiming {
            origin_us,
            origin_real_ms,
            milestones: [0.0; 15],
        }
    }
}

#[repr(C)]
pub(crate) struct WorkerRealm {
    host: *mut WorkerHost,
    context: *mut GMainContext,
    url: *const c_char,
    base_url: *const c_char,
    origin: *const c_char,
    name: *const c_char,
    is_service_worker: GBoolean,
}

pub(crate) struct WorkerSetup<'a> {
    host: *mut WorkerHost,
    context: *mut GMainContext,
    pub url: Option<&'a CStr>,
    pub base_url: Option<&'a CStr>,
    pub origin: Option<&'a CStr>,
    pub name: Option<&'a CStr>,
    pub service_worker: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Callbacks {
    log: Option<LogFn>,
    log_data: *mut c_void,
    mutated: Option<DataFn>,
    mutated_data: *mut c_void,
    navigate: Option<NavigateFn>,
    navigate_data: *mut c_void,
}

#[derive(Clone, Copy)]
pub(crate) enum RejectionTracker {
    Page,
    Worker,
}

#[derive(Clone, Copy)]
pub(crate) enum Class {
    Element,
    Attr,
    Style,
    TokenList,
    Live,
    Dataset,
    Storage,
    WindowNamed,
}

const PROTO_TABLE_ELEMENT: c_int = 13;
const LOG_LEVEL_WARNING: c_int = 1 << 4;

unsafe extern "C" {
    fn ns_js_alloc() -> *mut NsJs;
    fn ns_js_set_time_origin(js: *mut NsJs, origin_us: i64, origin_real_ms: f64);
    fn ns_js_page_navigation_timing(js: *const NsJs) -> *const NavigationTiming;
    fn ns_perf_coarsen_navigation_timing(timing: *mut NavigationTiming);
    fn ns_perf_init(js: *mut NsJs);
    fn ns_perf_teardown(js: *mut NsJs);
    fn ns_js_create_runtime(
        js: *mut NsJs,
        memory_limit: usize,
        stack_size: usize,
        interrupt: InterruptHandler,
        rejection_tracker: RejectionTrackerFn,
    ) -> GBoolean;
    fn ns_js_create_context(js: *mut NsJs) -> GBoolean;
    fn g_main_context_default() -> *mut GMainContext;
    fn ns_js_init_tables(js: *mut NsJs, page: GBoolean);
    fn ns_js_set_partition_key(js: *mut NsJs, partition_key: *const c_char);
    fn ns_js_runtime_state(js: *const NsJs) -> *mut c_void;
    fn ns_js_set_runtime_state(js: *mut NsJs, state: *mut c_void);
    fn ns_js_slots_of(js: *mut NsJs, out: *mut Slots);
    fn g_source_remove(id: c_uint) -> GBoolean;
    fn ns_js_mutation_init(js: *mut NsJs);
    fn ns_storage_init(js: *mut NsJs);
    fn ns_media_init(js: *mut NsJs);
    fn ns_js_register_class(ctx: *mut JSContext, which: c_int) -> JSValue;
    fn ns_js_new_window_named(ctx: *mut JSContext) -> JSValue;
    fn ns_js_set_proto_document(js: *mut NsJs, proto: JSValue);
    fn ns_js_set_navigator_brand(js: *mut NsJs, brand: JSValue);
    fn ns_js_set_pristine_promise(js: *mut NsJs, promise: JSValue);
    fn ns_js_install_proto_table(ctx: *mut JSContext, proto: JSValue, table: c_int);
    fn ns_active_js() -> *mut NsJs;
    fn ns_set_active_js(js: *mut NsJs);

    fn ns_js_promise_rejection_tracker(
        ctx: *mut JSContext,
        promise: JSValue,
        reason: JSValue,
        is_handled: bool,
        opaque: *mut c_void,
    );
    fn ns_worker_promise_rejection_tracker(
        ctx: *mut JSContext,
        promise: JSValue,
        reason: JSValue,
        is_handled: bool,
        opaque: *mut c_void,
    );
    fn ns_worker_log_cb(line: *const c_char, user_data: *mut c_void);
    fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;

    fn ns_worker_host_closing(host: *const WorkerHost) -> GBoolean;

    fn ns_services_window_navigator(ctx: *mut JSContext) -> JSValue;
    fn ns_services_worker_navigator(ctx: *mut JSContext) -> JSValue;
    fn ns_services_chrome_compat() -> GBoolean;
    fn ns_make_performance_object(
        ctx: *mut JSContext,
        js: *mut NsJs,
        include_memory: GBoolean,
    ) -> JSValue;
    fn ns_perf_supported_entry_types(ctx: *mut JSContext) -> JSValue;
    fn ns_performance_extend_event_target(
        ctx: *mut JSContext,
        global: JSValue,
        performance: JSValue,
    );
    fn ns_js_sync_window_metrics(js: *const NsJs);
    fn ns_canvas_register_classes(rt: *mut c_void);
    fn ns_canvas_install(ctx: *mut JSContext, global: JSValue, window: GBoolean);
    fn ns_window_install_state(js: *mut NsJs, ctx: *mut JSContext, global: JSValue);
    fn ns_live_proto(ctx: *mut JSContext, which: c_int) -> JSValue;
    #[cfg(feature = "webgpu")]
    fn ns_webgpu_install(ctx: *mut JSContext, js: *mut NsJs, navigator: JSValue);
    fn ns_worker_install_location(ctx: *mut JSContext, global: JSValue, url: *const c_char);
    fn ns_worker_shape_global(ctx: *mut JSContext, service_worker: GBoolean);
    fn ns_js_eval_worker_polyfills(js: *mut NsJs);
    fn ns_worker_compile(
        ctx: *mut JSContext,
        src: *const c_char,
        len: usize,
        url: *const c_char,
        module: GBoolean,
        parse_error: *mut GBoolean,
    ) -> JSValue;

    fn ns_js_pinned_wrappers(js: *const NsJs) -> *mut GHashTable;
    fn ns_js_detach_wrapper(js: *mut NsJs, node: *mut NsNode, release: GBoolean);
    fn ns_js_orphans_snapshot(js: *mut NsJs) -> *mut GPtrArray;
    fn ns_js_orphan_tracked(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_js_mutation_teardown(js: *mut NsJs);
    fn ns_node_free(node: *mut NsNode);

    fn ns_js_free_page_strings(js: *mut NsJs);
    fn ns_js_free_pinned_set(js: *mut NsJs);
    fn ns_js_free_realm_values(js: *mut NsJs);
    fn ns_js_free_platform_globals(js: *mut NsJs);
    fn ns_js_free_partition(js: *mut NsJs);
    fn ns_js_free_lookup_caches(js: *mut NsJs);
    fn ns_js_free_window_values(js: *mut NsJs);
    fn ns_js_free_engine(js: *mut NsJs);

    fn ns_js_frames_clear_initial_blank(js: *mut NsJs);
    fn ns_top_layer_clear(js: *mut NsJs);
    fn ns_js_fonts_teardown(js: *mut NsJs);
    fn ns_lifecycle_teardown(js: *mut NsJs);
    fn ns_js_blob_registry_remove(js: *mut NsJs);
    fn ns_storage_flush(js: *mut NsJs);
    fn ns_js_forget_pending_change(js: *mut NsJs);
    fn ns_document_write_teardown(js: *mut NsJs);
    fn ns_workers_teardown(js: *mut NsJs);
    fn ns_window_history_teardown(js: *mut NsJs);
    fn ns_traversal_teardown(js: *mut NsJs);
    fn ns_services_teardown(js: *mut NsJs);
    fn ns_js_blob_urls_free(js: *mut NsJs);
    fn ns_canvas_states_teardown(js: *mut NsJs);
    fn ns_js_images_teardown(js: *mut NsJs);
    fn ns_dispatch_teardown_listeners(js: *mut NsJs);
    fn ns_js_net_teardown(js: *mut NsJs);
    fn ns_js_loader_teardown(js: *mut NsJs);
    fn ns_js_filereader_teardown(js: *mut NsJs);
    fn ns_attr_detach_all(js: *mut NsJs);
    fn ns_attribute_maps_teardown(js: *mut NsJs);
    fn ns_storage_free_deferred_events(js: *mut NsJs);
    fn ns_focus_teardown(js: *mut NsJs);
    fn ns_js_input_teardown(js: *mut NsJs);
    fn ns_observers_teardown(js: *mut NsJs);
    fn ns_ce_teardown(js: *mut NsJs);
    fn ns_storage_teardown(js: *mut NsJs);
    fn ns_media_teardown(js: *mut NsJs);
    fn ns_collections_teardown(js: *mut NsJs);
    fn ns_cssom_teardown(js: *mut NsJs);
    fn ns_url_teardown(js: *mut NsJs);
    fn ns_css_set_fullscreen_node(node: *const NsNode);
    fn ns_js_frames_teardown(js: *mut NsJs);
    fn ns_window_links_clear(js: *mut NsJs, destroy: GBoolean);
    fn ns_js_clear_frame_clocks(js: *mut NsJs, destroy: GBoolean);
    fn ns_js_drop_pending_rejections(js: *mut NsJs);
    fn ns_dispatch_teardown(js: *mut NsJs);

    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_self() -> usize;
    fn pthread_get_stacksize_np(thread: usize) -> usize;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    fn alloc() -> Js {
        let js = Js(unsafe { ns_js_alloc() });
        let mut slots = Slots::empty();
        unsafe { ns_js_slots_of(js.0, &mut slots) };
        let state = Box::into_raw(Box::new(PageState::new(slots)));
        unsafe { ns_js_set_runtime_state(js.0, state.cast()) };
        js
    }

    pub(crate) fn of(js: *const NsJs) -> Option<Js> {
        (!js.is_null()).then_some(Js(js.cast_mut()))
    }

    pub(crate) fn of_context(ctx: *mut JSContext) -> Option<Js> {
        if ctx.is_null() {
            return None;
        }
        Js::of(unsafe { JS_GetContextOpaque(ctx) }.cast())
    }

    pub(crate) fn ptr(self) -> *mut NsJs {
        self.0
    }

    pub(crate) fn st(self) -> &'static PageState {
        unsafe { &*ns_js_runtime_state(self.0).cast::<PageState>() }
    }

    pub fn ctx(self) -> *mut JSContext {
        unsafe { *self.st().slots.ctx }
    }

    pub fn set_ctx(self, ctx: *mut JSContext) {
        unsafe { *self.st().slots.ctx = ctx };
    }

    pub fn main_realm(self) -> *mut JSContext {
        unsafe { *self.st().slots.main_realm_ctx }
    }

    pub fn rt(self) -> *mut c_void {
        unsafe { *self.st().slots.rt }
    }

    pub fn current_url_raw(self) -> *mut c_char {
        unsafe { *self.st().slots.current_url }
    }

    pub fn set_current_url_raw(self, url: *mut c_char) {
        unsafe { *self.st().slots.current_url = url };
    }

    pub fn current_doc(self) -> *mut NsNode {
        unsafe { *self.st().slots.current_doc }
    }

    pub fn set_current_doc(self, doc: *mut NsNode) {
        unsafe { *self.st().slots.current_doc = doc };
    }

    pub fn frame(self) -> *mut NsNode {
        unsafe { *self.st().slots.raf_frame_ctx }
    }

    pub fn set_frame(self, frame: *mut NsNode) {
        unsafe { *self.st().slots.raf_frame_ctx = frame };
    }

    pub fn set_ce_main_doc(self, doc: *const NsNode) {
        unsafe { *self.st().slots.ce_main_doc = doc };
    }

    pub fn pinned_set(self) -> *mut GHashTable {
        unsafe { *self.st().slots.pinned_wrappers_set }
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let ctx = self.ctx();
        if ctx.is_null() {
            return None;
        }
        Some(unsafe { quickjs::with_context(ctx, f) })
    }

    pub fn set_navigation_timing(self, timing: NavigationTiming) {
        let slot = unsafe { ns_js_page_navigation_timing(self.0) }.cast_mut();
        unsafe { *slot = timing };
        self.set_time_origin(timing.origin_us, timing.origin_real_ms);
    }

    pub fn set_time_origin(self, origin_us: i64, origin_real_ms: f64) {
        unsafe { ns_js_set_time_origin(self.0, origin_us, origin_real_ms) };
    }

    pub fn perf_init(self) {
        unsafe { ns_perf_init(self.0) };
    }

    pub fn perf_teardown(self) {
        unsafe { ns_perf_teardown(self.0) };
    }

    pub fn create_runtime(
        self,
        memory_limit: usize,
        stack_size: usize,
        tracker: RejectionTracker,
    ) -> bool {
        let tracker: RejectionTrackerFn = match tracker {
            RejectionTracker::Page => ns_js_promise_rejection_tracker,
            RejectionTracker::Worker => ns_worker_promise_rejection_tracker,
        };
        unsafe {
            ns_js_create_runtime(self.0, memory_limit, stack_size, interrupt_handler, tracker) != 0
        }
    }

    pub fn create_context(self) -> bool {
        unsafe { ns_js_create_context(self.0) != 0 }
    }

    pub fn set_callbacks(self, callbacks: Callbacks) {
        let state = self.st();
        state.log.set(callbacks.log, callbacks.log_data);
        state
            .mutated_hook
            .set(callbacks.mutated, callbacks.mutated_data);
        state
            .navigate
            .set(callbacks.navigate, callbacks.navigate_data);
    }

    pub fn init_tables(self, main_context: *mut GMainContext, page: bool) {
        self.st().main_context.set(main_context);
        unsafe { ns_js_init_tables(self.0, glib::GBoolean::from(page)) };
    }

    pub fn init_page_states(self) {
        unsafe {
            ns_js_mutation_init(self.0);
            ns_storage_init(self.0);
            ns_media_init(self.0);
        }
    }

    pub fn adopt_worker_host(self, setup: &WorkerSetup<'_>) {
        let partition = setup
            .origin
            .filter(|origin| !origin.is_empty())
            .map_or(ptr::null(), CStr::as_ptr);
        self.st().worker_host.set(setup.host);
        unsafe { ns_js_set_partition_key(self.0, partition) };
        self.set_callbacks(Callbacks {
            log: Some(ns_worker_log_cb),
            log_data: setup.host.cast(),
            mutated: None,
            mutated_data: ptr::null_mut(),
            navigate: None,
            navigate_data: ptr::null_mut(),
        });
        self.init_tables(setup.context, false);
    }

    pub fn halted(self) -> bool {
        unsafe { *self.st().slots.halted != 0 }
    }

    pub fn halt(self) {
        unsafe { *self.st().slots.halted = glib::TRUE };
    }

    pub fn worker_closing(self) -> bool {
        let host = self.st().worker_host.get();
        !host.is_null() && unsafe { ns_worker_host_closing(host) != 0 }
    }

    pub fn set_proto_document(self, proto: Value) {
        unsafe { ns_js_set_proto_document(self.0, quickjs::into_raw(proto)) };
    }

    pub fn set_navigator_brand(self, brand: Value) {
        unsafe { ns_js_set_navigator_brand(self.0, quickjs::into_raw(brand)) };
    }

    pub fn set_pristine_promise(self, promise: Value) {
        unsafe { ns_js_set_pristine_promise(self.0, quickjs::into_raw(promise)) };
    }

    pub fn make_performance_object(self, scope: &mut Scope<'_>, include_memory: bool) -> Value {
        let ctx = quickjs::raw_context(scope);
        let raw =
            unsafe { ns_make_performance_object(ctx, self.0, GBoolean::from(include_memory)) };
        unsafe { quickjs::take_value(scope, raw) }
    }

    pub fn sync_window_metrics(self) {
        unsafe { ns_js_sync_window_metrics(self.0) };
    }

    pub fn install_window_state(self, scope: &mut Scope<'_>, global: &Value) {
        unsafe {
            ns_window_install_state(self.0, quickjs::raw_context(scope), quickjs::raw(global))
        };
    }

    #[cfg(feature = "webgpu")]
    pub fn install_webgpu(self, scope: &mut Scope<'_>, navigator: &Value) {
        unsafe { ns_webgpu_install(quickjs::raw_context(scope), self.0, quickjs::raw(navigator)) };
    }

    pub fn eval_worker_polyfills(self) {
        unsafe { ns_js_eval_worker_polyfills(self.0) };
    }

    pub fn is_active(self) -> bool {
        unsafe { ns_active_js() == self.0 }
    }

    pub fn clear_active(self) {
        unsafe { ns_set_active_js(ptr::null_mut()) };
    }

    pub fn take_pinned_wrappers(self) -> Vec<Element> {
        let set = unsafe { ns_js_pinned_wrappers(self.0) };
        if set.is_null() {
            return Vec::new();
        }
        let mut len: c_uint = 0;
        let keys = unsafe { glib::g_hash_table_get_keys_as_array(set, &mut len) };
        unsafe { glib::g_hash_table_remove_all(set) };
        let nodes = (0..len as usize)
            .filter_map(|i| unsafe { Node::from_ptr((*keys.add(i)).cast::<NsNode>()) })
            .collect();
        unsafe { glib::g_free(keys.cast()) };
        nodes
    }

    pub fn detach_wrapper(self, node: Element, release: bool) {
        unsafe { ns_js_detach_wrapper(self.0, node.as_mut_ptr(), GBoolean::from(release)) };
    }

    pub fn current_document(self) -> Option<*const NsNode> {
        let doc = self.current_doc();
        (!doc.is_null()).then_some(doc.cast_const())
    }

    pub fn orphans_snapshot(self) -> Option<Vec<Option<Element>>> {
        let orphans = unsafe { ns_js_orphans_snapshot(self.0) };
        if orphans.is_null() {
            return None;
        }
        let len = unsafe { (*orphans).len } as usize;
        let nodes = (0..len)
            .map(|i| unsafe { Node::from_ptr((*(*orphans).pdata.add(i)).cast::<NsNode>()) })
            .collect();
        unsafe { glib::g_ptr_array_free(orphans, glib::TRUE) };
        Some(nodes)
    }

    pub fn orphan_tracked(self, node: Element) -> bool {
        unsafe { ns_js_orphan_tracked(self.0, node.as_ptr()) != 0 }
    }

    pub fn teardown_mutation(self) {
        unsafe { ns_js_mutation_teardown(self.0) };
    }

    pub fn free_node(self, node: Element) {
        unsafe { ns_node_free(node.as_mut_ptr()) };
    }

    pub fn free_doc_ready_states(self) {
        self.st().doc_ready_states.borrow_mut().clear();
    }

    pub fn teardown_document_states(self) {
        unsafe {
            ns_js_frames_clear_initial_blank(self.0);
            ns_top_layer_clear(self.0);
            ns_js_fonts_teardown(self.0);
            ns_lifecycle_teardown(self.0);
            ns_js_blob_registry_remove(self.0);
            ns_storage_flush(self.0);
        }
    }

    pub fn free_page_strings(self) {
        unsafe { ns_js_free_page_strings(self.0) };
    }

    pub fn teardown_page_services(self) {
        unsafe {
            ns_js_forget_pending_change(self.0);
            ns_document_write_teardown(self.0);
            ns_workers_teardown(self.0);
            ns_window_history_teardown(self.0);
            ns_perf_teardown(self.0);
            ns_traversal_teardown(self.0);
            ns_services_teardown(self.0);
            ns_js_blob_urls_free(self.0);
            ns_canvas_states_teardown(self.0);
            ns_js_images_teardown(self.0);
            ns_dispatch_teardown_listeners(self.0);
            ns_js_net_teardown(self.0);
        }
    }

    pub fn stop_observer_tick(self) {
        let source = self.st().observer_tick_source.replace(0);
        if source != 0 {
            unsafe { g_source_remove(source) };
        }
    }

    pub fn teardown_loaders_and_attributes(self) {
        unsafe {
            ns_js_loader_teardown(self.0);
            ns_js_filereader_teardown(self.0);
            ns_attr_detach_all(self.0);
            ns_attribute_maps_teardown(self.0);
        }
    }

    pub fn free_pinned_set(self) {
        unsafe { ns_js_free_pinned_set(self.0) };
    }

    pub fn teardown_input_states(self) {
        unsafe {
            ns_storage_free_deferred_events(self.0);
            ns_focus_teardown(self.0);
            ns_js_input_teardown(self.0);
        }
    }

    pub fn free_realm_values(self) {
        unsafe { ns_js_free_realm_values(self.0) };
    }

    pub fn teardown_registries(self) {
        unsafe {
            ns_observers_teardown(self.0);
            ns_ce_teardown(self.0);
            ns_js_free_platform_globals(self.0);
            ns_storage_teardown(self.0);
            ns_media_teardown(self.0);
        }
    }

    pub fn free_partition(self) {
        unsafe { ns_js_free_partition(self.0) };
    }

    pub fn teardown_style_states(self) {
        unsafe {
            ns_collections_teardown(self.0);
            ns_cssom_teardown(self.0);
            ns_url_teardown(self.0);
        }
    }

    pub fn free_lookup_caches(self) {
        unsafe { ns_js_free_lookup_caches(self.0) };
    }

    pub fn teardown_windows(self) {
        unsafe {
            ns_css_set_fullscreen_node(ptr::null());
            ns_js_frames_teardown(self.0);
            ns_window_links_clear(self.0, glib::TRUE);
        }
        crate::scheduling::drop_message_tasks(self);
        unsafe {
            ns_js_clear_frame_clocks(self.0, glib::TRUE);
        }
    }

    pub fn free_window_values(self) {
        unsafe { ns_js_free_window_values(self.0) };
    }

    pub fn teardown_dispatch(self) {
        unsafe {
            ns_js_drop_pending_rejections(self.0);
            ns_dispatch_teardown(self.0);
        }
    }

    pub fn free_engine(self) {
        let state = unsafe { ns_js_runtime_state(self.0) }.cast::<PageState>();
        unsafe { ns_js_free_engine(self.0) };
        if !state.is_null() {
            drop(unsafe { Box::from_raw(state) });
        }
    }
}

unsafe extern "C" fn interrupt_handler(_rt: *mut c_void, opaque: *mut c_void) -> c_int {
    if opaque.is_null() {
        return 0;
    }
    c_int::from(interrupt::should_interrupt(Js(opaque.cast())))
}

unsafe extern "C" fn engine_name(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, argc, argv, |scope, _this, _args| {
            Ok(scope.string("quickjs"))
        })
    }
}

unsafe extern "C" fn is_engine_function(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, argc, argv, |_scope, _this, args| {
            Ok(Value::boolean(
                args.first().is_some_and(quickjs::is_engine_function),
            ))
        })
    }
}

pub(crate) fn engine_name_native(scope: &mut Scope<'_>) -> Value {
    quickjs::c_function(scope, "__jsEngine", 0, engine_name)
}

pub(crate) fn is_engine_function_native(scope: &mut Scope<'_>) -> Value {
    quickjs::c_function(scope, "__ndIsEngineFunction", 1, is_engine_function)
}

pub(crate) fn c_function(scope: &mut Scope<'_>, name: &str, arity: u32, native: Native) -> Value {
    quickjs::c_function(scope, name, arity, native)
}

pub(crate) fn c_constructor_or_function(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    quickjs::c_constructor_or_function(scope, name, arity, native)
}

pub(crate) fn c_constructor(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    quickjs::c_constructor(scope, name, arity, native)
}

pub(crate) fn install(scope: &mut Scope<'_>, installer: Installer, target: &Value) {
    unsafe { installer(quickjs::raw_context(scope), quickjs::raw(target)) };
}

pub(crate) fn install_context(
    scope: &mut Scope<'_>,
    installer: unsafe extern "C" fn(*mut JSContext),
) {
    unsafe { installer(quickjs::raw_context(scope)) };
}

pub(crate) fn add_engine_private_name(scope: &mut Scope<'_>, name: &str) {
    quickjs::add_engine_private_name(scope, name);
}

pub(crate) fn register_class(scope: &mut Scope<'_>, class: Class) -> Value {
    let raw = unsafe { ns_js_register_class(quickjs::raw_context(scope), class as c_int) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn new_window_named(scope: &mut Scope<'_>) -> Option<Value> {
    let raw = unsafe { ns_js_new_window_named(quickjs::raw_context(scope)) };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value).ok()
}

pub(crate) fn install_element_proto_table(scope: &mut Scope<'_>, proto: &Value) {
    unsafe {
        ns_js_install_proto_table(
            quickjs::raw_context(scope),
            quickjs::raw(proto),
            PROTO_TABLE_ELEMENT,
        )
    };
}

pub(crate) fn live_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    let raw = unsafe { ns_live_proto(quickjs::raw_context(scope), which) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn window_navigator(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_services_window_navigator(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn worker_navigator(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_services_worker_navigator(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn chrome_compat() -> bool {
    unsafe { ns_services_chrome_compat() != 0 }
}

pub(crate) fn supported_entry_types(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_perf_supported_entry_types(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn performance_extend_event_target(
    scope: &mut Scope<'_>,
    global: &Value,
    performance: &Value,
) {
    unsafe {
        ns_performance_extend_event_target(
            quickjs::raw_context(scope),
            quickjs::raw(global),
            quickjs::raw(performance),
        )
    };
}

pub(crate) fn register_canvas_classes(scope: &mut Scope<'_>) {
    unsafe { ns_canvas_register_classes(quickjs::runtime(scope)) };
}

pub(crate) fn install_canvas(scope: &mut Scope<'_>, global: &Value, window: bool) {
    unsafe {
        ns_canvas_install(
            quickjs::raw_context(scope),
            quickjs::raw(global),
            GBoolean::from(window),
        )
    };
}

pub(crate) fn install_worker_location(scope: &mut Scope<'_>, global: &Value, url: Option<&CStr>) {
    unsafe {
        ns_worker_install_location(
            quickjs::raw_context(scope),
            quickjs::raw(global),
            url.map_or(ptr::null(), CStr::as_ptr),
        )
    };
}

pub(crate) fn shape_worker_global(scope: &mut Scope<'_>, service_worker: bool) {
    unsafe { ns_worker_shape_global(quickjs::raw_context(scope), GBoolean::from(service_worker)) };
}

pub(crate) fn worker_compile(
    scope: &mut Scope<'_>,
    source: &CStr,
    len: usize,
    url: &CStr,
    module: bool,
) -> (Result<Value, Value>, bool) {
    let mut parse_error: GBoolean = glib::FALSE;
    let raw = unsafe {
        ns_worker_compile(
            quickjs::raw_context(scope),
            source.as_ptr(),
            len,
            url.as_ptr(),
            GBoolean::from(module),
            &mut parse_error,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    (quickjs::checked(scope, value), parse_error != 0)
}

pub(crate) fn coarsen_navigation_timing(timing: &mut NavigationTiming) {
    unsafe { ns_perf_coarsen_navigation_timing(timing) };
}

pub(crate) fn default_main_context() -> *mut GMainContext {
    unsafe { g_main_context_default() }
}

pub(crate) fn set_active(js: Js) {
    unsafe { ns_set_active_js(js.0) };
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn real_time_us() -> i64 {
    unsafe { g_get_real_time() }
}

pub(crate) fn warn(message: &str) {
    let message = glib::strdup(message.as_bytes());
    unsafe {
        g_log(ptr::null(), LOG_LEVEL_WARNING, c"%s".as_ptr(), message);
        glib::g_free(message.cast());
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn thread_stack_size() -> usize {
    unsafe { pthread_get_stacksize_np(pthread_self()) }
}

#[cfg(windows)]
pub(crate) fn thread_stack_size() -> usize {
    1024 * 1024
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn thread_stack_size() -> usize {
    0
}

unsafe fn c_text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new(
    log_cb: Option<LogFn>,
    log_user_data: *mut c_void,
    mut_cb: Option<DataFn>,
    mut_user_data: *mut c_void,
    nav_cb: Option<NavigateFn>,
    nav_user_data: *mut c_void,
    navigation_timing: *const NavigationTiming,
) -> *mut NsJs {
    let timing = unsafe { navigation_timing.as_ref() }.copied();
    let callbacks = Callbacks {
        log: log_cb,
        log_data: log_user_data,
        mutated: mut_cb,
        mutated_data: mut_user_data,
        navigate: nav_cb,
        navigate_data: nav_user_data,
    };
    let js = Js::alloc();
    if page::create(js, timing, callbacks) {
        js.0
    } else {
        ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_js_new(realm: *const WorkerRealm) -> *mut NsJs {
    let Some(realm) = (unsafe { realm.as_ref() }) else {
        return ptr::null_mut();
    };
    let setup = WorkerSetup {
        host: realm.host,
        context: realm.context,
        url: unsafe { c_text(realm.url) },
        base_url: unsafe { c_text(realm.base_url) },
        origin: unsafe { c_text(realm.origin) },
        name: unsafe { c_text(realm.name) },
        service_worker: realm.is_service_worker != 0,
    };
    let js = Js::alloc();
    if worker::create(js, &setup) {
        js.0
    } else {
        ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_js_eval(
    js: *mut NsJs,
    src: *const c_char,
    len: usize,
    url: *const c_char,
    module: GBoolean,
    exception: *mut JSValue,
) -> c_int {
    unsafe { *exception = quickjs::UNDEFINED };
    if js.is_null() {
        return 2;
    }
    let source = unsafe { c_text(src) }.unwrap_or(c"");
    let url = unsafe { c_text(url) }.unwrap_or(c"<worker>");
    let status = Js(js).scope(|scope| worker::eval(scope, source, len, url, module != 0));
    match status {
        None => 2,
        Some(EvalStatus::Done) => 0,
        Some(EvalStatus::ParseError(thrown)) => {
            unsafe { *exception = quickjs::into_raw(thrown) };
            1
        }
        Some(EvalStatus::Thrown(thrown)) => {
            unsafe { *exception = quickjs::into_raw(thrown) };
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_release_pinned_wrappers(js: *mut NsJs) {
    if !js.is_null() {
        teardown::release_pinned_wrappers(Js(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free(js: *mut NsJs) {
    if !js.is_null() {
        teardown::free(Js(js));
    }
}
