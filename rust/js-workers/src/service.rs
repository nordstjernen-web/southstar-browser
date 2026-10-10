//! Southstar — service workers: registration, the lifecycle a registration's worker goes through, its global scope, and the fetches it answers for its controlled page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::Ordering;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, CFn, Js};
use crate::worker::{self, Host, HostSpec, Owned, WorkerRef};
use crate::{JsResult, arg, bind, get, get_index, length, push, set, set_str};

const FETCH_DISPATCH: &str = "globalThis.__nd_sw_dispatch_fetch=function(id,url,method,headers,body){\
var hobj={};try{for(var i=0;i<headers.length;i++){var s=headers[i];var c=s.indexOf(':');\
if(c>0)hobj[s.slice(0,c).trim()]=s.slice(c+1).trim();}}catch(e){}\
var init={method:method,headers:hobj};\
if(body&&method!=='GET'&&method!=='HEAD')init.body=body;\
var req;try{req=new Request(url,init);}catch(e){req={url:url,method:method,headers:hobj};}\
var responded=false,captured=null;\
var ev;try{ev=new Event('fetch');}catch(e){ev={type:'fetch'};}\
ev.request=req;ev.clientId='';ev.resultingClientId='';\
ev.respondWith=function(r){responded=true;captured=r;};\
ev.waitUntil=function(){};\
try{self.dispatchEvent(ev);}catch(e){}\
if(!responded){__nd_sw_fetch_result(id,0);return;}\
Promise.resolve(captured).then(function(resp){\
if(!resp){__nd_sw_fetch_result(id,0);return;}\
return Promise.resolve(resp.arrayBuffer?resp.arrayBuffer():new ArrayBuffer(0)).then(function(ab){\
var hdrs=[];try{if(resp.headers&&resp.headers.forEach)resp.headers.forEach(function(v,k){hdrs.push(k);hdrs.push(v);});}catch(e){}\
__nd_sw_fetch_result(id,1,resp.status||200,resp.statusText||'',hdrs,new Uint8Array(ab));});\
}).catch(function(err){__nd_sw_fetch_result(id,2,String(err&&err.message||err));});\
};";

const INTERNAL: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

pub(crate) fn apply_state(scope: &mut Scope<'_>, host: &Host, sw: &Value, state: &str) {
    set_str(scope, sw, "state", state);
    let registration = get(scope, sw, "_registration");
    match state {
        "installed" => {
            if registration.is_object() {
                set(scope, &registration, "installing", Value::null());
                set(scope, &registration, "waiting", sw.clone());
            }
        }
        "activating" => {
            if registration.is_object() {
                set(scope, &registration, "waiting", Value::null());
            }
        }
        "activated" => {
            host.sw_active.store(true, Ordering::SeqCst);
            if registration.is_object() {
                set(scope, &registration, "waiting", Value::null());
                set(scope, &registration, "active", sw.clone());
            }
            let global = scope.global();
            let navigator = get(scope, &global, "navigator");
            let container = get(scope, &navigator, "serviceWorker");
            if container.is_object() {
                set(scope, &container, "controller", sw.clone());
                ffi::fire_event(scope, &container, "controllerchange");
                let resolve = get(scope, &container, "_readyResolve");
                if scope.is_function(&resolve) && registration.is_object() {
                    let _ = scope.call(
                        &resolve,
                        &Value::undefined(),
                        core::slice::from_ref(&registration),
                    );
                }
            }
        }
        _ => {}
    }
    ffi::fire_event(scope, sw, "statechange");
}

fn extendable_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    let event = ffi::event_new(scope);
    set_str(scope, &event, "type", kind);
    set(scope, &event, "bubbles", Value::boolean(false));
    set(scope, &event, "cancelable", Value::boolean(false));
    set(scope, &event, "defaultPrevented", Value::boolean(false));
    bind(scope, &event, "waitUntil", 1, crate::noop);
    ffi::bind_c(scope, &event, "preventDefault", 0, CFn::PreventDefault);
    ffi::bind_c(scope, &event, "stopPropagation", 0, CFn::StopPropagation);
    ffi::define_cancel_bubble(scope, &event);
    set(scope, &event, "_is_trusted", Value::boolean(true));
    ffi::bind_c(
        scope,
        &event,
        "stopImmediatePropagation",
        0,
        CFn::StopImmediate,
    );
    ffi::bind_c(scope, &event, "composedPath", 0, CFn::ComposedPath);
    event
}

