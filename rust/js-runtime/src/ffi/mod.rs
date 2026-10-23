//! Southstar — the page record's handle (Js), realm creation and teardown, and their C ABI.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod exports;
pub(crate) mod glue;
pub(crate) mod imports;
pub(crate) mod record;
pub(crate) mod scopes;
pub(crate) mod state;
pub(crate) mod wrapper;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_engine::{ContextHandle, RuntimeHandle, RuntimeOptions, Scope, Value};

use crate::worker;
use crate::{Element, api, interrupt, page, teardown};
use state::{DataFn, LogFn, NavigateFn, PageState};

pub use record::NsJs;

#[repr(C)]
pub(crate) struct WorkerHost {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct GMainContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NavigationTiming {
    pub origin_us: i64,
    pub origin_real_ms: f64,
    pub domain_lookup_start_ms: f64,
    pub domain_lookup_end_ms: f64,
    pub connect_start_ms: f64,
    pub connect_end_ms: f64,
    pub secure_connection_start_ms: f64,
    pub request_start_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
    pub dom_loading_ms: f64,
    pub dom_interactive_ms: f64,
    pub dom_content_loaded_event_start_ms: f64,
    pub dom_content_loaded_event_end_ms: f64,
    pub dom_complete_ms: f64,
    pub load_event_start_ms: f64,
    pub load_event_end_ms: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<NavigationTiming>() == 136);

impl NavigationTiming {
    pub fn starting_at(origin_us: i64, origin_real_ms: f64) -> NavigationTiming {
        NavigationTiming {
            origin_us,
            origin_real_ms,
            ..NavigationTiming::default()
        }
    }
}

pub(crate) struct WorkerSetup<'a> {
    pub host: *mut WorkerHost,
    pub context: *mut GMainContext,
    pub url: Option<&'a CStr>,
    pub base_url: Option<&'a CStr>,
    pub origin: Option<&'a CStr>,
    pub name: Option<&'a CStr>,
    pub service_worker: bool,
}

