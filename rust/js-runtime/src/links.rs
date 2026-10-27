//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{ContextHandle, Scope, Value};

use core::ffi::c_int;
use southstar_dom::AttrView;

pub type JsCompileModuleCachedFn = fn(&mut Scope<'_>, &[u8], &[u8]) -> Result<Value, Value>;

pub struct CanvasLinks {
    pub canvas_install: fn(&mut Scope<'_>, &Value, bool),
}

static CANVAS_LINKS: OnceLock<CanvasLinks> = OnceLock::new();

pub fn set_canvas_links(links: CanvasLinks) {
    let _ = CANVAS_LINKS.set(links);
}

fn canvas_links() -> &'static CanvasLinks {
    CANVAS_LINKS
        .get()
        .expect("southstar-canvas installs its links before the first page")
}

pub struct IdbLinks {
    pub idb_install: fn(&mut Scope<'_>, &Value),
}

static IDB_LINKS: OnceLock<IdbLinks> = OnceLock::new();

pub fn set_idb_links(links: IdbLinks) {
    let _ = IDB_LINKS.set(links);
}

fn idb_links() -> &'static IdbLinks {
    IDB_LINKS
        .get()
        .expect("southstar-idb installs its links before the first page")
}

pub struct JsAttrsLinks {
    pub attr_to_js: fn(&mut Scope<'_>, &Value, AttrView<'_>, bool) -> Result<Value, Value>,
}

static JS_ATTRS_LINKS: OnceLock<JsAttrsLinks> = OnceLock::new();

pub fn set_js_attrs_links(links: JsAttrsLinks) {
    let _ = JS_ATTRS_LINKS.set(links);
}

fn js_attrs_links() -> &'static JsAttrsLinks {
    JS_ATTRS_LINKS
        .get()
        .expect("southstar-js-attrs installs its links before the first page")
}

pub struct JsCloneLinks {
    pub window_structured_clone: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_CLONE_LINKS: OnceLock<JsCloneLinks> = OnceLock::new();

pub fn set_js_clone_links(links: JsCloneLinks) {
    let _ = JS_CLONE_LINKS.set(links);
}

fn js_clone_links() -> &'static JsCloneLinks {
    JS_CLONE_LINKS
        .get()
        .expect("southstar-js-clone installs its links before the first page")
}

pub struct JsCollectionsLinks {
    pub live_proto: fn(&mut Scope<'_>, c_int) -> Value,
}

static JS_COLLECTIONS_LINKS: OnceLock<JsCollectionsLinks> = OnceLock::new();

pub fn set_js_collections_links(links: JsCollectionsLinks) {
    let _ = JS_COLLECTIONS_LINKS.set(links);
}

fn js_collections_links() -> &'static JsCollectionsLinks {
    JS_COLLECTIONS_LINKS
        .get()
        .expect("southstar-js-collections installs its links before the first page")
}

