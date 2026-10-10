//! Southstar — fetch(): request extraction, the network round trip, abort signals, service worker routing and the Response it settles with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_js_engine::quickjs::JSContext;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Cancellable, Js, Request, Response, cstring};
use crate::headers;
use crate::{JsResult, c_bytes, is_nullish, prop, set, set_str, string_prop};

const BODY_BYTES_MAX: usize = 32 * 1024 * 1024;

const DRAIN_SOURCE: &str = "(function(stream){\
 var rd = stream.getReader(), chunks = [], total = 0;\
 function pump(){ return rd.read().then(function(x){\
  if (x.done) { var out = new Uint8Array(total), o = 0;\
   for (var i = 0; i < chunks.length; i++) {\
    out.set(chunks[i], o); o += chunks[i].length; }\
   return out.buffer; }\
  var c = (x.value instanceof Uint8Array)\
   ? x.value : new Uint8Array(x.value);\
  chunks.push(c); total += c.length; return pump(); }); }\
 return pump();\
})";

pub(crate) struct FetchState {
    ctx: *mut JSContext,
    timeline: *mut JSContext,
    resolve: Value,
    reject: Value,
    requested_url: Vec<u8>,
    origin_url: Option<Vec<u8>>,
    cancellable: Option<Cancellable>,
    signal: Option<Value>,
    abort_handler: Option<Value>,
    settled: bool,
    no_cors: bool,
    start_ms: f64,
    fallback: Option<Request>,
}

pub(crate) struct Ticket {
    js: Js,
    id: u32,
}

pub(crate) struct Delivery {
    js: Js,
    id: u32,
    resp: Response,
    error: Option<Vec<u8>>,
}

pub(crate) struct SwOutcome<'a> {
    pub outcome: i32,
    pub status: i64,
    pub content_type: Option<Vec<u8>>,
    pub raw_headers: Option<Vec<u8>>,
    pub body: &'a [u8],
    pub error: Option<Vec<u8>>,
}

fn body_stream(scope: &mut Scope<'_>, object: &Value) -> Option<Value> {
    if !object.is_object() {
        return None;
    }
    if prop(scope, object, "_bodyBuffer").is_object() {
        return None;
    }
    let mut stream = prop(scope, object, "_bodyStream");
    if !stream.is_object() {
        stream = scope.get(object, "body").ok()?;
    }
    if !stream.is_object() {
        return None;
    }
    let get_reader = scope.get(&stream, "getReader").ok()?;
    scope.is_function(&get_reader).then_some(stream)
}

fn drained(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    if let Some(buffer) = args.first().filter(|b| b.is_object()) {
        let _ = scope.define(&data[2], "_bodyBuffer", buffer.clone(), Attributes::METHOD);
    }
    let count = if data[1].is_undefined() { 1 } else { 2 };
    match fetch(scope, &data[5], &data[..count]) {
        Ok(inner) => crate::call_ignoring(scope, &data[3], &Value::undefined(), &[inner]),
        Err(error) => crate::call_ignoring(scope, &data[4], &Value::undefined(), &[error]),
    }
    Ok(Value::undefined())
}

fn drain_failed(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let reason = args.first().cloned().unwrap_or_else(Value::undefined);
    crate::call_ignoring(scope, &data[4], &Value::undefined(), &[reason]);
    Ok(Value::undefined())
}

fn defer_stream_body(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    carrier: &Value,
    stream: Value,
) -> JsResult {
    let (promise, resolve, reject) = scope.new_promise()?;
    let drain_promise = scope
        .eval_native_script(DRAIN_SOURCE, "<fetch-drain>")
        .and_then(|drain| scope.call(&drain, &Value::undefined(), &[stream]));
    let then = drain_promise
        .as_ref()
        .ok()
        .map(|p| prop(scope, p, "then"))
        .filter(|then| scope.is_function(then));
    let (Ok(drain_promise), Some(then)) = (drain_promise, then) else {
        let error = scope.new_error();
        set_str(
            scope,
            &error,
            "message",
            b"fetch: could not read body stream",
        );
        crate::call_ignoring(scope, &reject, &Value::undefined(), &[error]);
        return Ok(promise);
    };
    let data = [
        args[0].clone(),
        args.get(1).cloned().unwrap_or_else(Value::undefined),
        carrier.clone(),
        resolve,
        reject,
        this.clone(),
    ];
    let on_fulfilled = scope.bound_function("", 1, drained, &data);
    let on_rejected = scope.bound_function("", 1, drain_failed, &data);
    crate::call_ignoring(scope, &then, &drain_promise, &[on_fulfilled, on_rejected]);
    Ok(promise)
}

