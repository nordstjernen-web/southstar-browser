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

use core::ffi::{CStr, c_int, c_void};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::NsNode;
use southstar_glib::MainContext;
use southstar_js_engine::{ContextHandle, RuntimeHandle, RuntimeOptions, Scope, Value};

use crate::api::Page;
use crate::worker;
use crate::{Element, api, interrupt, page, teardown};
use state::{DataFn, LogFn, NavigateFn, PageState};

pub use record::NsJs;

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
    pub host: api::WorkerHost,
    pub context: MainContext,
    pub url: Option<&'a CStr>,
    pub base_url: Option<&'a CStr>,
    pub origin: Option<&'a CStr>,
    pub name: Option<&'a CStr>,
    pub service_worker: bool,
}

#[derive(Clone, Default)]
pub struct Callbacks {
    pub log: Option<Rc<LogFn>>,
    pub mutated: Option<Rc<DataFn>>,
    pub navigate: Option<Rc<NavigateFn>>,
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
pub(crate) struct Js(Page);

impl Js {
    fn alloc() -> Js {
        Js(record::alloc())
    }

    pub(crate) fn rec(self) -> &'static record::Record {
        record::record(Some(self.0)).expect("a Js handle names a live page")
    }

    pub(crate) fn of(js: Option<Page>) -> Option<Js> {
        js.map(Js)
    }

    pub(crate) fn of_raw(js: *const c_void) -> Option<Js> {
        Page::from_ptr(js.cast()).map(Js)
    }

    pub(crate) fn of_scope(scope: &Scope<'_>) -> Option<Js> {
        Page::current(scope).map(Js)
    }

    pub(crate) fn page(self) -> Page {
        self.0
    }

    pub(crate) fn some(self) -> Option<Page> {
        Some(self.0)
    }

    pub(crate) fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast_mut().cast()
    }

    pub(crate) fn st(self) -> &'static PageState {
        self.rec().state()
    }

    pub fn ctx(self) -> ContextHandle {
        self.rec().ctx.get()
    }

    pub fn set_ctx(self, ctx: ContextHandle) {
        self.rec().ctx.set(ctx);
    }

    pub fn main_realm(self) -> ContextHandle {
        self.rec().main_realm_ctx.get()
    }

    pub fn main_or_current(self) -> ContextHandle {
        self.main_realm().or(self.ctx())
    }

    pub fn rt(self) -> RuntimeHandle {
        self.rec().rt.get()
    }

    pub fn current_url(self) -> Option<CString> {
        self.st().current_url.borrow().clone()
    }

    pub fn replace_current_url(self, url: Option<CString>) -> Option<CString> {
        self.st().current_url.replace(url)
    }

    pub fn current_doc(self) -> *mut NsNode {
        self.rec().current_doc.get()
    }

    pub fn set_current_doc(self, doc: *mut NsNode) {
        self.rec().current_doc.set(doc);
    }

    pub fn frame(self) -> *mut NsNode {
        self.rec().raf_frame_ctx.get()
    }

    pub fn set_frame(self, frame: *mut NsNode) {
        self.rec().raf_frame_ctx.set(frame);
    }

    pub fn ce_main_doc(self) -> *const NsNode {
        self.rec().ce_main_doc.get()
    }

    pub fn set_ce_main_doc(self, doc: *const NsNode) {
        self.rec().ce_main_doc.set(doc);
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn set_navigation_timing(self, timing: NavigationTiming) {
        record::set_navigation_timing(self.some(), timing);
        self.set_time_origin(timing.origin_us, timing.origin_real_ms);
    }

    pub fn set_time_origin(self, origin_us: i64, origin_real_ms: f64) {
        record::set_time_origin(self.rec(), origin_us, origin_real_ms);
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
            interrupt_opaque: self.raw(),
            rejection_tracker: Some(tracker),
            compile_strings: Some(compile_strings_allowed),
            module_normalize: Some(imports::nsr_js_module_normalize),
            module_load: Some(imports::nsr_js_module_load),
            ..RuntimeOptions::default()
        };
        record::create_runtime(self.rec(), &options)
    }

    pub fn create_context(self) -> bool {
        record::create_context(self.0, self.rec())
    }

    pub fn set_callbacks(self, callbacks: Callbacks) {
        let state = self.st();
        state.log.set(callbacks.log);
        state.mutated_hook.set(callbacks.mutated);
        state.navigate.set(callbacks.navigate);
    }

    pub fn init_tables(self, main_context: MainContext, page: bool) {
        *self.st().main_context.borrow_mut() = Some(main_context);
        record::init_tables(self.rec(), page);
    }

    pub fn init_page_states(self) {
        (api::mutation_hooks().init)(self.0);
        (api::storage_hooks().init)(self.0);
        (api::media_hooks().init)(self.0);
    }

    pub fn adopt_worker_host(self, setup: &WorkerSetup<'_>) {
        let partition = setup.origin.filter(|origin| !origin.is_empty());
        *self.st().worker_host.borrow_mut() = Some(setup.host.clone());
        record::set_partition_key(self.rec(), partition);
        let host = setup.host.clone();
        self.set_callbacks(Callbacks {
            log: Some(Rc::new(move |line: &CStr| {
                (api::workers_hooks().log_line)(&host, line);
            })),
            ..Callbacks::default()
        });
        self.init_tables(setup.context.clone(), false);
    }

    pub fn halted(self) -> bool {
        self.rec().halted.get()
    }

    pub fn halt(self) {
        self.rec().halted.set(true);
    }

    pub fn worker_closing(self) -> bool {
        self.st()
            .worker_host
            .borrow()
            .as_ref()
            .is_some_and(api::workers_hooks().host_closing)
    }

    pub fn set_proto_document(self, proto: Value) {
        record::set_proto_document(self.rec(), proto);
    }

    pub fn set_navigator_brand(self, brand: Value) {
        record::set_navigator_brand(self.rec(), brand);
    }

    pub fn set_pristine_promise(self, promise: Value) {
        record::set_pristine_promise(self.rec(), promise);
    }

    pub fn make_performance_object(self, scope: &mut Scope<'_>, include_memory: bool) -> Value {
        (crate::api::perf_hooks().make_performance_object)(scope, self.0, include_memory)
    }

    pub fn sync_window_metrics(self) {
        (api::window_hooks().sync_metrics)(self.0);
    }

    pub fn install_window_state(self, scope: &mut Scope<'_>, global: &Value) {
        (api::window_hooks().install_state)(self.0, scope, global);
    }

    #[cfg(feature = "webgpu")]
    pub fn install_webgpu(self, scope: &mut Scope<'_>, navigator: &Value) {
        crate::links::webgpu_install(scope, navigator);
    }

    pub fn eval_worker_polyfills(self) {
        record::eval_worker_polyfills(self.some());
    }

    pub fn is_active(self) -> bool {
        record::active() == self.some()
    }

    pub fn clear_active(self) {
        record::set_active(None);
    }

    pub fn take_pinned_wrappers(self) -> Vec<Element> {
        record::take_pinned_wrappers(self.some())
            .into_iter()
            .filter_map(|key| record::node(key as *const NsNode))
            .collect()
    }

    pub fn detach_wrapper(self, node: Element, release: bool) {
        record::detach_wrapper(self.rec(), node, release);
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
        record::free_page_strings(self.rec());
        self.replace_current_url(None);
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
        if let Some(source) = self.st().observer_tick_source.take() {
            source.remove();
        }
    }

    pub fn teardown_loaders_and_attributes(self) {
        (api::loader_hooks().teardown)(self.0);
        (api::url_hooks().file_reader_teardown)(self.0);
        (api::attrs_hooks().detach_all)(self.0);
        (api::mutation_hooks().attribute_maps_teardown)(self.0);
    }

    pub fn free_pinned_set(self) {
        record::free_pinned_set(self.rec());
    }

    pub fn teardown_input_states(self) {
        (api::storage_hooks().free_deferred_events)(self.0);
        (api::focus_hooks().teardown)(self.0);
        (api::input_hooks().teardown)(self.0);
    }

    pub fn free_realm_values(self) {
        record::free_realm_values(self.rec());
    }

    pub fn teardown_registries(self) {
        (api::observers_hooks().teardown)(self.0);
        (api::custom_elements_hooks().teardown)(self.0);
        record::free_platform_globals(self.rec());
        (api::storage_hooks().teardown)(self.0);
        (api::media_hooks().teardown)(self.0);
    }

    pub fn free_partition(self) {
        record::free_partition(self.rec());
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
        record::free_window_values(self.rec());
    }

    pub fn teardown_dispatch(self) {
        (api::dispatch_hooks().drop_pending_rejections)(self.0);
        (api::dispatch_hooks().teardown)(self.0);
    }

    pub fn free_engine(self) {
        // SAFETY: teardown frees the engine last, and the page is not used afterwards.
        unsafe { record::free_engine(self.0) };
    }
}

