//! Southstar — creating a page's realm: the runtime, the engine classes and every window installer in order.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_bind::natives::{resolved_empty_array, resolved_false, resolved_undefined};
use southstar_js_bind::{
    attach_prototype, bind_constructor, bind_function, bind_int_constants, bind_native,
    bind_prototype_function_if_missing, bind_prototype_native_if_missing, prototype_of,
};
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::imports;
use crate::ffi::natives::*;
use crate::ffi::{self, Callbacks, Class, Js, Native, NavigationTiming, RejectionTracker};

const PAGE_STACK_SIZE: usize = 5 * 1024 * 1024;

const TOKEN_LIST_SHAPE: &str = "(function(p){ var a = Array.prototype; if (a) Object.setPrototypeOf(p, a); Object.defineProperty(p, Symbol.toStringTag,   { value: 'DOMTokenList', configurable: true });})";

const SCHEDULER: &str = "(function(){  var w = globalThis;  var nextTaskId = 1;  var canceled = Object.create(null);  function later(cb){ return setTimeout(cb, 0); }  if (!w.scheduler) w.scheduler = {};  if (typeof w.scheduler.postTask !== 'function')    w.scheduler.postTask = function(cb){      var id = nextTaskId++;      return new Promise(function(resolve, reject){        later(function(){          if (canceled[id]) return;          try { resolve(typeof cb === 'function' ? cb() : undefined); }          catch (e) { reject(e); }        });      });    };  if (typeof w.scheduler.yield !== 'function')    w.scheduler.yield = function(){ return new Promise(function(resolve){ later(resolve); }); };})();";

const NAVIGATOR_INTERFACE: &str = "(function(){ if (typeof Navigator !== 'function' || typeof navigator !== 'object'     || !navigator) return; var nav = navigator, Np = Navigator.prototype, others = new WeakSet(); var names = ['userAgent','appName','appCodeName','appVersion',   'platform','language','onLine','doNotTrack','globalPrivacyControl',   'cookieEnabled','hardwareConcurrency','vendor','product',   'productSub','maxTouchPoints','deviceMemory','pdfViewerEnabled',   'webdriver','vendorSub','languages']; names.forEach(function(n){   if (!(n in nav)) return;   var val = nav[n];   try { delete nav[n]; } catch(e) {}   var holder = { get [n](){     if (this !== nav && !others.has(this))       throw new TypeError('Illegal invocation');     return val; } };   Object.defineProperty(Np, n, { configurable:true, enumerable:true,     get: Object.getOwnPropertyDescriptor(holder,n).get }); }); try { Object.setPrototypeOf(nav, Np); } catch(e) {} try { Object.defineProperty(Np, Symbol.toStringTag,   { value:'Navigator', configurable:true }); } catch(e) {} return others;})();";

const HIDE_INTERNALS: &str = "(function(){ Object.getOwnPropertyNames(globalThis).forEach(function(k){   if (/^__(nd|ns|js|ND)/.test(k)) {     var d = Object.getOwnPropertyDescriptor(globalThis, k);     if (d && d.enumerable && d.configurable) {       try { Object.defineProperty(globalThis, k,         { enumerable: false }); } catch(e) {} } } });})();";

const NODE_CONSTANTS: [(&str, i32); 18] = [
    ("ELEMENT_NODE", 1),
    ("ATTRIBUTE_NODE", 2),
    ("TEXT_NODE", 3),
    ("CDATA_SECTION_NODE", 4),
    ("ENTITY_REFERENCE_NODE", 5),
    ("ENTITY_NODE", 6),
    ("PROCESSING_INSTRUCTION_NODE", 7),
    ("COMMENT_NODE", 8),
    ("DOCUMENT_NODE", 9),
    ("DOCUMENT_TYPE_NODE", 10),
    ("DOCUMENT_FRAGMENT_NODE", 11),
    ("NOTATION_NODE", 12),
    ("DOCUMENT_POSITION_DISCONNECTED", 0x01),
    ("DOCUMENT_POSITION_PRECEDING", 0x02),
    ("DOCUMENT_POSITION_FOLLOWING", 0x04),
    ("DOCUMENT_POSITION_CONTAINS", 0x08),
    ("DOCUMENT_POSITION_CONTAINED_BY", 0x10),
    ("DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC", 0x20),
];