fn copy_body(bytes: Vec<u8>) -> Option<Vec<u8>> {
    (bytes.len() <= BODY_BYTES_MAX).then_some(bytes)
}

fn read_body(
    scope: &mut Scope<'_>,
    object: &Value,
    body: &mut Option<Vec<u8>>,
    content_type: &mut Option<Vec<u8>>,
) {
    if let Ok(buffer) = scope.get(object, "_bodyBuffer")
        && buffer.is_object()
        && let Some(bytes) = scope.array_buffer_bytes(&buffer)
    {
        *body = copy_body(bytes);
        return;
    }
    let Ok(value) = scope.get(object, "body") else {
        return;
    };
    if is_nullish(&value) {
        return;
    }
    if let Some(serialized) = ffi::serialize_form_body(scope, &value) {
        *body = Some(serialized.body);
        *content_type = serialized.content_type;
        return;
    }
    *body = ffi::body_bytes(scope, &value);
    if body.is_some() && content_type.is_none() && value.is_object() {
        let Ok(kind) = scope.get(&value, "type") else {
            return;
        };
        if kind.is_string()
            && let Some(kind) = c_bytes(scope, &kind)
            && !kind.is_empty()
            && headers::value_is_safe(&kind)
        {
            *content_type = Some(kind);
        }
    }
}

fn collect_headers(
    scope: &mut Scope<'_>,
    headers_value: &Value,
    extras: &mut Vec<Vec<u8>>,
    content_type: &mut Option<Vec<u8>>,
) {
    let map = if headers_value.is_object() {
        prop(scope, headers_value, "__ndHeaderMap")
    } else {
        Value::undefined()
    };
    let source = if map.is_object() { &map } else { headers_value };
    if !source.is_object() {
        return;
    }
    let Ok(keys) = scope.own_enumerable_keys(source) else {
        return;
    };
    for key in keys {
        let Some(name) = c_bytes(scope, &key) else {
            continue;
        };
        let Ok(value) = scope.get_key(source, &key) else {
            continue;
        };
        let Some(value) = c_bytes(scope, &value) else {
            continue;
        };
        if name.eq_ignore_ascii_case(b"content-type") {
            if headers::value_is_safe(&value) {
                *content_type = Some(value);
            }
        } else if headers::is_token(&name)
            && !headers::is_forbidden(&name)
            && headers::value_is_safe(&value)
        {
            let mut line = name;
            line.extend_from_slice(b": ");
            line.extend_from_slice(&value);
            extras.push(line);
        }
    }
}

fn has_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    scope.get(object, key).is_ok_and(|v| !v.is_undefined())
}

fn url_source(scope: &mut Scope<'_>, input: &Value) -> Value {
    if !input.is_object() {
        return input.clone();
    }
    let slot = prop(scope, input, "__ns_url");
    if slot.is_string() {
        return slot;
    }
    let href = prop(scope, input, "href");
    if href.is_string() {
        return href;
    }
    let url = prop(scope, input, "url");
    if url.is_string() {
        return url;
    }
    input.clone()
}

fn base_url(scope: &Scope<'_>, js: Js) -> Option<Vec<u8>> {
    let ctx = ffi::context_of(scope);
    if js.is_null() || ctx == js.main_context() {
        return None;
    }
    js.realm_url(ctx)
}

fn reject_with_abort(scope: &mut Scope<'_>, reject: &Value, signal: Option<&Value>) {
    let mut reason = signal
        .filter(|s| s.is_object())
        .map_or_else(Value::undefined, |s| prop(scope, s, "reason"));
    if is_nullish(&reason) {
        reason = ffi::abort_error(scope);
    }
    crate::call_ignoring(scope, reject, &Value::undefined(), &[reason]);
}