pub(crate) fn fire_lifecycle(js: Js, host: &Arc<Host>) {
    ffi::with_js(js, |scope| {
        let global = scope.global();
        let install = extendable_event(scope, "install");
        ffi::dispatch_engine_event(scope, &global, &install);
        ffi::drain_microtasks(js);
        if !host.closing() {
            worker::post_owner_state(host, "installed");
            worker::post_owner_state(host, "activating");
            let activate = extendable_event(scope, "activate");
            ffi::dispatch_engine_event(scope, &global, &activate);
            ffi::drain_microtasks(js);
            worker::post_owner_state(host, "activated");
        }
    });
}

fn resolved(scope: &mut Scope<'_>, value: Value) -> JsResult {
    let (promise, resolve, _reject) = scope.new_promise()?;
    let _ = scope.call(&resolve, &Value::undefined(), &[value]);
    Ok(promise)
}

fn rejected(scope: &mut Scope<'_>, name: &str, message: &str) -> JsResult {
    let (promise, _resolve, reject) = scope.new_promise()?;
    let error = scope.new_error();
    set_str(scope, &error, "name", name);
    set_str(scope, &error, "message", message);
    let _ = scope.call(&reject, &Value::undefined(), &[error]);
    Ok(promise)
}

fn resolved_true(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    resolved(scope, Value::boolean(true))
}

fn default_scope(url: &str) -> String {
    match url.rfind('/') {
        Some(slash) => url[..=slash].to_owned(),
        None => url.to_owned(),
    }
}

fn registration_object(scope: &mut Scope<'_>, scope_url: &str, installing: Value) -> Value {
    let registration = scope.new_object();
    set_str(scope, &registration, "scope", scope_url);
    set(scope, &registration, "installing", installing);
    set(scope, &registration, "waiting", Value::null());
    set(scope, &registration, "active", Value::null());
    set_str(scope, &registration, "updateViaCache", "imports");
    let listeners = scope.new_array();
    set(scope, &registration, "_listeners", listeners);
    ffi::bind_event_target_listeners(scope, &registration);
    ffi::bind_c(scope, &registration, "dispatchEvent", 1, CFn::DispatchEvent);
    ffi::bind_c(scope, &registration, "update", 0, CFn::ResolvedUndefined);
    bind(scope, &registration, "unregister", 0, resolved_true);
    set(scope, &registration, "onupdatefound", Value::null());
    registration
}

