//! Southstar — creating a dedicated or service worker's realm and running a worker script.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_bind::{bind_constructor, bind_function};
use southstar_js_engine::{PromiseState, Scope, Value};

use crate::ffi::imports;
use crate::ffi::natives::*;
use crate::ffi::{self, Js, Native, RejectionTracker, WorkerSetup};
use crate::interrupt;
use crate::page::{bind_ctor, bind_url};

const DENY_OPAQUE_IDB: &str = "(function(){var f=self.indexedDB;if(!f)return;function deny(){throw new DOMException('Access to IndexedDB is denied for an opaque origin.','SecurityError');}['open','deleteDatabase','databases'].forEach(function(m){try{Object.defineProperty(f,m,{value:deny,configurable:true,writable:true});}catch(e){}});})()";

const WORKER_INTERFACES: [&str; 3] = ["WorkerLocation", "WorkerNavigator", "NavigatorUAData"];

pub(crate) enum EvalStatus {
    Done,
    ParseError(Value),
    Thrown(Value),
}

fn bind_fn(scope: &mut Scope<'_>, object: &Value, name: &str, native: Native, arity: u32) {
    let function = ffi::c_function(scope, name, arity, native);
    let _ = bind_function(scope, object, name, function);
}

fn text(value: Option<&CStr>) -> String {
    value.map_or_else(String::new, |v| v.to_string_lossy().into_owned())
}

pub(crate) fn create(js: Js, setup: &WorkerSetup<'_>) -> bool {
    let origin_us = (ffi::monotonic_us() / 100) * 100;
    let origin_real_ms = (ffi::real_time_us() as f64 / 100.0).floor() / 10.0;
    js.set_time_origin(origin_us, origin_real_ms);
    js.adopt_worker_host(setup);
    js.perf_init();
    let created = js.create_runtime(
        crate::memory_cap_bytes(256, Some(512)),
        interrupt::worker_stack_limit(),
        RejectionTracker::Worker,
    ) && js.create_context();
    if !created {
        js.perf_teardown();
        js.free_pinned_set();
        js.free_engine();
        return false;
    }
    js.scope(|scope| {
        crate::add_engine_private_names(scope);
        install_global(js, scope, setup, origin_real_ms);
    });
    true
}

fn install_global(js: Js, scope: &mut Scope<'_>, setup: &WorkerSetup<'_>, origin_real_ms: f64) {
    let global = scope.global();
    ffi::install(scope, ns_worker_install_console, &global);
    let functions: [(&str, Native, u32); 11] = [
        ("setTimeout", ns_services_set_timeout, 2),
        ("setInterval", ns_services_set_interval, 2),
        ("clearTimeout", ns_services_clear_timer, 1),
        ("clearInterval", ns_services_clear_timer, 1),
        ("postMessage", ns_worker_global_post_message, 1),
        ("close", ns_worker_global_close, 0),
        ("importScripts", ns_worker_import_scripts, 1),
        ("structuredClone", ns_window_structured_clone, 1),
        ("queueMicrotask", ns_services_queue_microtask, 1),
        ("btoa", ns_window_btoa, 1),
        ("atob", ns_window_atob, 1),
    ];
    for (name, native, arity) in functions {
        bind_fn(scope, &global, name, native, arity);
    }
    bind_ctor(
        scope,
        &global,
        "MessageChannel",
        ns_window_message_channel,
        0,
    );
    bind_ctor(scope, &global, "MessagePort", ns_illegal_constructor, 0);

    for name in ["Event", "MessageEvent", "ErrorEvent"] {
        bind_ctor(scope, &global, name, ns_window_event_ctor, 1);
    }
    ffi::install(scope, ns_events_install_worker, &global);
    bind_ctor(scope, &global, "EventTarget", ns_window_event_ctor, 0);
    bind_ctor(
        scope,
        &global,
        "TextEncoder",
        ns_window_text_encoder_ctor,
        0,
    );
    bind_ctor(
        scope,
        &global,
        "TextDecoder",
        ns_window_text_decoder_ctor,
        0,
    );
    ffi::install(scope, ns_net_install_text_codecs, &global);
    bind_ctor(scope, &global, "URLSearchParams", ns_window_usp_ctor, 0);
    ffi::install_context(scope, ns_usp_install_interface);
    bind_constructor(
        scope,
        &global,
        "FormData",
        0,
        imports::nsr_window_form_data_ctor,
    );
    bind_ctor(scope, &global, "FileReader", ns_window_filereader_ctor, 0);
    bind_url(scope, &global);
    ffi::install_context(scope, ns_url_install_interface);

    #[cfg(feature = "wasm")]
    ffi::install(scope, ns_wasm_install, &global);
    southstar_js_intl::install(scope, &global);
    let _ = southstar_js_temporal::install(scope, &global);
    let _ = scope.set(&global, "crossOriginIsolated", Value::boolean(false));
    southstar_js_realm::hide_shared_array_buffer(scope, &global);
    bind_ctor(
        scope,
        &global,
        "XMLHttpRequestEventTarget",
        ns_illegal_constructor,
        0,
    );
    bind_ctor(
        scope,
        &global,
        "XMLHttpRequestUpload",
        ns_illegal_constructor,
        0,
    );
    bind_ctor(scope, &global, "XMLHttpRequest", ns_window_xhr_ctor, 0);
    ffi::install(scope, ns_xhr_install_interface, &global);
    bind_fn(scope, &global, "fetch", ns_js_fetch, 1);
    bind_ctor(scope, &global, "Response", ns_window_response_ctor, 0);
    bind_ctor(scope, &global, "Request", ns_window_request_ctor, 1);
    ffi::install(scope, ns_fetch_install_interfaces, &global);
    let _ = scope.set(&global, "__ndWorkerHeadersOnly", Value::boolean(true));
    bind_ctor(
        scope,
        &global,
        "AbortController",
        ns_window_abort_controller_ctor,
        0,
    );
    ffi::install(scope, ns_install_abort_signal_interface, &global);
    ffi::install(scope, ns_net_install_interfaces, &global);
    ffi::install(scope, ns_idb_install, &global);
    js.eval_worker_polyfills();
    let current_global = scope.global();
    ffi::install(scope, ns_install_event_attribute_getters, &current_global);
    ffi::install_context(scope, ns_js_name_engine_members);
    ffi::install(scope, ns_crypto_install_worker, &global);
    ffi::install(scope, ns_webgl_install, &global);
    ffi::install_canvas(scope, &global, false);

    install_scope_members(scope, &global, setup, origin_real_ms);
}