pub struct JsCssomLinks {
    pub cssom_install_css: fn(&mut Scope<'_>, &Value),
    pub cssom_install_window: fn(&mut Scope<'_>, &Value),
}

static JS_CSSOM_LINKS: OnceLock<JsCssomLinks> = OnceLock::new();

pub fn set_js_cssom_links(links: JsCssomLinks) {
    let _ = JS_CSSOM_LINKS.set(links);
}

fn js_cssom_links() -> &'static JsCssomLinks {
    JS_CSSOM_LINKS
        .get()
        .expect("southstar-js-cssom installs its links before the first page")
}

pub struct JsDispatchLinks {
    pub js_install_event_target: fn(&mut Scope<'_>, &Value),
    pub target_handler_realm: fn(&mut Scope<'_>, &Value, &str, &str) -> ContextHandle,
    pub js_promise_rejection_tracker: fn(&mut Scope<'_>, &Value, &Value, bool),
    pub install_body_reflected_handlers: fn(&mut Scope<'_>, &Value),
    pub window_report_error: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub target_dispatch_event: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_dispatch_event: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_remove_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_add_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub target_remove_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub target_add_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_DISPATCH_LINKS: OnceLock<JsDispatchLinks> = OnceLock::new();

pub fn set_js_dispatch_links(links: JsDispatchLinks) {
    let _ = JS_DISPATCH_LINKS.set(links);
}

fn js_dispatch_links() -> &'static JsDispatchLinks {
    JS_DISPATCH_LINKS
        .get()
        .expect("southstar-js-dispatch installs its links before the first page")
}

pub struct JsDomFactoryLinks {
    pub tag_owner_document: fn(&mut Scope<'_>, &Value, &Value),
    pub window_close_watcher_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub dom_parser_parse_from_string: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_dom_parser_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_option_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_comment_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_text_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_document_fragment_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_audio_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_image_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub attr_clone_node: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_DOM_FACTORY_LINKS: OnceLock<JsDomFactoryLinks> = OnceLock::new();

pub fn set_js_dom_factory_links(links: JsDomFactoryLinks) {
    let _ = JS_DOM_FACTORY_LINKS.set(links);
}

fn js_dom_factory_links() -> &'static JsDomFactoryLinks {
    JS_DOM_FACTORY_LINKS
        .get()
        .expect("southstar-js-dom-factory installs its links before the first page")
}

pub struct JsEventsLinks {
    pub make_abort_error: fn(&mut Scope<'_>) -> Value,
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
    pub install_event_attribute_getters: fn(&mut Scope<'_>, &Value),
    pub events_install_worker: fn(&mut Scope<'_>, &Value),
    pub events_install_window: fn(&mut Scope<'_>, &Value),
    pub events_install_window_base: fn(&mut Scope<'_>, &Value),
    pub window_event_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_EVENTS_LINKS: OnceLock<JsEventsLinks> = OnceLock::new();

pub fn set_js_events_links(links: JsEventsLinks) {
    let _ = JS_EVENTS_LINKS.set(links);
}

fn js_events_links() -> &'static JsEventsLinks {
    JS_EVENTS_LINKS
        .get()
        .expect("southstar-js-events installs its links before the first page")
}

pub struct JsFormsLinks {
    pub form_named_control: fn(&mut Scope<'_>, &Value, &[u8]) -> Result<Value, Value>,
    pub form_indexed_control: fn(&mut Scope<'_>, &Value, u32) -> Result<Value, Value>,
    pub radio_node_list_set_value: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub radio_node_list_get_value: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub custom_state_set_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_form_data_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_FORMS_LINKS: OnceLock<JsFormsLinks> = OnceLock::new();

pub fn set_js_forms_links(links: JsFormsLinks) {
    let _ = JS_FORMS_LINKS.set(links);
}

fn js_forms_links() -> &'static JsFormsLinks {
    JS_FORMS_LINKS
        .get()
        .expect("southstar-js-forms installs its links before the first page")
}

pub struct JsInputLinks {
    pub js_input_install_wpt: fn(&mut Scope<'_>, &Value),
}

static JS_INPUT_LINKS: OnceLock<JsInputLinks> = OnceLock::new();

pub fn set_js_input_links(links: JsInputLinks) {
    let _ = JS_INPUT_LINKS.set(links);
}

fn js_input_links() -> &'static JsInputLinks {
    JS_INPUT_LINKS
        .get()
        .expect("southstar-js-input installs its links before the first page")
}

pub struct JsInterfacesLinks {
    pub js_install_proto_table: fn(&mut Scope<'_>, &Value, c_int),
    pub install_window_compat: fn(&mut Scope<'_>, &Value),
    pub install_window_chrome: fn(&mut Scope<'_>, &Value),
    pub js_register_class: fn(&mut Scope<'_>, c_int) -> Value,
}

static JS_INTERFACES_LINKS: OnceLock<JsInterfacesLinks> = OnceLock::new();

pub fn set_js_interfaces_links(links: JsInterfacesLinks) {
    let _ = JS_INTERFACES_LINKS.set(links);
}

fn js_interfaces_links() -> &'static JsInterfacesLinks {
    JS_INTERFACES_LINKS
        .get()
        .expect("southstar-js-interfaces installs its links before the first page")
}

pub struct JsLifecycleLinks {
    pub window_cancel_animation_frame: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_request_animation_frame:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_LIFECYCLE_LINKS: OnceLock<JsLifecycleLinks> = OnceLock::new();

pub fn set_js_lifecycle_links(links: JsLifecycleLinks) {
    let _ = JS_LIFECYCLE_LINKS.set(links);
}

fn js_lifecycle_links() -> &'static JsLifecycleLinks {
    JS_LIFECYCLE_LINKS
        .get()
        .expect("southstar-js-lifecycle installs its links before the first page")
}

pub struct JsLoaderLinks {
    pub js_module_load: fn(&mut Scope<'_>, &str, &Value) -> Result<Value, Value>,
    pub js_module_normalize: fn(&mut Scope<'_>, &str, &str) -> Result<String, Value>,
    pub js_module_set_import_meta: fn(&mut Scope<'_>, &Value, bool) -> Result<(), Value>,
    pub js_compile_module_cached: JsCompileModuleCachedFn,
}

static JS_LOADER_LINKS: OnceLock<JsLoaderLinks> = OnceLock::new();

pub fn set_js_loader_links(links: JsLoaderLinks) {
    let _ = JS_LOADER_LINKS.set(links);
}

fn js_loader_links() -> &'static JsLoaderLinks {
    JS_LOADER_LINKS
        .get()
        .expect("southstar-js-loader installs its links before the first page")
}

pub struct JsMediaLinks {
    pub media_install_audio: fn(&mut Scope<'_>, &Value),
    pub vtt_cue_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub media_source_is_type_supported:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_bytes: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_remove: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_buffered_start: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_buffered: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_eos: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_mse_append: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_MEDIA_LINKS: OnceLock<JsMediaLinks> = OnceLock::new();

pub fn set_js_media_links(links: JsMediaLinks) {
    let _ = JS_MEDIA_LINKS.set(links);
}

fn js_media_links() -> &'static JsMediaLinks {
    JS_MEDIA_LINKS
        .get()
        .expect("southstar-js-media installs its links before the first page")
}

pub struct JsNetLinks {
    pub net_install_interfaces: fn(&mut Scope<'_>, &Value),
    pub js_net_install_sockets: fn(&mut Scope<'_>, &Value),
    pub fetch_install_interfaces: fn(&mut Scope<'_>, &Value),
    pub install_abort_signal_interface: fn(&mut Scope<'_>, &Value),
    pub xhr_install_interface: fn(&mut Scope<'_>, &Value),
    pub window_request_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_response_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_abort_controller_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_xhr_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub js_fetch: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_NET_LINKS: OnceLock<JsNetLinks> = OnceLock::new();

pub fn set_js_net_links(links: JsNetLinks) {
    let _ = JS_NET_LINKS.set(links);
}

fn js_net_links() -> &'static JsNetLinks {
    JS_NET_LINKS
        .get()
        .expect("southstar-js-net installs its links before the first page")
}

pub struct JsNodeLinks {
    pub element_is_same_node: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_NODE_LINKS: OnceLock<JsNodeLinks> = OnceLock::new();

pub fn set_js_node_links(links: JsNodeLinks) {
    let _ = JS_NODE_LINKS.set(links);
}

fn js_node_links() -> &'static JsNodeLinks {
    JS_NODE_LINKS
        .get()
        .expect("southstar-js-node installs its links before the first page")
}

pub struct JsServicesLinks {
    pub services_worker_navigator: fn(&mut Scope<'_>) -> Value,
    pub services_window_navigator: fn(&mut Scope<'_>) -> Value,
    pub install_idle_deadline: fn(&mut Scope<'_>, &Value),
    pub services_install_rtc: fn(&mut Scope<'_>, &Value),
    pub services_install_screen: fn(&mut Scope<'_>, &Value),
    pub services_install_clipboard_item: fn(&mut Scope<'_>, &Value),
    pub services_install_console: fn(&mut Scope<'_>, &Value),
    pub cache_open: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_notification_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_queue_microtask: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_request_idle_callback:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_match_media: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_prompt: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_confirm: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_camera_enumerate: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_camera_label: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_mic_release: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_camera_release: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_camera_request: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_clear_timer: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_set_interval: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_set_timeout: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub services_alert: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_SERVICES_LINKS: OnceLock<JsServicesLinks> = OnceLock::new();

pub fn set_js_services_links(links: JsServicesLinks) {
    let _ = JS_SERVICES_LINKS.set(links);
}

fn js_services_links() -> &'static JsServicesLinks {
    JS_SERVICES_LINKS
        .get()
        .expect("southstar-js-services installs its links before the first page")
}

pub struct JsStorageLinks {
    pub storage_install_window: fn(&mut Scope<'_>, &Value),
}

static JS_STORAGE_LINKS: OnceLock<JsStorageLinks> = OnceLock::new();

pub fn set_js_storage_links(links: JsStorageLinks) {
    let _ = JS_STORAGE_LINKS.set(links);
}

fn js_storage_links() -> &'static JsStorageLinks {
    JS_STORAGE_LINKS
        .get()
        .expect("southstar-js-storage installs its links before the first page")
}

pub struct JsTraversalLinks {
    pub window_get_selection: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub static_range_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_TRAVERSAL_LINKS: OnceLock<JsTraversalLinks> = OnceLock::new();

pub fn set_js_traversal_links(links: JsTraversalLinks) {
    let _ = JS_TRAVERSAL_LINKS.set(links);
}

fn js_traversal_links() -> &'static JsTraversalLinks {
    JS_TRAVERSAL_LINKS
        .get()
        .expect("southstar-js-traversal installs its links before the first page")
}

pub struct JsUrlLinks {
    pub net_install_text_codecs: fn(&mut Scope<'_>, &Value),
    pub usp_install_interface: fn(&mut Scope<'_>),
    pub url_install_interface: fn(&mut Scope<'_>),
    pub blob_bytes: fn(&mut Scope<'_>, &Value) -> Vec<u8>,
    pub window_filereader_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_text_decoder_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_text_encoder_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_usp_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_revoke_object: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_update_object: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_create_object: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_parse_static: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_can_parse: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_url_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_atob: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_btoa: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_URL_LINKS: OnceLock<JsUrlLinks> = OnceLock::new();

pub fn set_js_url_links(links: JsUrlLinks) {
    let _ = JS_URL_LINKS.set(links);
}

fn js_url_links() -> &'static JsUrlLinks {
    JS_URL_LINKS
        .get()
        .expect("southstar-js-url installs its links before the first page")
}

pub struct JsWindowLinks {
    pub window_open_method: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_install_history: fn(&mut Scope<'_>, &Value),
    pub window_install_browsing_context: fn(&mut Scope<'_>, &Value),
    pub window_install_actions: fn(&mut Scope<'_>, &Value),
    pub window_find: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_bind_post_message: fn(&mut Scope<'_>, &Value),
    pub js_new_window_named: fn(&mut Scope<'_>) -> Result<Value, Value>,
}

static JS_WINDOW_LINKS: OnceLock<JsWindowLinks> = OnceLock::new();

pub fn set_js_window_links(links: JsWindowLinks) {
    let _ = JS_WINDOW_LINKS.set(links);
}

fn js_window_links() -> &'static JsWindowLinks {
    JS_WINDOW_LINKS
        .get()
        .expect("southstar-js-window installs its links before the first page")
}

pub struct JsWorkersLinks {
    pub sw_install_scope: fn(&mut Scope<'_>, &Value),
    pub worker_install_console: fn(&mut Scope<'_>, &Value),
    pub worker_install_constructor: fn(&mut Scope<'_>, &Value),
    pub sw_install_container: fn(&mut Scope<'_>, &Value),
    pub worker_promise_rejection_tracker: fn(&mut Scope<'_>, &Value, &Value, bool),
    pub worker_install_location: fn(&mut Scope<'_>, &Value, &str),
    pub worker_performance_clear: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_performance_entries: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_performance_now: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_report_error: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_import_scripts: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_global_close: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub worker_global_post_message: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_broadcast_channel: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_message_channel: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_WORKERS_LINKS: OnceLock<JsWorkersLinks> = OnceLock::new();

pub fn set_js_workers_links(links: JsWorkersLinks) {
    let _ = JS_WORKERS_LINKS.set(links);
}

fn js_workers_links() -> &'static JsWorkersLinks {
    JS_WORKERS_LINKS
        .get()
        .expect("southstar-js-workers installs its links before the first page")
}

pub struct WebglLinks {
    pub webgl_install: fn(&mut Scope<'_>, &Value),
}

static WEBGL_LINKS: OnceLock<WebglLinks> = OnceLock::new();

pub fn set_webgl_links(links: WebglLinks) {
    let _ = WEBGL_LINKS.set(links);
}

fn webgl_links() -> &'static WebglLinks {
    WEBGL_LINKS
        .get()
        .expect("southstar-webgl installs its links before the first page")
}

pub(crate) fn nsr_canvas_install(scope: &mut Scope<'_>, global: &Value, window: bool) {
    (canvas_links().canvas_install)(scope, global, window)
}

pub(crate) fn nsr_idb_install(scope: &mut Scope<'_>, global: &Value) {
    (idb_links().idb_install)(scope, global)
}

pub(crate) fn nsr_attr_to_js(
    scope: &mut Scope<'_>,
    owner: &Value,
    attr: AttrView<'_>,
    include_base: bool,
) -> Result<Value, Value> {
    (js_attrs_links().attr_to_js)(scope, owner, attr, include_base)
}

pub(crate) fn nsr_window_structured_clone(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_clone_links().window_structured_clone)(scope, this, args)
}

pub(crate) fn nsr_live_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    (js_collections_links().live_proto)(scope, which)
}

pub(crate) fn nsr_cssom_install_css(scope: &mut Scope<'_>, global: &Value) {
    (js_cssom_links().cssom_install_css)(scope, global)
}

pub(crate) fn nsr_cssom_install_window(scope: &mut Scope<'_>, global: &Value) {
    (js_cssom_links().cssom_install_window)(scope, global)
}

pub(crate) fn nsr_js_install_event_target(scope: &mut Scope<'_>, global: &Value) {
    (js_dispatch_links().js_install_event_target)(scope, global)
}

pub(crate) fn nsr_target_handler_realm(
    scope: &mut Scope<'_>,
    obj: &Value,
    kind: &str,
    listener_key: &str,
) -> ContextHandle {
    (js_dispatch_links().target_handler_realm)(scope, obj, kind, listener_key)
}

pub(crate) fn nsr_js_promise_rejection_tracker(
    scope: &mut Scope<'_>,
    promise: &Value,
    reason: &Value,
    handled: bool,
) {
    (js_dispatch_links().js_promise_rejection_tracker)(scope, promise, reason, handled)
}

pub(crate) fn nsr_install_body_reflected_handlers(scope: &mut Scope<'_>, wrapper: &Value) {
    (js_dispatch_links().install_body_reflected_handlers)(scope, wrapper)
}

pub(crate) fn nsr_tag_owner_document(scope: &mut Scope<'_>, document: &Value, node: &Value) {
    (js_dom_factory_links().tag_owner_document)(scope, document, node)
}

pub(crate) fn nsr_make_abort_error(scope: &mut Scope<'_>) -> Value {
    (js_events_links().make_abort_error)(scope)
}

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

pub(crate) fn nsr_install_event_attribute_getters(scope: &mut Scope<'_>, global: &Value) {
    (js_events_links().install_event_attribute_getters)(scope, global)
}

pub(crate) fn nsr_events_install_worker(scope: &mut Scope<'_>, global: &Value) {
    (js_events_links().events_install_worker)(scope, global)
}

pub(crate) fn nsr_events_install_window(scope: &mut Scope<'_>, global: &Value) {
    (js_events_links().events_install_window)(scope, global)
}

pub(crate) fn nsr_events_install_window_base(scope: &mut Scope<'_>, global: &Value) {
    (js_events_links().events_install_window_base)(scope, global)
}

pub(crate) fn nsr_form_named_control(
    scope: &mut Scope<'_>,
    form: &Value,
    name: &[u8],
) -> Result<Value, Value> {
    (js_forms_links().form_named_control)(scope, form, name)
}

pub(crate) fn nsr_form_indexed_control(
    scope: &mut Scope<'_>,
    form: &Value,
    index: u32,
) -> Result<Value, Value> {
    (js_forms_links().form_indexed_control)(scope, form, index)
}

pub(crate) fn nsr_js_input_install_wpt(scope: &mut Scope<'_>, global: &Value) {
    (js_input_links().js_input_install_wpt)(scope, global)
}

pub(crate) fn nsr_js_install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: c_int) {
    (js_interfaces_links().js_install_proto_table)(scope, proto, table)
}

pub(crate) fn nsr_install_window_compat(scope: &mut Scope<'_>, global: &Value) {
    (js_interfaces_links().install_window_compat)(scope, global)
}

pub(crate) fn nsr_install_window_chrome(scope: &mut Scope<'_>, global: &Value) {
    (js_interfaces_links().install_window_chrome)(scope, global)
}

pub(crate) fn nsr_js_register_class(scope: &mut Scope<'_>, which: c_int) -> Value {
    (js_interfaces_links().js_register_class)(scope, which)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_cancelAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_lifecycle_links().window_cancel_animation_frame)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_requestAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_lifecycle_links().window_request_animation_frame)(scope, this, args)
}

pub(crate) fn nsr_js_module_load(
    scope: &mut Scope<'_>,
    name: &str,
    attributes: &Value,
) -> Result<Value, Value> {
    (js_loader_links().js_module_load)(scope, name, attributes)
}

pub(crate) fn nsr_js_module_normalize(
    scope: &mut Scope<'_>,
    base: &str,
    name: &str,
) -> Result<String, Value> {
    (js_loader_links().js_module_normalize)(scope, base, name)
}

pub(crate) fn nsr_js_module_set_import_meta(
    scope: &mut Scope<'_>,
    module: &Value,
    is_main: bool,
) -> Result<(), Value> {
    (js_loader_links().js_module_set_import_meta)(scope, module, is_main)
}

pub(crate) fn nsr_js_compile_module_cached(
    scope: &mut Scope<'_>,
    src: &[u8],
    name: &[u8],
) -> Result<Value, Value> {
    (js_loader_links().js_compile_module_cached)(scope, src, name)
}

pub(crate) fn nsr_media_install_audio(scope: &mut Scope<'_>, global: &Value) {
    (js_media_links().media_install_audio)(scope, global)
}

pub(crate) fn nsr_net_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    (js_net_links().net_install_interfaces)(scope, global)
}

pub(crate) fn nsr_js_net_install_sockets(scope: &mut Scope<'_>, global: &Value) {
    (js_net_links().js_net_install_sockets)(scope, global)
}

pub(crate) fn nsr_fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value) {
    (js_net_links().fetch_install_interfaces)(scope, global)
}

pub(crate) fn nsr_install_abort_signal_interface(scope: &mut Scope<'_>, global: &Value) {
    (js_net_links().install_abort_signal_interface)(scope, global)
}

pub(crate) fn nsr_xhr_install_interface(scope: &mut Scope<'_>, global: &Value) {
    (js_net_links().xhr_install_interface)(scope, global)
}

pub(crate) fn nsr_window_request_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_net_links().window_request_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_response_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_net_links().window_response_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_abort_controller_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_net_links().window_abort_controller_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_xhr_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_net_links().window_xhr_ctor)(scope, this, args)
}

pub(crate) fn nsr_js_fetch(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_net_links().js_fetch)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_element_isSameNode(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_node_links().element_is_same_node)(scope, this, args)
}

pub(crate) fn nsr_services_worker_navigator(scope: &mut Scope<'_>) -> Value {
    (js_services_links().services_worker_navigator)(scope)
}

pub(crate) fn nsr_services_window_navigator(scope: &mut Scope<'_>) -> Value {
    (js_services_links().services_window_navigator)(scope)
}

pub(crate) fn nsr_install_idle_deadline(scope: &mut Scope<'_>, global: &Value) {
    (js_services_links().install_idle_deadline)(scope, global)
}

pub(crate) fn nsr_services_install_rtc(scope: &mut Scope<'_>, global: &Value) {
    (js_services_links().services_install_rtc)(scope, global)
}

pub(crate) fn nsr_services_install_screen(scope: &mut Scope<'_>, global: &Value) {
    (js_services_links().services_install_screen)(scope, global)
}

pub(crate) fn nsr_services_install_clipboard_item(scope: &mut Scope<'_>, global: &Value) {
    (js_services_links().services_install_clipboard_item)(scope, global)
}

pub(crate) fn nsr_services_install_console(scope: &mut Scope<'_>, global: &Value) {
    (js_services_links().services_install_console)(scope, global)
}

pub(crate) fn nsr_storage_install_window(scope: &mut Scope<'_>, global: &Value) {
    (js_storage_links().storage_install_window)(scope, global)
}

pub(crate) fn nsr_window_get_selection(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_traversal_links().window_get_selection)(scope, this, args)
}

pub(crate) fn nsr_static_range_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_traversal_links().static_range_ctor)(scope, this, args)
}

pub(crate) fn nsr_net_install_text_codecs(scope: &mut Scope<'_>, global: &Value) {
    (js_url_links().net_install_text_codecs)(scope, global)
}

pub(crate) fn nsr_usp_install_interface(scope: &mut Scope<'_>) {
    (js_url_links().usp_install_interface)(scope)
}

pub(crate) fn nsr_url_install_interface(scope: &mut Scope<'_>) {
    (js_url_links().url_install_interface)(scope)
}

pub(crate) fn nsr_blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8> {
    (js_url_links().blob_bytes)(scope, blob)
}

pub(crate) fn nsr_window_open_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_window_links().window_open_method)(scope, this, args)
}

pub(crate) fn nsr_window_install_history(scope: &mut Scope<'_>, global: &Value) {
    (js_window_links().window_install_history)(scope, global)
}

pub(crate) fn nsr_window_install_browsing_context(scope: &mut Scope<'_>, global: &Value) {
    (js_window_links().window_install_browsing_context)(scope, global)
}

pub(crate) fn nsr_window_install_actions(scope: &mut Scope<'_>, global: &Value) {
    (js_window_links().window_install_actions)(scope, global)
}

pub(crate) fn nsr_window_find(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_window_links().window_find)(scope, this, args)
}

pub(crate) fn nsr_window_bind_post_message(scope: &mut Scope<'_>, global: &Value) {
    (js_window_links().window_bind_post_message)(scope, global)
}

pub(crate) fn nsr_js_new_window_named(scope: &mut Scope<'_>) -> Result<Value, Value> {
    (js_window_links().js_new_window_named)(scope)
}

pub(crate) fn nsr_sw_install_scope(scope: &mut Scope<'_>, target: &Value) {
    (js_workers_links().sw_install_scope)(scope, target)
}

pub(crate) fn nsr_worker_install_console(scope: &mut Scope<'_>, target: &Value) {
    (js_workers_links().worker_install_console)(scope, target)
}

pub(crate) fn nsr_worker_install_constructor(scope: &mut Scope<'_>, target: &Value) {
    (js_workers_links().worker_install_constructor)(scope, target)
}

pub(crate) fn nsr_sw_install_container(scope: &mut Scope<'_>, target: &Value) {
    (js_workers_links().sw_install_container)(scope, target)
}

pub(crate) fn nsr_worker_promise_rejection_tracker(
    scope: &mut Scope<'_>,
    promise: &Value,
    reason: &Value,
    handled: bool,
) {
    (js_workers_links().worker_promise_rejection_tracker)(scope, promise, reason, handled)
}

pub(crate) fn nsr_worker_install_location(scope: &mut Scope<'_>, global: &Value, url: &str) {
    (js_workers_links().worker_install_location)(scope, global, url)
}

pub(crate) fn nsr_webgl_install(scope: &mut Scope<'_>, global: &Value) {
    (webgl_links().webgl_install)(scope, global)
}

pub struct JsDocumentLinks {
    pub document_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_DOCUMENT_LINKS: OnceLock<JsDocumentLinks> = OnceLock::new();

pub fn set_js_document_links(links: JsDocumentLinks) {
    let _ = JS_DOCUMENT_LINKS.set(links);
}

fn js_document_links() -> &'static JsDocumentLinks {
    JS_DOCUMENT_LINKS
        .get()
        .expect("southstar-js-document installs its links before the first page")
}

pub struct JsGeometryLinks {
    pub window_scroll_by_pages: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_scroll_by_lines: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_scroll_by: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub window_scroll_to: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_GEOMETRY_LINKS: OnceLock<JsGeometryLinks> = OnceLock::new();

pub fn set_js_geometry_links(links: JsGeometryLinks) {
    let _ = JS_GEOMETRY_LINKS.set(links);
}

fn js_geometry_links() -> &'static JsGeometryLinks {
    JS_GEOMETRY_LINKS
        .get()
        .expect("southstar-js-geometry installs its links before the first page")
}

pub struct JsObserversLinks {
    pub resize_observer_disconnect: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub resize_observer_unobserve: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub resize_observer_observe: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub resize_observer_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub intersection_observer_take_records:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub intersection_observer_disconnect:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub intersection_observer_unobserve:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub intersection_observer_observe: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub intersection_observer_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub mutation_observer_take_records:
        fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub mutation_observer_disconnect: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub mutation_observer_observe: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub mutation_observer_ctor: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_OBSERVERS_LINKS: OnceLock<JsObserversLinks> = OnceLock::new();

pub fn set_js_observers_links(links: JsObserversLinks) {
    let _ = JS_OBSERVERS_LINKS.set(links);
}

fn js_observers_links() -> &'static JsObserversLinks {
    JS_OBSERVERS_LINKS
        .get()
        .expect("southstar-js-observers installs its links before the first page")
}

pub(crate) fn nsr_window_report_error(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().window_report_error)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_target_dispatchEvent(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().target_dispatch_event)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_dispatchEvent(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().window_dispatch_event)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_removeEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().window_remove_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_addEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().window_add_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_target_removeEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().target_remove_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_target_addEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().target_add_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_document_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_document_links().document_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_close_watcher_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_close_watcher_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_dom_parser_parseFromString(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().dom_parser_parse_from_string)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_dom_parser_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_dom_parser_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_option_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_option_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_comment_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_comment_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_text_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_text_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_document_fragment_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_document_fragment_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_audio_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_audio_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_image_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().window_image_ctor)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_attr_cloneNode(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dom_factory_links().attr_clone_node)(scope, this, args)
}

pub(crate) fn nsr_window_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_events_links().window_event_ctor)(scope, this, args)
}

pub(crate) fn nsr_radio_node_list_set_value(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_forms_links().radio_node_list_set_value)(scope, this, args)
}

pub(crate) fn nsr_radio_node_list_get_value(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_forms_links().radio_node_list_get_value)(scope, this, args)
}

pub(crate) fn nsr_custom_state_set_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_forms_links().custom_state_set_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_form_data_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_forms_links().window_form_data_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_scroll_by_pages(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_geometry_links().window_scroll_by_pages)(scope, this, args)
}

pub(crate) fn nsr_window_scroll_by_lines(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_geometry_links().window_scroll_by_lines)(scope, this, args)
}

pub(crate) fn nsr_window_scroll_by(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_geometry_links().window_scroll_by)(scope, this, args)
}

pub(crate) fn nsr_window_scroll_to(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_geometry_links().window_scroll_to)(scope, this, args)
}

pub(crate) fn nsr_vtt_cue_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().vtt_cue_ctor)(scope, this, args)
}

pub(crate) fn nsr_media_source_is_type_supported(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().media_source_is_type_supported)(scope, this, args)
}

pub(crate) fn nsr_window_mse_bytes(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_bytes)(scope, this, args)
}

pub(crate) fn nsr_window_mse_remove(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_remove)(scope, this, args)
}

pub(crate) fn nsr_window_mse_buffered_start(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_buffered_start)(scope, this, args)
}

pub(crate) fn nsr_window_mse_buffered(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_buffered)(scope, this, args)
}

pub(crate) fn nsr_window_mse_eos(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_eos)(scope, this, args)
}

pub(crate) fn nsr_window_mse_append(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_media_links().window_mse_append)(scope, this, args)
}

pub(crate) fn nsr_resize_observer_disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().resize_observer_disconnect)(scope, this, args)
}

pub(crate) fn nsr_resize_observer_unobserve(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().resize_observer_unobserve)(scope, this, args)
}

pub(crate) fn nsr_resize_observer_observe(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().resize_observer_observe)(scope, this, args)
}

pub(crate) fn nsr_resize_observer_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().resize_observer_ctor)(scope, this, args)
}

pub(crate) fn nsr_intersection_observer_take_records(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().intersection_observer_take_records)(scope, this, args)
}

pub(crate) fn nsr_intersection_observer_disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().intersection_observer_disconnect)(scope, this, args)
}

pub(crate) fn nsr_intersection_observer_unobserve(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().intersection_observer_unobserve)(scope, this, args)
}