fn on_signal_abort(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
    data: &[Value],
) -> JsResult {
    let id = scope.to_int64(&data[0]).unwrap_or(0) as u32;
    let js = Js::of(scope);
    let taken = crate::existing_page(js, |page| {
        let state = page.fetches.get_mut(&id)?;
        if state.settled {
            return None;
        }
        state.settled = true;
        if let Some(cancellable) = &state.cancellable {
            cancellable.cancel();
        }
        Some((state.reject.clone(), state.signal.clone()))
    })
    .flatten();
    if let Some((reject, signal)) = taken {
        reject_with_abort(scope, &reject, signal.as_ref());
    }
    Ok(Value::undefined())
}

fn mode_is_no_cors(scope: &mut Scope<'_>, object: &Value, current: bool) -> bool {
    match string_prop(scope, object, "mode") {
        Some(mode) => mode == b"no-cors",
        None => current,
    }
}

pub(crate) fn fetch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = Js::of(scope);
    if js.is_null() || args.is_empty() {
        return Err(scope.type_error("fetch requires a URL"));
    }
    let input = &args[0];
    let init = args.get(1).filter(|i| i.is_object());
    if let Some(init) = init
        && let Some(stream) = body_stream(scope, init)
    {
        return defer_stream_body(scope, this, args, init, stream);
    }
    if input.is_object()
        && let Some(stream) = body_stream(scope, input)
    {
        return defer_stream_body(scope, this, args, input, stream);
    }
    let (promise, resolve, reject) = scope.new_promise()?;
    let source = url_source(scope, input);
    let Some(url) = c_bytes(scope, &source) else {
        return Ok(promise);
    };
    let mut method: Option<Vec<u8>> = None;
    let mut body: Option<Vec<u8>> = None;
    let mut content_type: Option<Vec<u8>> = None;
    let base = base_url(scope, js);
    let mut extras: Vec<Vec<u8>> = Vec::new();
    let init_has_headers = init.is_some_and(|init| has_prop(scope, init, "headers"));
    if input.is_object() {
        let explicit = string_prop(scope, input, "__ns_method").filter(|m| !m.is_empty());
        let m = explicit.or_else(|| string_prop(scope, input, "method"));
        if let Some(m) = m.filter(|m| !m.is_empty()) {
            method = headers::normalize_method(&m);
        }
        read_body(scope, input, &mut body, &mut content_type);
        if !init_has_headers {
            let h = prop(scope, input, "headers");
            collect_headers(scope, &h, &mut extras, &mut content_type);
        }
    }
    if let Some(init) = init {
        if let Some(m) = string_prop(scope, init, "method") {
            method = headers::normalize_method(&m);
        }
        read_body(scope, init, &mut body, &mut content_type);
        let h = prop(scope, init, "headers");
        collect_headers(scope, &h, &mut extras, &mut content_type);
    }
    let mut no_cors = false;
    if input.is_object() {
        no_cors = mode_is_no_cors(scope, input, no_cors);
    }
    if let Some(init) = init {
        no_cors = mode_is_no_cors(scope, init, no_cors);
    }
    if let Some(m) = method
        .as_deref()
        .filter(|m| headers::is_forbidden_method(m))
    {
        let message = format!(
            "fetch: '{}' is a forbidden method",
            String::from_utf8_lossy(m)
        );
        let error = scope.type_error(&message);
        crate::call_ignoring(scope, &reject, &Value::undefined(), &[error]);
        return Ok(promise);
    }
    if body.is_some() && content_type.is_none() {
        content_type = Some(b"text/plain;charset=UTF-8".to_vec());
    }
    let top = base.filter(|b| !b.is_empty()).or_else(|| js.page_url());
    let mut state = FetchState {
        ctx: ffi::context_of(scope),
        timeline: js.main_context(),
        resolve,
        reject,
        requested_url: url.clone(),
        origin_url: top.clone(),
        cancellable: None,
        signal: None,
        abort_handler: None,
        settled: false,
        no_cors,
        start_ms: js.perf_now_ms(),
        fallback: None,
    };
    let id = crate::next_id();
    let signal = init
        .map(|init| prop(scope, init, "signal"))
        .filter(|s| s.is_object());
    if let Some(signal) = signal {
        if crate::bool_prop(scope, &signal, "aborted") {
            reject_with_abort(scope, &state.reject, Some(&signal));
            return Ok(promise);
        }
        let handler = scope.bound_function("", 0, on_signal_abort, &[Value::int64(id as i64)]);
        state.cancellable = Some(Cancellable::new());
        state.signal = Some(signal.clone());
        state.abort_handler = Some(handler.clone());
        crate::with_page(js, |page| page.fetches.insert(id, state));
        let add = prop(scope, &signal, "addEventListener");
        if scope.is_function(&add) {
            let kind = scope.string("abort");
            crate::call_ignoring(scope, &add, &signal, &[kind, handler]);
        }
    } else {
        crate::with_page(js, |page| page.fetches.insert(id, state));
    }
    if url.starts_with(b"blob:") {
        let mut resp = Response::synthesized();
        match js.blob_url(&url) {
            Some((bytes, kind)) => {
                resp.set_status(200);
                resp.set_final_url(&url);
                let kind = kind
                    .filter(|k| !k.is_empty())
                    .unwrap_or_else(|| b"application/octet-stream".to_vec());
                resp.set_content_type(&kind);
                resp.set_body(&bytes);
            }
            None => resp.set_error(b"blob URL not found"),
        }
        ffi::schedule_fetch_idle(
            js,
            Delivery {
                js,
                id,
                resp,
                error: None,
            },
            false,
        );
        return Ok(promise);
    }
    let send_url = top
        .as_deref()
        .and_then(|top| ffi::url_resolve(Some(top), &url))
        .unwrap_or_else(|| url.clone());
    let use_method = method.unwrap_or_else(|| b"GET".to_vec());
    let blocked_mixed = top
        .as_deref()
        .is_some_and(|top| crate::ascii_starts_with(top, b"https://"))
        && crate::ascii_starts_with(&send_url, b"http://");
    let blocked_csp = !js.csp_allows_connect(&send_url, top.as_deref());
    if blocked_mixed || blocked_csp {
        let mut resp = Response::synthesized();
        resp.set_error(if blocked_mixed {
            b"blocked as mixed content (http resource on https page)"
        } else {
            &b"blocked by Content-Security-Policy connect-src"[..]
        });
        ffi::schedule_fetch_idle(
            js,
            Delivery {
                js,
                id,
                resp,
                error: None,
            },
            false,
        );
        return Ok(promise);
    }
    let request = Request {
        url: cstring(&send_url),
        top: top.as_deref().map(cstring),
        method: cstring(&use_method),
        has_body: body.is_some(),
        body: body.unwrap_or_default(),
        content_type: content_type.as_deref().map(cstring),
        headers: extras.iter().map(|h| cstring(h)).collect::<Vec<CString>>(),
    };
    if let Some(worker) = js.service_worker_for(&send_url) {
        worker.post_fetch(id, &request);
        let fallback = Request {
            top: Some(cstring(top.as_deref().unwrap_or_default())),
            has_body: request.has_body && !request.body.is_empty(),
            ..request
        };
        crate::existing_page(js, |page| {
            if let Some(state) = page.fetches.get_mut(&id) {
                state.fallback = Some(fallback);
            }
        });
        return Ok(promise);
    }
    let cancellable = crate::existing_page(js, |page| {
        page.fetches
            .get(&id)
            .and_then(|s| s.cancellable.as_ref().map(Cancellable::share))
    })
    .flatten();
    let ticket = Box::into_raw(Box::new(Ticket { js, id }));
    request.send(cancellable.as_ref(), ffi::on_fetch_done, ticket.cast());
    Ok(promise)
}