fn install_scope_members(
    scope: &mut Scope<'_>,
    global: &Value,
    setup: &WorkerSetup<'_>,
    origin_real_ms: f64,
) {
    let _ = scope.set(global, "self", global.clone());
    let _ = scope.set(global, "globalThis", global.clone());
    bind_ctor(
        scope,
        global,
        "WorkerGlobalScope",
        ns_illegal_constructor,
        0,
    );
    if !setup.service_worker {
        bind_ctor(
            scope,
            global,
            "DedicatedWorkerGlobalScope",
            ns_illegal_constructor,
            0,
        );
    }
    for name in WORKER_INTERFACES {
        bind_ctor(scope, global, name, ns_illegal_constructor, 0);
    }
    bind_fn(scope, global, "reportError", ns_worker_report_error, 1);
    let listeners = scope.new_array();
    let _ = scope.set(global, "_listeners", listeners);
    crate::api::bind_event_target_listeners(scope, global);
    bind_fn(scope, global, "dispatchEvent", ns_target_dispatchEvent, 1);
    for name in ["onmessage", "onmessageerror", "onerror"] {
        let _ = scope.set(global, name, Value::null());
    }

    let navigator = ffi::worker_navigator(scope);
    let _ = scope.set(global, "navigator", navigator);

    let performance = scope.new_object();
    let methods: [(&str, Native, u32); 6] = [
        ("now", ns_worker_performance_now, 0),
        ("getEntries", ns_worker_performance_entries, 0),
        ("getEntriesByType", ns_worker_performance_entries, 1),
        ("getEntriesByName", ns_worker_performance_entries, 1),
        ("clearMarks", ns_worker_performance_clear, 0),
        ("clearMeasures", ns_worker_performance_clear, 0),
    ];
    for (name, native, arity) in methods {
        bind_fn(scope, &performance, name, native, arity);
    }
    let _ = scope.set(&performance, "timeOrigin", Value::number(origin_real_ms));
    let _ = scope.set(global, "performance", performance);
    let origin = scope.string(&text(setup.origin));
    let _ = scope.set(global, "origin", origin);
    let name = scope.string(&text(setup.name));
    let _ = scope.set(global, "name", name);
    if setup
        .origin
        .is_some_and(|origin| origin.to_bytes() == b"null")
    {
        let _ = scope.eval_native_script(DENY_OPAQUE_IDB, "<worker-origin>");
    }
    let secure_url = match setup.url {
        Some(url) if url.to_bytes().starts_with(b"blob:") => setup.base_url,
        url => url,
    };
    let secure = secure_url.is_some_and(|url| url.to_bytes().starts_with(b"https:"));
    let _ = scope.set(global, "isSecureContext", Value::boolean(secure));
    ffi::install_worker_location(scope, global, setup.url);

    if setup.service_worker {
        ffi::install(scope, ns_sw_install_scope, global);
    }
    ffi::shape_worker_global(scope, setup.service_worker);
}

pub(crate) fn eval(
    scope: &mut Scope<'_>,
    source: &CStr,
    len: usize,
    url: &CStr,
    module: bool,
) -> EvalStatus {
    let (compiled, parse_error) = ffi::worker_compile(scope, source, len, url, module);
    let result = compiled.and_then(|function| {
        let value = scope.evaluate(function)?;
        if !module {
            return Ok(value);
        }
        match scope.promise_state(&value) {
            PromiseState::Rejected(reason) => Err(reason),
            _ => Ok(value),
        }
    });
    match result {
        Ok(_) => EvalStatus::Done,
        Err(exception) if parse_error => EvalStatus::ParseError(exception),
        Err(exception) => EvalStatus::Thrown(exception),
    }
}