pub(crate) fn nsr_intersection_observer_observe(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().intersection_observer_observe)(scope, this, args)
}

pub(crate) fn nsr_intersection_observer_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().intersection_observer_ctor)(scope, this, args)
}

pub(crate) fn nsr_mutation_observer_take_records(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().mutation_observer_take_records)(scope, this, args)
}

pub(crate) fn nsr_mutation_observer_disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().mutation_observer_disconnect)(scope, this, args)
}

pub(crate) fn nsr_mutation_observer_observe(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().mutation_observer_observe)(scope, this, args)
}

pub(crate) fn nsr_mutation_observer_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_observers_links().mutation_observer_ctor)(scope, this, args)
}

pub(crate) fn nsr_cache_open(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().cache_open)(scope, this, args)
}

pub(crate) fn nsr_services_notification_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_notification_ctor)(scope, this, args)
}

pub(crate) fn nsr_services_queue_microtask(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_queue_microtask)(scope, this, args)
}

pub(crate) fn nsr_services_request_idle_callback(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_request_idle_callback)(scope, this, args)
}

pub(crate) fn nsr_services_match_media(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_match_media)(scope, this, args)
}

pub(crate) fn nsr_services_prompt(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_prompt)(scope, this, args)
}

pub(crate) fn nsr_services_confirm(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_confirm)(scope, this, args)
}