pub(crate) fn register(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return rejected(scope, "InvalidStateError", "ServiceWorker unavailable");
    }
    let Some(raw) = args.first() else {
        return rejected(scope, "TypeError", "register requires a script URL");
    };
    let raw = scope.to_string(raw)?;
    let current = ffi::current_url(js);
    let Some(url) = ffi::url_resolve(current.as_deref(), &raw) else {
        return rejected(scope, "TypeError", "invalid script URL");
    };
    let options = arg(args, 1);
    let mut scope_url = None;
    if options.is_object() {
        let requested = get(scope, &options, "scope");
        if requested.is_string()
            && let Some(requested) = crate::text(scope, &requested)
        {
            scope_url = ffi::url_resolve(Some(current.as_deref().unwrap_or(&url)), &requested);
        }
    }
    let scope_url = scope_url.unwrap_or_else(|| default_scope(&url));
    if !ffi::url_same_origin(&url, &scope_url) {
        return rejected(
            scope,
            "SecurityError",
            "scope origin does not match the script URL",
        );
    }
    if let Err(error) = worker::script_url_allowed(current.as_deref(), &url, false) {
        let error = if error.is_empty() { "blocked" } else { &error };
        return rejected(scope, "SecurityError", error);
    }
    if !ffi::csp_allows_worker(js, &url) {
        return rejected(
            scope,
            "SecurityError",
            "blocked by Content-Security-Policy worker-src",
        );
    }
    let base_url = current.unwrap_or_else(|| url.clone());
    let origin = ffi::url_origin_from(&base_url).unwrap_or_default();
    let host = Host::new(HostSpec {
        url: url.clone(),
        base_url,
        name: String::new(),
        origin,
        inline_script: None,
        is_module: false,
        is_service_worker: true,
        scope: Some(scope_url.clone()),
    });
    let sw = scope.new_host_object(None, WorkerRef(Some(host.clone())));
    set_str(scope, &sw, "scriptURL", &url);
    set_str(scope, &sw, "state", "installing");
    set(scope, &sw, "onstatechange", Value::null());
    set(scope, &sw, "onerror", Value::null());
    let listeners = scope.new_array();
    set(scope, &sw, "_listeners", listeners);
    ffi::bind_event_target_listeners(scope, &sw);
    ffi::bind_c(scope, &sw, "dispatchEvent", 1, CFn::DispatchEvent);
    bind(scope, &sw, "postMessage", 1, worker::post_message);
    let registration = registration_object(scope, &scope_url, sw.clone());
    set(scope, &sw, "_registration", registration.clone());
    let registrations = get(scope, this, "_registrations");
    if registrations.is_array() {
        push(scope, &registrations, registration.clone());
    }
    worker::adopt(Owned {
        host: host.clone(),
        js,
        ctx: ffi::ctx_of(scope),
        object: sw,
    });
    worker::spawn(&host, "nd-service-worker");
    ffi::fire_event(scope, &registration, "updatefound");
    resolved(scope, registration)
}

fn get_registrations(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let registrations = get(scope, this, "_registrations");
    let out = scope.new_array();
    if registrations.is_array() {
        for i in 0..length(scope, &registrations) {
            let item = get_index(scope, &registrations, i);
            crate::set_index(scope, &out, i, item);
        }
    }
    resolved(scope, out)
}

fn get_registration(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let registrations = get(scope, this, "_registrations");
    let mut found = Value::undefined();
    if registrations.is_array() {
        let n = length(scope, &registrations);
        if n > 0 {
            found = get_index(scope, &registrations, n - 1);
        }
    }
    resolved(scope, found)
}

pub(crate) fn install_container(scope: &mut Scope<'_>, navigator: &Value) {
    let container = scope.new_object();
    set(scope, &container, "controller", Value::null());
    set(scope, &container, "oncontrollerchange", Value::null());
    set(scope, &container, "onmessage", Value::null());
    set(scope, &container, "onmessageerror", Value::null());
    let listeners = scope.new_array();
    set(scope, &container, "_listeners", listeners);
    let registrations = scope.new_array();
    set(scope, &container, "_registrations", registrations);
    ffi::bind_event_target_listeners(scope, &container);
    ffi::bind_c(scope, &container, "dispatchEvent", 1, CFn::DispatchEvent);
    bind(scope, &container, "register", 2, register);
    bind(scope, &container, "getRegistration", 1, get_registration);
    bind(scope, &container, "getRegistrations", 0, get_registrations);
    bind(scope, &container, "startMessages", 0, crate::noop);
    if let Ok((ready, resolve, _reject)) = scope.new_promise() {
        set(scope, &container, "ready", ready);
        set(scope, &container, "_readyResolve", resolve);
    }
    set(scope, navigator, "serviceWorker", container);
}

pub(crate) fn controller_for(js: Js, url: &str) -> Option<Arc<Host>> {
    worker::hosts_of(js).into_iter().find(|h| {
        h.is_service_worker
            && !h.closing()
            && h.sw_active.load(Ordering::SeqCst)
            && h.scope
                .as_deref()
                .is_some_and(|s| !s.is_empty() && url.starts_with(s))
    })
}