fn interrupt_handler(opaque: *mut c_void) -> bool {
    Js::of_raw(opaque).is_some_and(interrupt::should_interrupt)
}

fn compile_strings_allowed(
    scope: &mut Scope<'_>,
    caller: ContextHandle,
    callee: ContextHandle,
) -> bool {
    let Some(js) = Js::of_scope(scope) else {
        return true;
    };
    if caller != callee
        && !(crate::api::frames_hooks().realms_same_origin)(js.page(), caller, callee)
    {
        return false;
    }
    let main = js.main_realm();
    (caller != main && callee != main)
        || scope.is_host_access()
        || record::with_page_csp(js.some(), |csp| {
            csp.is_none_or(southstar_csp::Csp::eval_allowed)
        })
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

pub(crate) fn worker_compile(
    scope: &mut Scope<'_>,
    source: &CStr,
    len: usize,
    url: &CStr,
    module: bool,
) -> (Result<Value, Value>, bool) {
    record::worker_compile(scope, source, len, url, module)
}

pub(crate) fn coarsen_navigation_timing(timing: &mut NavigationTiming) {
    (crate::api::perf_hooks().coarsen_navigation_timing)(timing);
}

pub(crate) fn set_active(js: Js) {
    record::set_active(js.some());
}

pub(crate) fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub(crate) fn real_time_us() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |before| -i64::try_from(before.duration().as_micros()).unwrap_or(i64::MAX),
            |after| i64::try_from(after.as_micros()).unwrap_or(i64::MAX),
        )
}

pub(crate) fn warn(message: &str) {
    southstar_glib::warning(message);
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

pub(crate) fn new_page(callbacks: Callbacks, timing: Option<NavigationTiming>) -> Option<Page> {
    let js = Js::alloc();
    page::create(js, timing, callbacks).then_some(js.0)
}

pub(crate) fn new_worker(setup: &WorkerSetup<'_>) -> Option<Page> {
    let js = Js::alloc();
    worker::create(js, setup).then_some(js.0)
}

pub(crate) fn free(js: Page) {
    teardown::free(Js(js));
}