pub(crate) fn nsr_services_camera_enumerate(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_camera_enumerate)(scope, this, args)
}

pub(crate) fn nsr_services_camera_label(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_camera_label)(scope, this, args)
}

pub(crate) fn nsr_services_mic_release(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_mic_release)(scope, this, args)
}

pub(crate) fn nsr_services_camera_release(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_camera_release)(scope, this, args)
}

pub(crate) fn nsr_services_camera_request(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_camera_request)(scope, this, args)
}

pub(crate) fn nsr_services_clear_timer(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_clear_timer)(scope, this, args)
}

pub(crate) fn nsr_services_set_interval(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_set_interval)(scope, this, args)
}

pub(crate) fn nsr_services_set_timeout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_set_timeout)(scope, this, args)
}

pub(crate) fn nsr_services_alert(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_services_links().services_alert)(scope, this, args)
}

pub(crate) fn nsr_window_filereader_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_filereader_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_text_decoder_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_text_decoder_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_text_encoder_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_text_encoder_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_usp_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_usp_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_url_revoke_object(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_revoke_object)(scope, this, args)
}

pub(crate) fn nsr_window_url_update_object(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_update_object)(scope, this, args)
}

pub(crate) fn nsr_window_url_create_object(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_create_object)(scope, this, args)
}