#[derive(Clone, Copy)]
pub struct Callbacks {
    pub log: Option<LogFn>,
    pub log_data: *mut c_void,
    pub mutated: Option<DataFn>,
    pub mutated_data: *mut c_void,
    pub navigate: Option<NavigateFn>,
    pub navigate_data: *mut c_void,
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
    fn g_main_context_default() -> *mut GMainContext;
    fn g_source_remove(id: c_uint) -> GBoolean;

    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadStackLimits(low_limit: *mut usize, high_limit: *mut usize);
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
        let js = Js(record::alloc());
        let slots = unsafe { record::slots_of(js.0) };
        let state = Box::into_raw(Box::new(PageState::new(slots)));
        unsafe { record::set_runtime_state(js.0, state.cast()) };
        js
    }

    pub(crate) fn of(js: *const NsJs) -> Option<Js> {
        (!js.is_null()).then_some(Js(js.cast_mut()))
    }

    pub(crate) fn of_scope(scope: &Scope<'_>) -> Option<Js> {
        Js::of(scope.context_opaque().cast())
    }

    pub(crate) fn ptr(self) -> *mut NsJs {
        self.0
    }

    pub(crate) fn st(self) -> &'static PageState {
        unsafe { &*record::runtime_state(self.0).cast::<PageState>() }
    }

    pub fn ctx(self) -> ContextHandle {
        unsafe { *self.st().slots.ctx }
    }

    pub fn set_ctx(self, ctx: ContextHandle) {
        unsafe { *self.st().slots.ctx = ctx };
    }

    pub fn main_realm(self) -> ContextHandle {
        unsafe { *self.st().slots.main_realm_ctx }
    }

    pub fn main_or_current(self) -> ContextHandle {
        self.main_realm().or(self.ctx())
    }

    pub fn rt(self) -> RuntimeHandle {
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
        self.ctx().with(f)
    }

    pub fn set_navigation_timing(self, timing: NavigationTiming) {
        record::set_navigation_timing(self.0, timing);
        self.set_time_origin(timing.origin_us, timing.origin_real_ms);
    }

    pub fn set_time_origin(self, origin_us: i64, origin_real_ms: f64) {
        unsafe { record::set_time_origin(self.0, origin_us, origin_real_ms) };
    }

    pub fn perf_init(self) {
        (crate::api::perf_hooks().init)(self.0);
    }

    pub fn perf_teardown(self) {
        (crate::api::perf_hooks().teardown)(self.0);
    }

    pub fn create_runtime(
        self,
        memory_limit: usize,
        stack_size: usize,
        tracker: RejectionTracker,
    ) -> bool {
        let tracker: southstar_js_engine::RejectionTrackerFn = match tracker {
            RejectionTracker::Page => imports::nsr_js_promise_rejection_tracker,
            RejectionTracker::Worker => imports::nsr_worker_promise_rejection_tracker,
        };
        let options = RuntimeOptions {
            memory_limit,
            stack_size,
            interrupt: Some(interrupt_handler),
            interrupt_opaque: self.0.cast(),
            rejection_tracker: Some(tracker),
            compile_strings: Some(compile_strings_allowed),
            module_normalize: Some(imports::nsr_js_module_normalize),
            module_load: Some(imports::nsr_js_module_load),
            ..RuntimeOptions::default()
        };
        unsafe { record::create_runtime(self.0, &options) }
    }

    pub fn create_context(self) -> bool {
        unsafe { record::create_context(self.0) }
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
        unsafe { record::init_tables(self.0, page) };
    }

    pub fn init_page_states(self) {
        (api::mutation_hooks().init)(self.0);
        (api::storage_hooks().init)(self.0);
        (api::media_hooks().init)(self.0);
    }

    pub fn adopt_worker_host(self, setup: &WorkerSetup<'_>) {
        let partition = setup
            .origin
            .filter(|origin| !origin.is_empty())
            .map_or(ptr::null(), CStr::as_ptr);
        self.st().worker_host.set(setup.host);
        unsafe { record::set_partition_key(self.0, partition) };
        self.set_callbacks(Callbacks {
            log: Some(worker_log),
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
        // SAFETY: a worker page's host stays alive for as long as the page, which is the host_closing hook's obligation.
        !host.is_null() && unsafe { (api::workers_hooks().host_closing)(host.cast()) }
    }

    pub fn set_proto_document(self, proto: Value) {
        unsafe { record::set_proto_document(self.0, proto) };
    }

    pub fn set_navigator_brand(self, brand: Value) {
        unsafe { record::set_navigator_brand(self.0, brand) };
    }

    pub fn set_pristine_promise(self, promise: Value) {
        unsafe { record::set_pristine_promise(self.0, promise) };
    }

    pub fn make_performance_object(self, scope: &mut Scope<'_>, include_memory: bool) -> Value {
        (crate::api::perf_hooks().make_performance_object)(scope, self.0, include_memory)
    }

    pub fn sync_window_metrics(self) {
        (api::window_hooks().sync_metrics)(self.0);
    }

    pub fn install_window_state(self, scope: &mut Scope<'_>, global: &Value) {
        unsafe { imports::nsr_window_install_state(self.0, scope, global) };
    }

    #[cfg(feature = "webgpu")]
    pub fn install_webgpu(self, scope: &mut Scope<'_>, navigator: &Value) {
        imports::nsr_webgpu_install(scope, navigator);
    }

    pub fn eval_worker_polyfills(self) {
        record::eval_worker_polyfills(self.0);
    }

    pub fn is_active(self) -> bool {
        record::active() == self.0
    }

    pub fn clear_active(self) {
        record::set_active(ptr::null_mut());
    }

    pub fn take_pinned_wrappers(self) -> Vec<Element> {
        let set = unsafe { record::pinned_wrappers(self.0) };
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
        unsafe { record::detach_wrapper(self.0, node.as_mut_ptr(), release) };
    }

    pub fn current_document(self) -> Option<*const NsNode> {
        let doc = self.current_doc();
        (!doc.is_null()).then_some(doc.cast_const())
    }

    pub fn orphans_snapshot(self) -> Option<Vec<Element>> {
        (api::mutation_hooks().orphans_snapshot)(self.0)
    }

    pub fn orphan_tracked(self, node: Element) -> bool {
        (api::mutation_hooks().orphan_tracked)(self.0, node)
    }

    pub fn teardown_mutation(self) {
        (api::mutation_hooks().teardown)(self.0);
    }

    pub fn free_node(self, node: Element) {
        node.free_tree();
    }

    pub fn free_doc_ready_states(self) {
        self.st().doc_ready_states.borrow_mut().clear();
    }

    pub fn teardown_document_states(self) {
        (api::frames_hooks().clear_initial_blank)(self.0);
        (api::top_layer_hooks().clear)(self.0);
        (api::fonts_hooks().teardown)(self.0);
        (api::lifecycle_hooks().teardown)(self.0);
        (api::url_hooks().blob_registry_remove)(self.0);
        (api::storage_hooks().flush)(self.0);
    }

    pub fn free_page_strings(self) {
        unsafe { record::free_page_strings(self.0) };
    }

    pub fn teardown_page_services(self) {
        (api::mutation_hooks().forget_pending_change)(self.0);
        (api::document_hooks().write_teardown)(self.0);
        (api::workers_hooks().teardown)(self.0);
        (api::window_hooks().history_teardown)(self.0);
        self.perf_teardown();
        (api::traversal_hooks().teardown)(self.0);
        (api::services_hooks().teardown)(self.0);
        (api::url_hooks().blob_urls_free)(self.0);
        (api::canvas_hooks().teardown)(self.0);
        (api::images_hooks().teardown)(self.0);
        (api::dispatch_hooks().teardown_listeners)(self.0);
        (api::net_hooks().teardown)(self.0);
    }

    pub fn stop_observer_tick(self) {
        let source = self.st().observer_tick_source.replace(0);
        if source != 0 {
            unsafe { g_source_remove(source) };
        }
    }

    pub fn teardown_loaders_and_attributes(self) {
        (api::loader_hooks().teardown)(self.0);
        (api::url_hooks().file_reader_teardown)(self.0);
        (api::attrs_hooks().detach_all)(self.0);
        (api::mutation_hooks().attribute_maps_teardown)(self.0);
    }

    pub fn free_pinned_set(self) {
        unsafe { record::free_pinned_set(self.0) };
    }

    pub fn teardown_input_states(self) {
        (api::storage_hooks().free_deferred_events)(self.0);
        (api::focus_hooks().teardown)(self.0);
        (api::input_hooks().teardown)(self.0);
    }

    pub fn free_realm_values(self) {
        unsafe { record::free_realm_values(self.0) };
    }

    pub fn teardown_registries(self) {
        (api::observers_hooks().teardown)(self.0);
        (api::custom_elements_hooks().teardown)(self.0);
        // SAFETY: the page is live until free_engine, which runs after this teardown phase.
        unsafe { record::free_platform_globals(self.0) };
        (api::storage_hooks().teardown)(self.0);
        (api::media_hooks().teardown)(self.0);
    }

    pub fn free_partition(self) {
        unsafe { record::free_partition(self.0) };
    }

    pub fn teardown_style_states(self) {
        (api::collections_hooks().teardown)(self.0);
        (api::cssom_hooks().teardown)(self.0);
        (api::url_hooks().teardown)(self.0);
    }

    pub fn teardown_windows(self) {
        southstar_css::set_fullscreen_node(None);
        (api::frames_hooks().teardown)(self.0);
        (api::window_hooks().links_clear)(self.0);
        crate::scheduling::drop_message_tasks(self);
        (crate::api::perf_hooks().clear_frame_clocks)(self.0, true);
    }

    pub fn free_window_values(self) {
        unsafe { record::free_window_values(self.0) };
    }

    pub fn teardown_dispatch(self) {
        (api::dispatch_hooks().drop_pending_rejections)(self.0);
        (api::dispatch_hooks().teardown)(self.0);
    }

    pub fn free_engine(self) {
        let state = unsafe { record::runtime_state(self.0) }.cast::<PageState>();
        unsafe { record::free_engine(self.0) };
        if !state.is_null() {
            drop(unsafe { Box::from_raw(state) });
        }
    }
}

fn interrupt_handler(opaque: *mut c_void) -> bool {
    Js::of(opaque.cast()).is_some_and(interrupt::should_interrupt)
}

fn compile_strings_allowed(
    scope: &mut Scope<'_>,
    caller: ContextHandle,
    callee: ContextHandle,
) -> bool {
    if caller == callee {
        return true;
    }
    Js::of_scope(scope)
        .is_none_or(|js| (crate::api::frames_hooks().realms_same_origin)(js.ptr(), caller, callee))
}

fn engine_name(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let name = southstar_js_engine::ENGINE_NAME;
    Ok(scope.string(name.split('-').next().unwrap_or(name)))
}

fn is_engine_function(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let engine = args
        .first()
        .is_some_and(|value| scope.is_engine_function(value));
    Ok(Value::boolean(engine))
}

pub(crate) fn engine_name_native(scope: &mut Scope<'_>) -> Value {
    scope.function("__jsEngine", 0, engine_name)
}

pub(crate) fn is_engine_function_native(scope: &mut Scope<'_>) -> Value {
    scope.function("__ndIsEngineFunction", 1, is_engine_function)
}

pub(crate) fn add_engine_private_name(scope: &mut Scope<'_>, name: &str) {
    scope.add_engine_private_name(name);
}

pub(crate) fn register_class(scope: &mut Scope<'_>, class: Class) -> Value {
    imports::nsr_js_register_class(scope, class as c_int)
}

pub(crate) fn new_window_named(scope: &mut Scope<'_>) -> Option<Value> {
    imports::nsr_js_new_window_named(scope).ok()
}

pub(crate) fn install_element_proto_table(scope: &mut Scope<'_>, proto: &Value) {
    imports::nsr_js_install_proto_table(scope, proto, PROTO_TABLE_ELEMENT);
}

pub(crate) fn live_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    imports::nsr_live_proto(scope, which)
}