const NODE_CARRIERS: [&str; 6] = [
    "Node",
    "Element",
    "HTMLElement",
    "Document",
    "HTMLDocument",
    "DocumentFragment",
];

const FIRST_DOCUMENT_CARRIER: usize = 3;

const ELEMENT_PROTO_CARRIERS: [&str; 3] = ["Node", "Element", "HTMLElement"];

const EVENT_PHASES: [(&str, i32); 4] = [
    ("NONE", 0),
    ("CAPTURING_PHASE", 1),
    ("AT_TARGET", 2),
    ("BUBBLING_PHASE", 3),
];

const EVENT_BASE_CONSTRUCTORS: [&str; 10] = [
    "EventTarget",
    "Node",
    "Element",
    "HTMLElement",
    "SVGElement",
    "SVGAElement",
    "SVGSVGElement",
    "MathMLElement",
    "HTMLDocument",
    "Window",
];

const WINDOW_EVENT_HANDLERS: [&str; 20] = [
    "onload",
    "onunload",
    "onbeforeunload",
    "onhashchange",
    "onpopstate",
    "onpagehide",
    "onpageshow",
    "onmessage",
    "onmessageerror",
    "onoffline",
    "ononline",
    "onresize",
    "onscroll",
    "onstorage",
    "onerror",
    "onrejectionhandled",
    "onunhandledrejection",
    "onlanguagechange",
    "onafterprint",
    "onbeforeprint",
];

const PERFORMANCE_NAVIGATION_TYPES: [(&str, i32); 4] = [
    ("TYPE_NAVIGATE", 0),
    ("TYPE_RELOAD", 1),
    ("TYPE_BACK_FORWARD", 2),
    ("TYPE_RESERVED", 255),
];

const MEDIA_ERRORS: [(&str, i32); 4] = [
    ("MEDIA_ERR_ABORTED", 1),
    ("MEDIA_ERR_NETWORK", 2),
    ("MEDIA_ERR_DECODE", 3),
    ("MEDIA_ERR_SRC_NOT_SUPPORTED", 4),
];

const ENUMERABLE_ONLY: Attributes = Attributes::ENUMERABLE;

const WRITABLE_ONLY: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

const FIXED: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

const ENUMERABLE_ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) fn create(js: Js, timing: Option<NavigationTiming>, callbacks: Callbacks) -> bool {
    let timing = navigation_timing(timing);
    js.set_navigation_timing(timing);
    js.perf_init();
    let created = js.create_runtime(
        crate::memory_cap_bytes(2048, None),
        PAGE_STACK_SIZE,
        RejectionTracker::Page,
    ) && js.create_context();
    if !created {
        js.perf_teardown();
        js.free_engine();
        return false;
    }
    js.scope(|scope| {
        crate::add_engine_private_names(scope);
        ffi::add_engine_private_name(scope, "__ndRealmDoc");
        ffi::add_engine_private_name(scope, "__ndRealmWindow");
    });
    js.set_callbacks(callbacks);
    js.init_tables(ffi::default_main_context(), true);
    js.init_page_states();
    js.scope(|scope| install_window(js, scope));
    ffi::set_active(js);
    true
}

fn navigation_timing(given: Option<NavigationTiming>) -> NavigationTiming {
    let mut timing = match given {
        Some(timing) if timing.origin_us > 0 && timing.origin_real_ms > 0.0 => timing,
        _ => {
            NavigationTiming::starting_at(ffi::monotonic_us(), ffi::real_time_us() as f64 / 1000.0)
        }
    };
    ffi::coarsen_navigation_timing(&mut timing);
    timing
}

fn bind_fn(scope: &mut Scope<'_>, object: &Value, name: &str, native: Native, arity: u32) {
    let function = ffi::c_function(scope, name, arity, native);
    let _ = bind_function(scope, object, name, function);
}

