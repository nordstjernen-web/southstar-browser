//! Southstar — creating a dedicated or service worker's realm and running a worker script.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_bind::{bind_constructor, bind_function, bind_native};
use southstar_js_engine::{NativeFn, PromiseState, Scope, Value};

use crate::ffi::imports;
use crate::ffi::natives::*;
use crate::ffi::{self, Js, Native, RejectionTracker, WorkerSetup};
use crate::interrupt;
use crate::page::{bind_ctor, bind_url};

const DENY_OPAQUE_IDB: &str = "(function(){var f=self.indexedDB;if(!f)return;function deny(){throw new DOMException('Access to IndexedDB is denied for an opaque origin.','SecurityError');}['open','deleteDatabase','databases'].forEach(function(m){try{Object.defineProperty(f,m,{value:deny,configurable:true,writable:true});}catch(e){}});})()";

const WORKER_INTERFACES: [&str; 3] = ["WorkerLocation", "WorkerNavigator", "NavigatorUAData"];

enum Binding {
    Raw(Native),
    Rust(NativeFn),
}

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
    imports::nsr_worker_install_console(scope, &global);
    let functions: [(&str, Binding, u32); 11] = [
        (
            "setTimeout",
            Binding::Rust(imports::nsr_services_set_timeout),
            2,
        ),
        (
            "setInterval",
            Binding::Rust(imports::nsr_services_set_interval),
            2,
        ),
        (
            "clearTimeout",
            Binding::Rust(imports::nsr_services_clear_timer),
            1,
        ),
        (
            "clearInterval",
            Binding::Rust(imports::nsr_services_clear_timer),
            1,
        ),
        (
            "postMessage",
            Binding::Rust(imports::nsr_worker_global_post_message),
            1,
        ),
        ("close", Binding::Rust(imports::nsr_worker_global_close), 0),
        (
            "importScripts",
            Binding::Rust(imports::nsr_worker_import_scripts),
            1,
        ),
        (
            "structuredClone",
            Binding::Rust(imports::nsr_window_structured_clone),
            1,
        ),
        (
            "queueMicrotask",
            Binding::Rust(imports::nsr_services_queue_microtask),
            1,
        ),
        ("btoa", Binding::Raw(ns_window_btoa), 1),
        ("atob", Binding::Raw(ns_window_atob), 1),
    ];
    for (name, binding, arity) in functions {
        match binding {
            Binding::Raw(native) => bind_fn(scope, &global, name, native, arity),
            Binding::Rust(native) => bind_native(scope, &global, name, arity, native),
        }
    }
    bind_constructor(
        scope,
        &global,
        "MessageChannel",
        0,
        imports::nsr_window_message_channel,
    );
    bind_ctor(scope, &global, "MessagePort", ns_illegal_constructor, 0);

    for name in ["Event", "MessageEvent", "ErrorEvent"] {
        bind_constructor(scope, &global, name, 1, ffi::imports::nsr_window_event_ctor);
    }
    ffi::imports::nsr_events_install_worker(scope, &global);
    bind_constructor(
        scope,
        &global,
        "EventTarget",
        0,
        ffi::imports::nsr_window_event_ctor,
    );
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
    ffi::imports::nsr_wasm_install(scope, &global);
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
    bind_constructor(
        scope,
        &global,
        "XMLHttpRequest",
        0,
        imports::nsr_window_xhr_ctor,
    );
    imports::nsr_xhr_install_interface(scope, &global);
    bind_native(scope, &global, "fetch", 1, imports::nsr_js_fetch);
    bind_constructor(
        scope,
        &global,
        "Response",
        0,
        imports::nsr_window_response_ctor,
    );
    bind_constructor(
        scope,
        &global,
        "Request",
        1,
        imports::nsr_window_request_ctor,
    );
    imports::nsr_fetch_install_interfaces(scope, &global);
    let _ = scope.set(&global, "__ndWorkerHeadersOnly", Value::boolean(true));
    bind_constructor(
        scope,
        &global,
        "AbortController",
        0,
        imports::nsr_window_abort_controller_ctor,
    );
    imports::nsr_install_abort_signal_interface(scope, &global);
    imports::nsr_net_install_interfaces(scope, &global);
    ffi::imports::nsr_idb_install(scope, &global);
    js.eval_worker_polyfills();
    let current_global = scope.global();
    ffi::imports::nsr_install_event_attribute_getters(scope, &current_global);
    ffi::imports::nsr_name_engine_members(scope);
    ffi::imports::nsr_crypto_install_worker(scope, &global);
    ffi::install_webgl(scope, &global);
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
    bind_native(
        scope,
        global,
        "reportError",
        1,
        imports::nsr_worker_report_error,
    );
    let listeners = scope.new_array();
    let _ = scope.set(global, "_listeners", listeners);
    crate::api::bind_event_target_listeners(scope, global);
    bind_native(
        scope,
        global,
        "dispatchEvent",
        1,
        imports::nsr_target_dispatchEvent,
    );
    for name in ["onmessage", "onmessageerror", "onerror"] {
        let _ = scope.set(global, name, Value::null());
    }

    let navigator = ffi::worker_navigator(scope);
    let _ = scope.set(global, "navigator", navigator);

    let performance = scope.new_object();
    let methods: [(&str, NativeFn, u32); 6] = [
        ("now", imports::nsr_worker_performance_now, 0),
        ("getEntries", imports::nsr_worker_performance_entries, 0),
        (
            "getEntriesByType",
            imports::nsr_worker_performance_entries,
            1,
        ),
        (
            "getEntriesByName",
            imports::nsr_worker_performance_entries,
            1,
        ),
        ("clearMarks", imports::nsr_worker_performance_clear, 0),
        ("clearMeasures", imports::nsr_worker_performance_clear, 0),
    ];
    for (name, native, arity) in methods {
        bind_native(scope, &performance, name, arity, native);
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
        imports::nsr_sw_install_scope(scope, global);
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