pub(crate) fn window_navigator(scope: &mut Scope<'_>) -> Value {
    imports::nsr_services_window_navigator(scope)
}

pub(crate) fn worker_navigator(scope: &mut Scope<'_>) -> Value {
    imports::nsr_services_worker_navigator(scope)
}

pub(crate) fn chrome_compat() -> bool {
    (api::services_hooks().chrome_compat)()
}

pub(crate) fn supported_entry_types(scope: &mut Scope<'_>) -> Value {
    (crate::api::perf_hooks().supported_entry_types)(scope)
}

pub(crate) fn performance_extend_event_target(
    scope: &mut Scope<'_>,
    global: &Value,
    performance: &Value,
) {
    (crate::api::perf_hooks().extend_event_target)(scope, global, performance);
}

pub(crate) fn install_canvas(scope: &mut Scope<'_>, global: &Value, window: bool) {
    imports::nsr_canvas_install(scope, global, window);
}

pub(crate) fn install_webgl(scope: &mut Scope<'_>, global: &Value) {
    imports::nsr_webgl_install(scope, global);
}

pub(crate) fn install_worker_location(scope: &mut Scope<'_>, global: &Value, url: Option<&CStr>) {
    let url = url.map_or_else(String::new, |url| url.to_string_lossy().into_owned());
    imports::nsr_worker_install_location(scope, global, &url);
}