pub(crate) struct FetchRequest {
    pub id: u32,
    pub url: String,
    pub method: String,
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

pub(crate) struct FetchResult {
    pub id: u32,
    pub outcome: i32,
    pub status: i32,
    pub raw_headers: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub error: Option<String>,
}

fn post_result(host: &Arc<Host>, result: FetchResult) {
    let host = host.clone();
    ffi::invoke_default(move || {
        if !host.owner_alive.load(Ordering::SeqCst) {
            return;
        }
        if let Some((js, _, _)) = worker::owner_of(&host) {
            ffi::sw_fetch_deliver(js, &result);
        }
    });
}

fn report_unhandled(host: &Arc<Host>, id: u32) {
    post_result(
        host,
        FetchResult {
            id,
            outcome: 0,
            status: 0,
            raw_headers: None,
            content_type: None,
            body: Vec::new(),
            error: None,
        },
    );
}

fn dispatch_fetch(host: Arc<Host>, request: FetchRequest) {
    let js = host.worker_js();
    if host.closing() || js.is_null() {
        report_unhandled(&host, request.id);
        return;
    }
    let dispatched = ffi::with_js(js, |scope| {
        let budget = ffi::Budget::enter(scope);
        let global = scope.global();
        let function = get(scope, &global, "__nd_sw_dispatch_fetch");
        let mut dispatched = false;
        if scope.is_function(&function) {
            let headers = scope.new_array();
            for (i, header) in request.headers.iter().enumerate() {
                let header = scope.string(header);
                crate::set_index(scope, &headers, i as u32, header);
            }
            let body = if request.body.is_empty() {
                Value::null()
            } else {
                scope
                    .new_array_buffer(&request.body)
                    .unwrap_or_else(|_| Value::null())
            };
            let url = scope.string(&request.url);
            let method = scope.string(if request.method.is_empty() {
                "GET"
            } else {
                &request.method
            });
            let args = [
                Value::int64(i64::from(request.id)),
                url,
                method,
                headers,
                body,
            ];
            dispatched = scope.call(&function, &Value::undefined(), &args).is_ok();
        }
        ffi::drain_microtasks(js);
        drop(budget);
        dispatched
    })
    .unwrap_or(false);
    if !dispatched {
        report_unhandled(&host, request.id);
    }
}

pub(crate) fn post_fetch_request(host: &Arc<Host>, request: FetchRequest) {
    let target = host.clone();
    host.context.invoke(move || dispatch_fetch(target, request));
}

fn fetch_result(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let host = ffi::worker_host_of(ffi::js_of(scope));
    let Some(host) = host.filter(|_| args.len() >= 2) else {
        return Ok(Value::undefined());
    };
    let mut result = FetchResult {
        id: scope.to_int64(&args[0]).unwrap_or(0) as u32,
        outcome: scope.to_int32(&args[1]).unwrap_or(0),
        status: 0,
        raw_headers: None,
        content_type: None,
        body: Vec::new(),
        error: None,
    };
    if result.outcome == 1 {
        result.status = args
            .get(2)
            .map_or(200, |status| scope.to_int32(status).unwrap_or(200));
        if let Some(pairs) = args.get(4).filter(|p| p.is_array()) {
            let mut raw = String::new();
            let n = length(scope, pairs);
            let mut i = 0;
            while i + 1 < n {
                let key = get_index(scope, pairs, i);
                let value = get_index(scope, pairs, i + 1);
                if let (Some(key), Some(value)) =
                    (crate::text(scope, &key), crate::text(scope, &value))
                {
                    raw.push_str(&format!("{key}: {value}\r\n"));
                    if result.content_type.is_none() && key.eq_ignore_ascii_case("content-type") {
                        result.content_type = Some(value);
                    }
                }
                i += 2;
            }
            result.raw_headers = Some(raw);
        }
        if let Some(body) = args.get(5).filter(|b| b.is_object())
            && let Ok(Some(view)) = scope.typed_array_view(body)
            && let Some(bytes) = scope.array_buffer_bytes(&view.buffer)
        {
            let len = view.length * element_size(&view.element);
            if let Some(slice) = bytes.get(view.byte_offset..view.byte_offset + len)
                && !slice.is_empty()
            {
                result.body = slice.to_vec();
            }
        }
    } else if result.outcome == 2
        && let Some(error) = args.get(2).filter(|e| e.is_string())
    {
        result.error = crate::text(scope, error);
    }
    post_result(&host, result);
    Ok(Value::undefined())
}

fn element_size(element: &southstar_js_engine::ElementType) -> usize {
    use southstar_js_engine::ElementType::*;
    match element {
        Int8 | Uint8 | Uint8Clamped => 1,
        Int16 | Uint16 | Float16 => 2,
        Int32 | Uint32 | Float32 => 4,
        BigInt64 | BigUint64 | Float64 => 8,
    }
}

pub(crate) fn install_scope(scope: &mut Scope<'_>, global: &Value, host: &Host) {
    ffi::bind_ctor(
        scope,
        global,
        "ServiceWorkerGlobalScope",
        0,
        ffi::CCtor::Illegal,
    );
    ffi::bind_c(scope, global, "skipWaiting", 0, CFn::ResolvedUndefined);
    for handler in [
        "oninstall",
        "onactivate",
        "onfetch",
        "onpush",
        "onnotificationclick",
    ] {
        set(scope, global, handler, Value::null());
    }
    let registration = scope.new_object();
    set_str(
        scope,
        &registration,
        "scope",
        host.scope.as_deref().unwrap_or(""),
    );
    set(scope, &registration, "installing", Value::null());
    set(scope, &registration, "waiting", Value::null());
    set(scope, &registration, "active", Value::null());
    set_str(scope, &registration, "updateViaCache", "imports");
    let listeners = scope.new_array();
    set(scope, &registration, "_listeners", listeners);
    ffi::bind_event_target_listeners(scope, &registration);
    ffi::bind_c(scope, &registration, "dispatchEvent", 1, CFn::DispatchEvent);
    ffi::bind_c(scope, &registration, "update", 0, CFn::ResolvedUndefined);
    bind(scope, &registration, "unregister", 0, resolved_true);
    set(scope, &registration, "onupdatefound", Value::null());
    let preload = scope.new_object();
    ffi::bind_c(scope, &preload, "enable", 0, CFn::ResolvedUndefined);
    ffi::bind_c(scope, &preload, "disable", 0, CFn::ResolvedUndefined);
    ffi::bind_c(scope, &preload, "getState", 0, CFn::ResolvedUndefined);
    ffi::bind_c(scope, &preload, "setHeaderValue", 1, CFn::ResolvedUndefined);
    set(scope, &registration, "navigationPreload", preload);
    set(scope, global, "registration", registration);

    let clients = scope.new_object();
    ffi::bind_c(scope, &clients, "claim", 0, CFn::ResolvedUndefined);
    ffi::bind_c(scope, &clients, "matchAll", 1, CFn::ResolvedEmptyArray);
    ffi::bind_c(scope, &clients, "get", 1, CFn::ResolvedUndefined);
    ffi::bind_c(scope, &clients, "openWindow", 1, CFn::ResolvedUndefined);
    set(scope, global, "clients", clients);

    let caches = scope.new_object();
    ffi::bind_c(scope, &caches, "open", 1, CFn::CacheOpen);
    ffi::bind_c(scope, &caches, "has", 1, CFn::ResolvedFalse);
    ffi::bind_c(scope, &caches, "delete", 1, CFn::ResolvedFalse);
    ffi::bind_c(scope, &caches, "keys", 0, CFn::ResolvedEmptyArray);
    ffi::bind_c(scope, &caches, "match", 2, CFn::ResolvedUndefined);
    set(scope, global, "caches", caches);

    ffi::bind_ctor(scope, global, "ExtendableEvent", 2, ffi::CCtor::Event);
    ffi::bind_ctor(scope, global, "FetchEvent", 2, ffi::CCtor::Event);
    ffi::bind_ctor(
        scope,
        global,
        "ExtendableMessageEvent",
        2,
        ffi::CCtor::Event,
    );
    ffi::bind_ctor(scope, global, "Response", 0, ffi::CCtor::Response);
    ffi::bind_ctor(scope, global, "Request", 1, ffi::CCtor::Request);
    ffi::fetch_install_interfaces(scope, global);

    let result = scope.function("__nd_sw_fetch_result", 6, fetch_result);
    let _ = scope.define(global, "__nd_sw_fetch_result", result, INTERNAL);
    let _ = scope.eval_native_script(FETCH_DISPATCH, "<sw-fetch-dispatch>");
}