fn is_live(js: Js, id: u32) -> bool {
    crate::existing_page(js, |page| page.fetches.contains_key(&id)).unwrap_or(false)
}

pub(crate) fn done(ticket: Ticket, resp: Response, error: Option<Vec<u8>>) {
    let Ticket { js, id } = ticket;
    if !is_live(js, id) {
        return;
    }
    let delivery = Delivery {
        js,
        id,
        resp,
        error,
    };
    if js.in_pump() {
        ffi::schedule_fetch_idle(js, delivery, false);
        return;
    }
    deliver(delivery);
}

pub(crate) fn deliver_idle(delivery: Delivery) {
    if is_live(delivery.js, delivery.id) && delivery.js.in_pump() {
        ffi::schedule_fetch_idle(delivery.js, delivery, true);
        return;
    }
    deliver(delivery);
}

pub(crate) fn final_url_connect_blocked(js: Js, final_url: Option<&[u8]>) -> bool {
    let Some(final_url) = final_url.filter(|u| !u.is_empty()) else {
        return false;
    };
    let page = js.page_url();
    let mixed = page
        .as_deref()
        .is_some_and(|p| crate::ascii_starts_with(p, b"https://"))
        && crate::ascii_starts_with(final_url, b"http://");
    mixed || !js.csp_allows_connect(final_url, page.as_deref())
}