fn make_ctor(scope: &mut Scope<'_>, name: &str, native: Native, arity: u32) -> Value {
    let constructor = ffi::c_constructor_or_function(scope, name, arity, native);
    let _ = attach_prototype(scope, &constructor, name);
    constructor
}

pub(crate) fn bind_ctor(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    native: Native,
    arity: u32,
) {
    let constructor = make_ctor(scope, name, native, arity);
    let _ = scope.set(object, name, constructor);
}

fn bind_proto_fn(
    scope: &mut Scope<'_>,
    global: &Value,
    constructor: &str,
    name: &str,
    native: Native,
    arity: u32,
) {
    let _ = bind_prototype_function_if_missing(scope, global, constructor, name, |scope| {
        ffi::c_function(scope, name, arity, native)
    });
}

pub(crate) fn bind_url(scope: &mut Scope<'_>, global: &Value) {
    let url = make_ctor(scope, "URL", ns_window_url_ctor, 1);
    bind_fn(scope, &url, "canParse", ns_window_url_can_parse, 1);
    bind_fn(scope, &url, "parse", ns_window_url_parse_static, 1);
    bind_fn(
        scope,
        &url,
        "createObjectURL",
        ns_window_url_create_object,
        1,
    );
    bind_fn(
        scope,
        &url,
        "revokeObjectURL",
        ns_window_url_revoke_object,
        1,
    );
    let _ = scope.set(global, "URL", url);
}