pub(crate) fn shape_worker_global(scope: &mut Scope<'_>, service_worker: bool) {
    (api::workers_hooks().shape_global)(scope, service_worker);
}

unsafe extern "C" fn worker_log(line: *const c_char, host: *mut c_void) {
    let Some(line) = glue::text(line) else {
        return;
    };
    // SAFETY: the log data of a worker page is its worker host, alive as long as the page.
    unsafe { (api::workers_hooks().log_line)(host, line) };
}

pub(crate) fn worker_compile(
    scope: &mut Scope<'_>,
    source: &CStr,
    len: usize,
    url: &CStr,
    module: bool,
) -> (Result<Value, Value>, bool) {
    unsafe { record::worker_compile(scope, source, len, url, module) }
}

pub(crate) fn coarsen_navigation_timing(timing: &mut NavigationTiming) {
    (crate::api::perf_hooks().coarsen_navigation_timing)(timing);
}

pub(crate) fn default_main_context() -> *mut GMainContext {
    unsafe { g_main_context_default() }
}

pub(crate) fn set_active(js: Js) {
    record::set_active(js.0);
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
    let mut low = 0usize;
    let mut high = 0usize;
    // SAFETY: both arguments point to live, writable usize locals.
    unsafe { GetCurrentThreadStackLimits(&raw mut low, &raw mut high) };
    high.saturating_sub(low)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn thread_stack_size() -> usize {
    0
}

pub(crate) fn new_page(callbacks: Callbacks, timing: Option<NavigationTiming>) -> *mut NsJs {
    let js = Js::alloc();
    if page::create(js, timing, callbacks) {
        js.0
    } else {
        ptr::null_mut()
    }
}

pub(crate) fn new_worker(setup: &WorkerSetup<'_>) -> *mut NsJs {
    let js = Js::alloc();
    if worker::create(js, setup) {
        js.0
    } else {
        ptr::null_mut()
    }
}

pub(crate) fn free(js: *const NsJs) {
    if let Some(js) = Js::of(js) {
        teardown::free(js);
    }
}