fn add_raw_headers(scope: &mut Scope<'_>, init: &Value, raw: &[u8]) {
    for (name, value) in headers::raw_header_lines(raw) {
        if name.is_empty() {
            continue;
        }
        let name = String::from_utf8_lossy(&name.to_ascii_lowercase()).into_owned();
        let previous = prop(scope, init, &name);
        let joined = if previous.is_string() {
            c_bytes(scope, &previous).map(|mut old| {
                old.extend_from_slice(b", ");
                old.extend_from_slice(value);
                old
            })
        } else {
            None
        };
        let text = scope.string_from_bytes(joined.as_deref().unwrap_or(value));
        set(scope, init, &name, text);
    }
}

pub(crate) fn make_headers(scope: &mut Scope<'_>, init: &Value) -> Value {
    let ctor = crate::global_ctor(scope, "Headers");
    if scope.is_constructor(&ctor)
        && let Ok(headers) = scope.construct(&ctor, core::slice::from_ref(init))
    {
        return headers;
    }
    scope.new_object()
}

fn resolve_response(scope: &mut Scope<'_>, js: Js, state: &FetchState, resp: &Response) {
    let origin_url = state.origin_url.clone().or_else(|| js.page_url());
    let final_url = resp.final_url();
    let allow = headers::cors_allows(
        origin_url.as_deref(),
        final_url.as_deref(),
        resp.cors_allow_origin().as_deref(),
    );
    if !allow && !state.no_cors {
        let error = scope.type_error("Failed to fetch: CORS request blocked");
        crate::call_ignoring(scope, &state.reject, &Value::undefined(), &[error]);
        return;
    }
    let r = scope.new_object();
    let status = if allow { resp.status() } else { 0 };
    set(
        scope,
        &r,
        "ok",
        Value::boolean(allow && (200..300).contains(&resp.status())),
    );
    set(scope, &r, "status", Value::int(status as i32));
    set_str(
        scope,
        &r,
        "statusText",
        headers::status_text(status as i32).as_bytes(),
    );
    let url = if allow {
        final_url.clone().unwrap_or_default()
    } else {
        Vec::new()
    };
    set_str(scope, &r, "url", &url);
    let kind: &[u8] = if !allow {
        b"opaque"
    } else if ffi::url_same_origin(origin_url.as_deref(), final_url.as_deref()) {
        b"basic"
    } else {
        b"cors"
    };
    set_str(scope, &r, "type", kind);
    set(
        scope,
        &r,
        "redirected",
        Value::boolean(resp.redirect_count() > 0),
    );
    set(
        scope,
        &r,
        "redirectCount",
        Value::int(if allow { resp.redirect_count() } else { 0 }),
    );
    set(scope, &r, "bodyUsed", Value::boolean(false));
    let body: &[u8] = if allow { resp.body() } else { &[] };
    if let Ok(buffer) = scope.new_array_buffer(body) {
        set(scope, &r, "_bodyBuffer", buffer);
    }
    let init = scope.new_object();
    if allow {
        let same_origin = ffi::url_same_origin(js.page_url().as_deref(), final_url.as_deref());
        let raw = if same_origin {
            resp.raw_headers()
        } else {
            None
        };
        if let Some(raw) = &raw {
            add_raw_headers(scope, &init, raw);
        }
        for (name, value) in resp.known_headers() {
            let Some(value) = value.filter(|v| !v.is_empty()) else {
                continue;
            };
            if headers::raw_headers_have(raw.as_deref(), name) {
                continue;
            }
            let name = String::from_utf8_lossy(name).into_owned();
            set_str(scope, &init, &name, &value);
        }
    }
    let headers_object = make_headers(scope, &init);
    set(scope, &r, "headers", headers_object);
    crate::body::attach_consumers(scope, &r);
    crate::call_ignoring(scope, &state.resolve, &Value::undefined(), &[r]);
}