fn get_or_undefined(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

fn eval_shape(scope: &mut Scope<'_>, source: &str, name: &str) -> Option<Value> {
    scope.eval_native_script(source, name).ok()
}

fn install_window(js: Js, scope: &mut Scope<'_>) {
    let element_proto = register_classes(scope);
    let global = scope.global();
    register_storage_and_named(scope, &global);
    ffi::imports::nsr_services_install_console(scope, &global);
    install_timers_and_navigator(js, scope, &global);
    install_performance(js, scope, &global);
    install_window_members(js, scope, &global);
    install_constructors(scope, &global);
    link_document_prototypes(js, scope, &global);
    install_node_constants(scope, &global, &element_proto);
    let function = ffi::is_engine_function_native(scope);
    let _ = bind_function(scope, &global, "__ndIsEngineFunction", function);
    imports::nsr_js_install_event_target(scope, &global);
    install_named_properties(scope, &global);
    let _ = eval_shape(scope, SCHEDULER, "<post-message>");
    install_window_tail(js, scope, &global);
    install_navigator_interface(js, scope);
    let _ = eval_shape(scope, HIDE_INTERNALS, "<hide-internals>");
    let promise = get_or_undefined(scope, &global, "Promise");
    js.set_pristine_promise(promise);
}

fn register_classes(scope: &mut Scope<'_>) -> Value {
    let element_proto = ffi::register_class(scope, Class::Element);
    ffi::register_class(scope, Class::Attr);
    ffi::register_class(scope, Class::Style);
    let token_list_proto = ffi::register_class(scope, Class::TokenList);
    if let Some(shape) = eval_shape(scope, TOKEN_LIST_SHAPE, "<tlist-iter>") {
        let _ = scope.call(&shape, &Value::undefined(), &[token_list_proto]);
    }
    ffi::register_class(scope, Class::Live);
    let radio_node_list = ffi::live_proto(scope, 2);
    let getter = scope.function("get value", 0, imports::nsr_radio_node_list_get_value);
    let setter = scope.function("set value", 1, imports::nsr_radio_node_list_set_value);
    let _ = scope.define_accessor(
        &radio_node_list,
        "value",
        Some(&getter),
        Some(&setter),
        ENUMERABLE_ACCESSOR,
    );
    ffi::register_class(scope, Class::Dataset);
    element_proto
}

fn register_storage_and_named(scope: &mut Scope<'_>, global: &Value) {
    let storage_proto = ffi::register_class(scope, Class::Storage);
    let storage = ffi::c_constructor(scope, "Storage", 0, ns_illegal_constructor);
    let _ = scope.set_constructor(&storage, &storage_proto);
    let _ = scope.set(global, "Storage", storage);
    ffi::register_class(scope, Class::WindowNamed);
}

fn install_timers_and_navigator(js: Js, scope: &mut Scope<'_>, global: &Value) {
    bind_native(scope, global, "alert", 1, ffi::imports::nsr_services_alert);
    let engine_name = ffi::engine_name_native(scope);
    let _ = bind_function(scope, global, "__jsEngine", engine_name);
    bind_native(
        scope,
        global,
        "setTimeout",
        2,
        ffi::imports::nsr_services_set_timeout,
    );
    bind_native(
        scope,
        global,
        "setInterval",
        2,
        ffi::imports::nsr_services_set_interval,
    );
    bind_native(
        scope,
        global,
        "clearTimeout",
        1,
        ffi::imports::nsr_services_clear_timer,
    );
    bind_native(
        scope,
        global,
        "clearInterval",
        1,
        ffi::imports::nsr_services_clear_timer,
    );
    bind_native(scope, global, "fetch", 1, imports::nsr_js_fetch);
    imports::nsr_window_bind_post_message(scope, global);

    bind_ctor(scope, global, "Navigator", ns_illegal_constructor, 0);
    let navigator = ffi::window_navigator(scope);
    let camera: [(&str, NativeFn, u32); 5] = [
        (
            "__nd_camera_request",
            ffi::imports::nsr_services_camera_request,
            2,
        ),
        (
            "__nd_camera_release",
            ffi::imports::nsr_services_camera_release,
            0,
        ),
        (
            "__nd_mic_release",
            ffi::imports::nsr_services_mic_release,
            0,
        ),
        (
            "__nd_camera_label",
            ffi::imports::nsr_services_camera_label,
            0,
        ),
        (
            "__nd_camera_enumerate",
            ffi::imports::nsr_services_camera_enumerate,
            0,
        ),
    ];
    for (name, native, arity) in camera {
        bind_native(scope, global, name, arity, native);
    }
    imports::nsr_sw_install_container(scope, &navigator);
    #[cfg(feature = "webgpu")]
    js.install_webgpu(scope, &navigator);
    #[cfg(not(feature = "webgpu"))]
    let _ = js;
    let _ = scope.set(global, "navigator", navigator);
}

fn install_performance(js: Js, scope: &mut Scope<'_>, global: &Value) {
    let chrome_compat = ffi::chrome_compat();
    if chrome_compat {
        ffi::imports::nsr_install_window_chrome(scope, global);
    }
    bind_ctor(scope, global, "Performance", ns_illegal_constructor, 0);
    bind_ctor(
        scope,
        global,
        "PerformanceTiming",
        ns_illegal_constructor,
        0,
    );
    bind_ctor(
        scope,
        global,
        "PerformanceNavigation",
        ns_illegal_constructor,
        0,
    );
    let _ = bind_int_constants(
        scope,
        global,
        "PerformanceNavigation",
        PERFORMANCE_NAVIGATION_TYPES,
    );
    let performance = js.make_performance_object(scope, chrome_compat);
    let _ = scope.set(global, "performance", performance);

    bind_ctor(
        scope,
        global,
        "MutationObserver",
        ns_mutation_observer_ctor,
        1,
    );
    bind_ctor(
        scope,
        global,
        "IntersectionObserver",
        ns_intersection_observer_ctor,
        1,
    );
    bind_ctor(scope, global, "ResizeObserver", ns_resize_observer_ctor, 1);
    bind_ctor(
        scope,
        global,
        "PerformanceObserver",
        ns_perf_observer_ctor,
        1,
    );
    bind_ctor(
        scope,
        global,
        "PerformanceObserverEntryList",
        ns_illegal_constructor,
        0,
    );
    ffi::install(scope, ns_perf_install_entry_list, global);
    let observer_methods: [(&str, &str, Native, u32); 13] = [
        (
            "MutationObserver",
            "observe",
            ns_mutation_observer_observe,
            2,
        ),
        (
            "MutationObserver",
            "disconnect",
            ns_mutation_observer_disconnect,
            0,
        ),
        (
            "MutationObserver",
            "takeRecords",
            ns_mutation_observer_takeRecords,
            0,
        ),
        (
            "IntersectionObserver",
            "observe",
            ns_intersection_observer_observe,
            1,
        ),
        (
            "IntersectionObserver",
            "unobserve",
            ns_intersection_observer_unobserve,
            1,
        ),
        (
            "IntersectionObserver",
            "disconnect",
            ns_intersection_observer_disconnect,
            0,
        ),
        (
            "IntersectionObserver",
            "takeRecords",
            ns_intersection_observer_takeRecords,
            0,
        ),
        ("ResizeObserver", "observe", ns_resize_observer_observe, 2),
        (
            "ResizeObserver",
            "unobserve",
            ns_resize_observer_unobserve,
            1,
        ),
        (
            "ResizeObserver",
            "disconnect",
            ns_resize_observer_disconnect,
            0,
        ),
        (
            "PerformanceObserver",
            "observe",
            ns_perf_observer_observe,
            1,
        ),
        (
            "PerformanceObserver",
            "disconnect",
            ns_perf_observer_disconnect,
            0,
        ),
        (
            "PerformanceObserver",
            "takeRecords",
            ns_perf_observer_takeRecords,
            0,
        ),
    ];
    for (constructor, name, native, arity) in observer_methods {
        bind_proto_fn(scope, global, constructor, name, native, arity);
    }
    let observer = get_or_undefined(scope, global, "PerformanceObserver");
    let entry_types = ffi::supported_entry_types(scope);
    let _ = scope.set(&observer, "supportedEntryTypes", entry_types);
}

fn install_window_members(js: Js, scope: &mut Scope<'_>, global: &Value) {
    bind_native(
        scope,
        global,
        "addEventListener",
        2,
        imports::nsr_window_addEventListener,
    );
    bind_native(
        scope,
        global,
        "removeEventListener",
        2,
        imports::nsr_window_removeEventListener,
    );
    bind_native(
        scope,
        global,
        "dispatchEvent",
        1,
        imports::nsr_window_dispatchEvent,
    );
    for key in ["scrollY", "scrollX", "pageYOffset", "pageXOffset"] {
        let _ = scope.set(global, key, Value::int(0));
    }
    js.sync_window_metrics();

    imports::nsr_window_install_actions(scope, global);
    bind_native(scope, global, "find", 7, imports::nsr_window_find);
    let scroll_methods: [(&str, NativeFn, u32); 5] = [
        ("scrollTo", imports::nsr_window_scroll_to, 2),
        ("scrollBy", imports::nsr_window_scroll_by, 2),
        ("scroll", imports::nsr_window_scroll_to, 2),
        ("scrollByLines", imports::nsr_window_scroll_by_lines, 1),
        ("scrollByPages", imports::nsr_window_scroll_by_pages, 1),
    ];
    for (name, native, arity) in scroll_methods {
        bind_native(scope, global, name, arity, native);
    }
    bind_native(scope, global, "open", 3, imports::nsr_window_open_method);
    let service_methods: [(&str, NativeFn, u32); 3] = [
        ("confirm", ffi::imports::nsr_services_confirm, 1),
        ("prompt", ffi::imports::nsr_services_prompt, 2),
        ("matchMedia", ffi::imports::nsr_services_match_media, 1),
    ];
    for (name, native, arity) in service_methods {
        bind_native(scope, global, name, arity, native);
    }
    ffi::install(scope, ns_cssom_install_window, global);
    bind_fn(
        scope,
        global,
        "requestAnimationFrame",
        ns_window_requestAnimationFrame,
        1,
    );
    bind_fn(
        scope,
        global,
        "cancelAnimationFrame",
        ns_window_cancelAnimationFrame,
        1,
    );
    ffi::install(scope, ns_js_input_install_wpt, global);
    let media_natives: [(&str, Native, u32); 8] = [
        ("__ndUpdateBlobURL", ns_window_url_update_object, 2),
        ("__ndMseAppend", ns_window_mse_append, 3),
        ("__ndMseEos", ns_window_mse_eos, 1),
        ("__ndMseBuffered", ns_window_mse_buffered, 2),
        ("__ndMseBufferedStart", ns_window_mse_buffered_start, 2),
        ("__ndMseRemove", ns_window_mse_remove, 4),
        ("__ndMseBytes", ns_window_mse_bytes, 2),
        ("__ndMseTypeSupported", ns_media_source_is_type_supported, 1),
    ];
    for (name, native, arity) in media_natives {
        bind_fn(scope, global, name, native, arity);
    }

    ffi::imports::nsr_events_install_window_base(scope, global);
    bind_ctor(scope, global, "History", ns_illegal_constructor, 0);
    imports::nsr_window_install_history(scope, global);
    ffi::imports::nsr_crypto_install_window(scope, global);

    bind_fn(scope, global, "btoa", ns_window_btoa, 1);
    bind_fn(scope, global, "atob", ns_window_atob, 1);
    bind_url(scope, global);
    let custom_elements = scope.new_object();
    bind_native(scope, &custom_elements, "define", 3, imports::nsr_ce_define);
    bind_native(scope, &custom_elements, "get", 1, imports::nsr_ce_get);
    bind_native(
        scope,
        &custom_elements,
        "upgrade",
        1,
        imports::nsr_ce_upgrade,
    );
    bind_native(
        scope,
        &custom_elements,
        "whenDefined",
        1,
        imports::nsr_ce_when_defined,
    );
    bind_native(
        scope,
        &custom_elements,
        "getName",
        1,
        imports::nsr_ce_get_name,
    );
    let _ = scope.set(global, "customElements", custom_elements);
}

fn install_constructors(scope: &mut Scope<'_>, global: &Value) {
    bind_constructor(scope, global, "Image", 2, imports::nsr_window_image_ctor);
    bind_ctor(scope, global, "MediaError", ns_illegal_constructor, 0);
    let _ = bind_int_constants(scope, global, "MediaError", MEDIA_ERRORS);
    bind_constructor(
        scope,
        global,
        "StaticRange",
        1,
        imports::nsr_static_range_ctor,
    );
    bind_ctor(scope, global, "VTTCue", ns_vtt_cue_ctor, 3);
    ffi::imports::nsr_services_install_clipboard_item(scope, global);
    bind_constructor(
        scope,
        global,
        "CustomStateSet",
        0,
        imports::nsr_custom_state_set_ctor,
    );
    ffi::install_webgl(scope, global);
    bind_constructor(scope, global, "Audio", 1, imports::nsr_window_audio_ctor);
    ffi::install(scope, ns_media_install_audio, global);
    bind_constructor(
        scope,
        global,
        "DocumentFragment",
        0,
        imports::nsr_window_document_fragment_ctor,
    );
    bind_constructor(scope, global, "Text", 1, imports::nsr_window_text_ctor);
    bind_constructor(
        scope,
        global,
        "Comment",
        1,
        imports::nsr_window_comment_ctor,
    );
    bind_constructor(scope, global, "Option", 4, imports::nsr_window_option_ctor);
    bind_ctor(scope, global, "URLSearchParams", ns_window_usp_ctor, 0);
    ffi::install_context(scope, ns_usp_install_interface);
    ffi::install_context(scope, ns_url_install_interface);
    bind_ctor(
        scope,
        global,
        "XMLHttpRequestEventTarget",
        ns_illegal_constructor,
        0,
    );
    bind_ctor(
        scope,
        global,
        "XMLHttpRequestUpload",
        ns_illegal_constructor,
        0,
    );
    bind_constructor(
        scope,
        global,
        "XMLHttpRequest",
        0,
        imports::nsr_window_xhr_ctor,
    );
    imports::nsr_xhr_install_interface(scope, global);
    bind_constructor(
        scope,
        global,
        "DOMParser",
        0,
        imports::nsr_window_dom_parser_ctor,
    );
    bind_prototype_native_if_missing(
        scope,
        global,
        "DOMParser",
        "parseFromString",
        2,
        imports::nsr_dom_parser_parseFromString,
    );
    bind_constructor(
        scope,
        global,
        "FormData",
        0,
        imports::nsr_window_form_data_ctor,
    );
    bind_constructor(
        scope,
        global,
        "AbortController",
        0,
        imports::nsr_window_abort_controller_ctor,
    );
    bind_constructor(
        scope,
        global,
        "CloseWatcher",
        0,
        imports::nsr_window_close_watcher_ctor,
    );
    imports::nsr_install_abort_signal_interface(scope, global);

    let caches = scope.new_object();
    bind_native(scope, &caches, "open", 1, ffi::imports::nsr_cache_open);
    bind_native(scope, &caches, "has", 1, resolved_false);
    bind_native(scope, &caches, "delete", 1, resolved_false);
    bind_native(scope, &caches, "keys", 0, resolved_empty_array);
    bind_native(scope, &caches, "match", 2, resolved_undefined);
    let _ = scope.get(global, "caches");
    let _ = scope.set(global, "caches", caches);
    bind_ctor(scope, global, "TextEncoder", ns_window_text_encoder_ctor, 0);
    bind_ctor(scope, global, "TextDecoder", ns_window_text_decoder_ctor, 0);
    ffi::install(scope, ns_net_install_text_codecs, global);
    bind_constructor(
        scope,
        global,
        "Response",
        0,
        imports::nsr_window_response_ctor,
    );
    bind_constructor(
        scope,
        global,
        "Request",
        1,
        imports::nsr_window_request_ctor,
    );
    imports::nsr_fetch_install_interfaces(scope, global);
    bind_ctor(scope, global, "FileReader", ns_window_filereader_ctor, 0);

    ffi::imports::nsr_events_install_window(scope, global);
    for name in EVENT_BASE_CONSTRUCTORS {
        bind_constructor(scope, global, name, 0, ffi::imports::nsr_window_event_ctor);
    }
    ffi::install_canvas(scope, global, true);
    bind_constructor(scope, global, "Document", 0, imports::nsr_document_ctor);
}

fn link_document_prototypes(js: Js, scope: &mut Scope<'_>, global: &Value) {
    let html_document = prototype_of(scope, global, "HTMLDocument");
    let document = prototype_of(scope, global, "Document");
    if html_document.is_object() && document.is_object() {
        let _ = scope.set_prototype(&html_document, &document);
        js.set_proto_document(html_document);
    }
}

fn install_node_constants(scope: &mut Scope<'_>, global: &Value, element_proto: &Value) {
    for (name, value) in NODE_CONSTANTS {
        let _ = scope.define(element_proto, name, Value::int(value), ENUMERABLE_ONLY);
    }
    for (index, carrier_name) in NODE_CARRIERS.into_iter().enumerate() {
        let carrier = get_or_undefined(scope, global, carrier_name);
        if !carrier.is_object() {
            continue;
        }
        let proto = get_or_undefined(scope, &carrier, "prototype");
        let document_like = index >= FIRST_DOCUMENT_CARRIER;
        for (name, value) in NODE_CONSTANTS {
            let _ = scope.define(&carrier, name, Value::int(value), ENUMERABLE_ONLY);
            if proto.is_object() && !document_like {
                let _ = scope.define(&proto, name, Value::int(value), ENUMERABLE_ONLY);
            }
        }
        if proto.is_object() {
            ffi::install_element_proto_table(scope, &proto);
            if !proto.same_object(element_proto) {
                let _ = scope.set_prototype(&proto, element_proto);
            }
        }
    }
    for carrier_name in ELEMENT_PROTO_CARRIERS {
        let carrier = get_or_undefined(scope, global, carrier_name);
        if !carrier.is_object() {
            continue;
        }
        let _ = scope.define(&carrier, "prototype", element_proto.clone(), WRITABLE_ONLY);
        let _ = scope.define(element_proto, "constructor", carrier, Attributes::METHOD);
    }
    let event_target = get_or_undefined(scope, global, "EventTarget");
    if event_target.is_object() {
        let event_target_proto = get_or_undefined(scope, &event_target, "prototype");
        if event_target_proto.is_object() {
            let _ = scope.set_prototype(element_proto, &event_target_proto);
        }
    }
    let performance = get_or_undefined(scope, global, "performance");
    if performance.is_object() {
        ffi::performance_extend_event_target(scope, global, &performance);
    }
    let event = get_or_undefined(scope, global, "Event");
    if event.is_object() {
        let proto = get_or_undefined(scope, &event, "prototype");
        for (name, value) in EVENT_PHASES {
            let _ = scope.define(&event, name, Value::int(value), FIXED);
            if proto.is_object() {
                let _ = scope.define(&proto, name, Value::int(value), FIXED);
            }
        }
    }
}

fn install_named_properties(scope: &mut Scope<'_>, global: &Value) {
    let Ok(window_proto) = scope.get_prototype(global) else {
        return;
    };
    if !window_proto.is_object() {
        return;
    }
    let base = scope
        .get_prototype(&window_proto)
        .unwrap_or_else(|_| Value::undefined());
    if let Some(named) = ffi::new_window_named(scope) {
        let _ = scope.set_prototype(&named, &base);
        let _ = scope.set_prototype(&window_proto, &named);
    }
}

fn install_window_tail(js: Js, scope: &mut Scope<'_>, global: &Value) {
    imports::nsr_window_install_browsing_context(scope, global);
    for name in WINDOW_EVENT_HANDLERS {
        let _ = scope.set(global, name, Value::null());
    }
    bind_native(
        scope,
        global,
        "getSelection",
        0,
        imports::nsr_window_get_selection,
    );
    bind_native(
        scope,
        global,
        "requestIdleCallback",
        2,
        ffi::imports::nsr_services_request_idle_callback,
    );
    bind_native(
        scope,
        global,
        "cancelIdleCallback",
        1,
        ffi::imports::nsr_services_clear_timer,
    );
    ffi::imports::nsr_install_idle_deadline(scope, global);
    js.install_window_state(scope, global);
    ffi::imports::nsr_services_install_screen(scope, global);

    bind_native(
        scope,
        global,
        "structuredClone",
        1,
        ffi::imports::nsr_window_structured_clone,
    );
    bind_native(
        scope,
        global,
        "reportError",
        1,
        imports::nsr_window_report_error,
    );
    bind_native(
        scope,
        global,
        "queueMicrotask",
        1,
        ffi::imports::nsr_services_queue_microtask,
    );
    bind_constructor(
        scope,
        global,
        "MessageChannel",
        0,
        imports::nsr_window_message_channel,
    );
    bind_ctor(scope, global, "MessagePort", ns_illegal_constructor, 0);
    bind_constructor(
        scope,
        global,
        "BroadcastChannel",
        1,
        imports::nsr_window_broadcast_channel,
    );
    ffi::imports::nsr_services_install_rtc(scope, global);
    bind_constructor(
        scope,
        global,
        "Notification",
        2,
        ffi::imports::nsr_services_notification_ctor,
    );
    imports::nsr_worker_install_constructor(scope, global);
    imports::nsr_js_net_install_sockets(scope, global);
    ffi::install(scope, ns_cssom_install_css, global);
    ffi::imports::nsr_crypto_install_window_subtle(scope, global);
    #[cfg(feature = "wasm")]
    ffi::install(scope, ns_wasm_install, global);
    ffi::install(scope, ns_storage_install_window, global);
    ffi::imports::nsr_install_window_compat(scope, global);
}

fn install_navigator_interface(js: Js, scope: &mut Scope<'_>) {
    if let Some(brand) = eval_shape(scope, NAVIGATOR_INTERFACE, "<navigator-iface>")
        && brand.is_object()
    {
        js.set_navigator_brand(brand);
    }
}