pub(crate) fn nsr_window_url_parse_static(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_parse_static)(scope, this, args)
}

pub(crate) fn nsr_window_url_can_parse(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_can_parse)(scope, this, args)
}

pub(crate) fn nsr_window_url_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_url_ctor)(scope, this, args)
}

pub(crate) fn nsr_window_atob(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_atob)(scope, this, args)
}

pub(crate) fn nsr_window_btoa(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_url_links().window_btoa)(scope, this, args)
}

pub(crate) fn nsr_worker_performance_clear(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_performance_clear)(scope, this, args)
}

pub(crate) fn nsr_worker_performance_entries(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_performance_entries)(scope, this, args)
}

pub(crate) fn nsr_worker_performance_now(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_performance_now)(scope, this, args)
}

pub(crate) fn nsr_worker_report_error(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_report_error)(scope, this, args)
}

pub(crate) fn nsr_worker_import_scripts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_import_scripts)(scope, this, args)
}

pub(crate) fn nsr_worker_global_close(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_global_close)(scope, this, args)
}

pub(crate) fn nsr_worker_global_post_message(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().worker_global_post_message)(scope, this, args)
}

pub(crate) fn nsr_window_broadcast_channel(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().window_broadcast_channel)(scope, this, args)
}

pub(crate) fn nsr_window_message_channel(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().window_message_channel)(scope, this, args)
}