fn finish(state: FetchState) {
    if let (Some(signal), Some(handler)) = (&state.signal, &state.abort_handler) {
        ffi::with_context(state.ctx, |scope| {
            let kind = scope.string("abort");
            crate::call_method(
                scope,
                signal,
                "removeEventListener",
                &[kind, handler.clone()],
            );
        });
    }
}

fn deliver(delivery: Delivery) {
    let Delivery {
        js,
        id,
        mut resp,
        error,
    } = delivery;
    let Some(state) = crate::existing_page(js, |page| page.fetches.remove(&id)).flatten() else {
        return;
    };
    js.with_budget(|| {
        if state.settled {
            return;
        }
        let end_us = ffi::monotonic_us();
        let start_us = if state.start_ms > 0.0 {
            js.time_origin_us() + (state.start_ms * 1000.0) as i64
        } else {
            end_us
        };
        let absolute = state
            .origin_url
            .as_deref()
            .and_then(|origin| ffi::url_resolve(Some(origin), &state.requested_url));
        js.add_resource_timing(
            &ffi::Timing {
                timeline: state.timeline,
                cors_mode: !state.no_cors,
                url: absolute.as_deref().unwrap_or(&state.requested_url),
                initiator: c"fetch",
                start_us,
                end_us,
            },
            &resp,
        );
        if !resp.is_null()
            && resp.error().is_none()
            && final_url_connect_blocked(js, resp.final_url().as_deref())
        {
            resp.set_error(b"blocked after redirect (mixed content or connect-src CSP)");
        }
        ffi::with_context(state.ctx, |scope| {
            let failure = if resp.is_null() {
                Some(error.clone().unwrap_or_else(|| b"fetch failed".to_vec()))
            } else {
                resp.error()
            };
            match failure {
                Some(message) => {
                    let message = String::from_utf8_lossy(&message).into_owned();
                    let error = scope.type_error(&message);
                    crate::call_ignoring(scope, &state.reject, &Value::undefined(), &[error]);
                }
                None => resolve_response(scope, js, &state, &resp),
            }
        });
        js.drain_mutations();
    });
    drop(resp);
    finish(state);
}

pub(crate) fn service_worker_result(js: Js, id: u32, result: SwOutcome<'_>) {
    let taken = crate::existing_page(js, |page| {
        let state = page.fetches.get_mut(&id)?;
        let fallback = state.fallback.take()?;
        let cancellable = state.cancellable.as_ref().map(Cancellable::share);
        Some((fallback, cancellable))
    })
    .flatten();
    let Some((fallback, cancellable)) = taken else {
        return;
    };
    match result.outcome {
        1 => {
            let mut resp = Response::synthesized();
            resp.set_status(if result.status > 0 {
                result.status
            } else {
                200
            });
            resp.set_final_url(fallback.url.to_bytes());
            resp.set_content_type(
                result
                    .content_type
                    .as_deref()
                    .unwrap_or(b"text/plain;charset=UTF-8"),
            );
            resp.set_cors_allow_origin(b"*");
            if let Some(raw) = &result.raw_headers {
                resp.set_raw_headers(raw);
            }
            resp.set_body(result.body);
            ffi::schedule_fetch_idle(
                js,
                Delivery {
                    js,
                    id,
                    resp,
                    error: None,
                },
                false,
            );
        }
        2 => {
            let mut resp = Response::synthesized();
            resp.set_error(
                result
                    .error
                    .as_deref()
                    .unwrap_or(b"ServiceWorker fetch handler failed"),
            );
            ffi::schedule_fetch_idle(
                js,
                Delivery {
                    js,
                    id,
                    resp,
                    error: None,
                },
                false,
            );
        }
        _ => {
            let ticket = Box::into_raw(Box::new(Ticket { js, id }));
            fallback.send(cancellable.as_ref(), ffi::on_fetch_done, ticket.cast());
        }
    }
}
