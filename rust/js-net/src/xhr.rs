//! Southstar — XMLHttpRequest and XMLHttpRequestUpload: open(), send(), the response types and the readystatechange and progress events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, HO_XHR, HO_XHR_UPLOAD, Js, Realm, Request, Response, cstring};
use crate::headers;
use crate::{JsResult, bool_prop, c_bytes, int_prop, is_nullish, prop, set, set_str};

const SYNC_DEADLINE_US: i64 = 60 * 1_000_000;

const XHR_HANDLERS: [&str; 8] = [
    "onreadystatechange",
    "onloadstart",
    "onprogress",
    "onabort",
    "onerror",
    "onload",
    "ontimeout",
    "onloadend",
];

const UPLOAD_HANDLERS: [&str; 7] = [
    "onloadstart",
    "onprogress",
    "onabort",
    "onerror",
    "onload",
    "ontimeout",
    "onloadend",
];

pub(crate) struct XhrState {
    realm: Realm,
    timeline: Realm,
    obj: Value,
    url: Vec<u8>,
    origin_url: Option<Vec<u8>>,
    start_ms: f64,
    generation: i64,
}

pub(crate) struct Ticket {
    js: Js,
    id: u32,
}

pub(crate) struct Delivery {
    js: Js,
    id: u32,
    resp: Response,
    failed: bool,
}

fn illegal(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

fn check(scope: &mut Scope<'_>, this: &Value) -> JsResult<()> {
    if ffi::host_is(this, HO_XHR) {
        Ok(())
    } else {
        Err(illegal(scope))
    }
}

fn generation(scope: &mut Scope<'_>, obj: &Value) -> i64 {
    let value = prop(scope, obj, "_gen");
    scope.to_int64(&value).unwrap_or(0)
}

fn bump_generation(scope: &mut Scope<'_>, obj: &Value) {
    let next = generation(scope, obj) + 1;
    set(scope, obj, "_gen", Value::int64(next));
}

fn set_ready_state(scope: &mut Scope<'_>, obj: &Value, state: i32) {
    set(scope, obj, "_readyState", Value::int(state));
}

fn progress(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    loaded: f64,
    total: f64,
    computable: bool,
) {
    ffi::fire_progress_event(
        scope,
        target,
        &cstring(kind.as_bytes()),
        loaded,
        total,
        computable,
    );
}

fn reset_response(scope: &mut Scope<'_>, obj: &Value) {
    set(scope, obj, "status", Value::int(0));
    for key in ["statusText", "responseText", "response"] {
        set_str(scope, obj, key, b"");
    }
    set(scope, obj, "responseXML", Value::null());
    set_str(scope, obj, "responseURL", b"");
    set_str(scope, obj, "_responseHeaders", b"");
}

fn get_response_header(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    check(scope, this)?;
    let Some(name) = args.first().and_then(|n| c_bytes(scope, n)) else {
        return Ok(Value::null());
    };
    let all = prop(scope, this, "_responseHeaders");
    let Some(all) = c_bytes(scope, &all) else {
        return Ok(Value::null());
    };
    Ok(headers::raw_header_lines(&all)
        .find(|(n, _)| n.len() == name.len() && n.eq_ignore_ascii_case(&name))
        .map_or_else(Value::null, |(_, v)| scope.string_from_bytes(v)))
}

fn get_all_response_headers(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    check(scope, this)?;
    let all = prop(scope, this, "_responseHeaders");
    if all.is_string() {
        Ok(all)
    } else {
        Ok(scope.string(""))
    }
}

fn abort(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    check(scope, this)?;
    let mut state = int_prop(scope, this, "_readyState");
    let sent = bool_prop(scope, this, "_sendFlag");
    if (state == 1 && sent) || state == 2 || state == 3 {
        bump_generation(scope, this);
        set(scope, this, "_aborted", Value::boolean(true));
        set(scope, this, "_sendFlag", Value::boolean(false));
        set_ready_state(scope, this, 4);
        set(scope, this, "status", Value::int(0));
        ffi::fire_event(scope, this, c"readystatechange");
        progress(scope, this, "abort", 0.0, 0.0, false);
        progress(scope, this, "loadend", 0.0, 0.0, false);
        state = int_prop(scope, this, "_readyState");
    }
    if state == 4 {
        bump_generation(scope, this);
        set_ready_state(scope, this, 0);
        reset_response(scope, this);
    }
    Ok(Value::undefined())
}

fn override_mime_type(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    check(scope, this)?;
    if let Some(mime) = args.first() {
        set(scope, this, "_mimeOverride", mime.clone());
    }
    Ok(Value::undefined())
}

fn user_defined(mime: Option<&[u8]>) -> bool {
    mime.is_some_and(|m| {
        m.to_ascii_lowercase()
            .windows(14)
            .any(|w| w == b"x-user-defined")
    })
}

fn response_text(
    scope: &mut Scope<'_>,
    obj: &Value,
    body: &[u8],
    content_type: Option<&[u8]>,
) -> Value {
    let override_value = prop(scope, obj, "_mimeOverride");
    let mime = c_bytes(scope, &override_value);
    if !user_defined(mime.as_deref()) && !user_defined(content_type) {
        return scope.string_from_bytes(body);
    }
    let text: String = body.iter().map(|&b| char::from(b)).collect();
    scope.string(&text)
}

fn open(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    check(scope, this)?;
    if args.len() < 2 {
        return Err(scope.type_error("XMLHttpRequest.open requires at least 2 arguments"));
    }
    let method = scope.to_bytes(&args[0])?;
    let url = scope.to_bytes(&args[1])?;
    let url = crate::until_nul(url);
    let asynchronous = args.len() < 3 || scope.to_bool(&args[2]);
    if let Some(user) = args.get(3).filter(|u| !is_nullish(u)) {
        scope.to_bytes(user)?;
    }
    if let Some(password) = args.get(4).filter(|p| !is_nullish(p)) {
        scope.to_bytes(password)?;
    }
    if !headers::byte_string_fits(&method) {
        return Err(scope.type_error("XMLHttpRequest.open method is not a ByteString"));
    }
    if !headers::is_token(&method) {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "XMLHttpRequest.open: invalid method",
        ));
    }
    if headers::is_forbidden_method(&method) {
        return Err(ffi::throw_dom(
            scope,
            c"SecurityError",
            18,
            "XMLHttpRequest.open: forbidden method",
        ));
    }
    if !Js::of(scope).url_parses(&url) {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "XMLHttpRequest.open: invalid URL",
        ));
    }
    let method = crate::until_nul(method);
    let normalized = headers::standard_method(&method).map_or(method, <[u8]>::to_vec);
    set_str(scope, this, "_method", &normalized);
    set_str(scope, this, "_url", &url);
    set(scope, this, "_sync", Value::boolean(!asynchronous));
    let list = scope.new_array();
    set(scope, this, "_headers", list);
    set(scope, this, "_aborted", Value::boolean(false));
    bump_generation(scope, this);
    reset_response(scope, this);
    set(scope, this, "_sendFlag", Value::boolean(false));
    set_ready_state(scope, this, 1);
    ffi::fire_event(scope, this, c"readystatechange");
    Ok(Value::undefined())
}

fn set_request_header(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    check(scope, this)?;
    if args.len() < 2 {
        return Err(scope.type_error("setRequestHeader requires at least 2 arguments"));
    }
    let ready_state = int_prop(scope, this, "_readyState");
    if ready_state != 1 || bool_prop(scope, this, "_sendFlag") {
        return Err(ffi::throw_dom(
            scope,
            c"InvalidStateError",
            11,
            "XMLHttpRequest is not open",
        ));
    }
    let name = scope.to_bytes(&args[0])?;
    let value = scope.to_bytes(&args[1])?;
    if !headers::byte_string_fits(&name) || !headers::byte_string_fits(&value) {
        return Err(scope.type_error("setRequestHeader arguments must be ByteStrings"));
    }
    if !headers::is_token(&name) || !headers::value_is_safe(&value) {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "Invalid HTTP header",
        ));
    }
    if !headers::is_forbidden(&name) {
        let mut line = name;
        line.extend_from_slice(b": ");
        line.extend_from_slice(value.trim_ascii());
        let mut list = prop(scope, this, "_headers");
        if !list.is_array() {
            list = scope.new_array();
            set(scope, this, "_headers", list.clone());
        }
        let len = crate::array_length(scope, &list);
        let line = scope.string_from_bytes(&line);
        let _ = scope.set_index(&list, len, line);
    }
    Ok(Value::undefined())
}

fn blob_type(scope: &mut Scope<'_>, body: &Value) -> Option<Vec<u8>> {
    if !body.is_object() {
        return None;
    }
    let kind = scope.get(body, "type").ok()?;
    if !kind.is_string() {
        return None;
    }
    c_bytes(scope, &kind).filter(|k| !k.is_empty() && headers::value_is_safe(k))
}

fn pump_until_done(scope: &mut Scope<'_>, js: Js, this: &Value) {
    let start = ffi::monotonic_us();
    let deadline = start + SYNC_DEADLINE_US;
    while ffi::monotonic_us() < deadline {
        if int_prop(scope, this, "readyState") >= 4 {
            break;
        }
        if !js.pump_iteration() {
            break;
        }
    }
    js.credit_pumped_time(start);
}

fn send(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    check(scope, this)?;
    let ready_state = int_prop(scope, this, "_readyState");
    if ready_state != 1 || bool_prop(scope, this, "_sendFlag") {
        return Err(ffi::throw_dom(
            scope,
            c"InvalidStateError",
            11,
            "XMLHttpRequest is not open",
        ));
    }
    let url = prop(scope, this, "_url");
    let Some(url) = c_bytes(scope, &url) else {
        return Ok(Value::undefined());
    };
    let method = prop(scope, this, "_method");
    let method = c_bytes(scope, &method);
    let send_body = method
        .as_deref()
        .is_some_and(|m| !m.eq_ignore_ascii_case(b"GET") && !m.eq_ignore_ascii_case(b"HEAD"));
    let mut body: Option<Vec<u8>> = None;
    let mut auto_content_type: Option<Vec<u8>> = None;
    let mut body_is_text = false;
    if let Some(value) = args.first().filter(|v| send_body && !is_nullish(v)) {
        if let Some(serialized) = ffi::serialize_form_body(scope, value) {
            body = Some(serialized.body);
            auto_content_type = serialized.content_type;
        } else if value.is_string() {
            if let Ok(text) = scope.to_bytes(value) {
                body = Some(text);
                auto_content_type = Some(b"text/plain;charset=UTF-8".to_vec());
                body_is_text = true;
            }
        } else {
            body = ffi::body_bytes(scope, value);
            auto_content_type = blob_type(scope, value);
        }
    }
    let js = Js::of(scope);
    let page = js.page_url();
    let resolved = page
        .as_deref()
        .and_then(|page| ffi::url_resolve(Some(page), &url))
        .unwrap_or(url);
    let state = XhrState {
        realm: Realm::of(scope),
        timeline: js.main_context(),
        obj: this.clone(),
        url: resolved.clone(),
        origin_url: page.clone(),
        start_ms: js.perf_now_ms(),
        generation: generation(scope, this),
    };
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let list = prop(scope, this, "_headers");
    if list.is_array() {
        let len = crate::array_length(scope, &list);
        for i in 0..len {
            let entry = scope
                .get_index(&list, i)
                .unwrap_or_else(|_| Value::undefined());
            if let Some(line) = c_bytes(scope, &entry) {
                lines.push(line);
            }
        }
    }
    lines.push(b"X-Requested-With: XMLHttpRequest".to_vec());
    let id = crate::next_id();
    crate::with_page(js, |page| page.xhrs.insert(id, state));
    let mut user_content_type = false;
    for line in &mut lines {
        if crate::ascii_starts_with(line, b"Content-Type:") {
            user_content_type = true;
            if body_is_text && let Some(utf8) = headers::content_type_utf8(&line[13..]) {
                let mut rewritten = b"Content-Type: ".to_vec();
                rewritten.extend_from_slice(&utf8);
                *line = rewritten;
            }
        }
    }
    let effective_content_type = if send_body && body.is_some() && !user_content_type {
        auto_content_type
    } else {
        None
    };
    let blocked_mixed = page
        .as_deref()
        .is_some_and(|p| crate::ascii_starts_with(p, b"https://"))
        && crate::ascii_starts_with(&resolved, b"http://");
    let blocked_csp = !js.csp_allows_connect(&resolved, page.as_deref());
    set(scope, this, "_sendFlag", Value::boolean(true));
    progress(scope, this, "loadstart", 0.0, 0.0, false);
    if let Some(bytes) = body.as_ref().filter(|_| send_body) {
        let upload = prop(scope, this, "upload");
        let len = bytes.len() as f64;
        for (i, kind) in ["loadstart", "progress", "load", "loadend"]
            .into_iter()
            .enumerate()
        {
            progress(
                scope,
                &upload,
                kind,
                if i == 0 { 0.0 } else { len },
                len,
                true,
            );
        }
    }
    if blocked_mixed || blocked_csp {
        ffi::schedule_xhr_blocked(js, Ticket { js, id });
    } else {
        let request = Request {
            url: cstring(&resolved),
            top: page.as_deref().map(cstring),
            method: cstring(method.as_deref().unwrap_or(b"GET")),
            has_body: body.is_some(),
            body: body.unwrap_or_default(),
            content_type: effective_content_type.as_deref().map(cstring),
            headers: lines.iter().map(|h| cstring(h)).collect::<Vec<CString>>(),
        };
        let ticket = Box::into_raw(Box::new(Ticket { js, id }));
        request.send(None, ffi::on_xhr_done, ticket.cast());
    }
    if bool_prop(scope, this, "_sync") {
        pump_until_done(scope, js, this);
    }
    Ok(Value::undefined())
}

pub(crate) fn construct(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let obj = ffi::host_construct(scope, this, HO_XHR)?;
    set_ready_state(scope, &obj, 0);
    set(scope, &obj, "_gen", Value::int(0));
    set(scope, &obj, "status", Value::int(0));
    for key in ["statusText", "responseText", "response"] {
        set_str(scope, &obj, key, b"");
    }
    set(scope, &obj, "responseXML", Value::null());
    for key in ["responseType", "responseURL", "_responseHeaders"] {
        set_str(scope, &obj, key, b"");
    }
    set(scope, &obj, "timeout", Value::int(0));
    set(scope, &obj, "withCredentials", Value::boolean(false));
    set(scope, &obj, "_sendFlag", Value::boolean(false));
    for key in XHR_HANDLERS {
        set(scope, &obj, key, Value::null());
    }
    let upload = ffi::host_new(scope, HO_XHR_UPLOAD)?;
    for key in UPLOAD_HANDLERS {
        set(scope, &upload, key, Value::null());
    }
    set(scope, &obj, "upload", upload);
    Ok(obj)
}

fn state_slot(scope: &mut Scope<'_>, this: &Value, key: &str) -> JsResult<Value> {
    match ffi::host_state(scope, this, HO_XHR) {
        Some(state) => Ok(prop(scope, &state, key)),
        None => Err(illegal(scope)),
    }
}

fn get_ready_state(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let state = state_slot(scope, this, "_readyState")?;
    Ok(if state.is_undefined() {
        Value::int(0)
    } else {
        state
    })
}

fn text_response_type(scope: &mut Scope<'_>, obj: &Value) -> bool {
    let kind = prop(scope, obj, "responseType");
    let kind = if kind.is_string() {
        c_bytes(scope, &kind)
    } else {
        None
    };
    kind.is_none_or(|k| k.is_empty() || k == b"text")
}

fn get_response_text(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_XHR) {
        return Err(illegal(scope));
    }
    if !text_response_type(scope, this) {
        return Err(ffi::throw_dom(
            scope,
            c"InvalidStateError",
            11,
            "Failed to read the 'responseText' property from 'XMLHttpRequest': The value is only accessible if the object's 'responseType' is '' or 'text'.",
        ));
    }
    let text = state_slot(scope, this, "responseText")?;
    Ok(if text.is_undefined() {
        scope.string("")
    } else {
        text
    })
}

fn get_response_type(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let kind = state_slot(scope, this, "responseType")?;
    Ok(if kind.is_undefined() {
        scope.string("")
    } else {
        kind
    })
}

fn set_response_type(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(state) = ffi::host_state(scope, this, HO_XHR) else {
        return Err(illegal(scope));
    };
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    let text = scope.to_string_value(&value)?;
    let kind = c_bytes(scope, &text);
    let valid = kind.as_deref().is_some_and(|k| {
        matches!(k, b"" | b"arraybuffer" | b"blob" | b"json" | b"text")
            || (k == b"document" && !Js::of(scope).is_worker())
    });
    if !valid {
        return Ok(Value::undefined());
    }
    let ready_state = int_prop(scope, &state, "_readyState");
    if ready_state == 3 || ready_state == 4 {
        return Err(ffi::throw_dom(
            scope,
            c"InvalidStateError",
            11,
            "Failed to set the 'responseType' property on 'XMLHttpRequest': The response type cannot be set if the object's state is LOADING or DONE.",
        ));
    }
    set(scope, &state, "responseType", text);
    Ok(Value::undefined())
}

fn is_live(js: Js, id: u32) -> bool {
    crate::existing_page(js, |page| page.xhrs.contains_key(&id)).unwrap_or(false)
}

pub(crate) fn done(ticket: Ticket, resp: Response, failed: bool) {
    let Ticket { js, id } = ticket;
    if !is_live(js, id) {
        return;
    }
    let delivery = Delivery {
        js,
        id,
        resp,
        failed,
    };
    if js.in_pump() {
        ffi::schedule_xhr_delivery(delivery);
        return;
    }
    deliver(delivery);
}

pub(crate) fn deliver_idle(delivery: Delivery) {
    if is_live(delivery.js, delivery.id) && delivery.js.in_pump() {
        ffi::schedule_xhr_delivery(delivery);
        return;
    }
    deliver(delivery);
}

fn serialize_known(resp: &Response, raw: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, value) in resp.known_headers() {
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            continue;
        };
        if headers::raw_headers_have(raw, name) {
            continue;
        }
        out.extend_from_slice(name);
        out.extend_from_slice(b": ");
        out.extend_from_slice(&value);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn make_blob(scope: &mut Scope<'_>, body: &[u8], content_type: Option<&[u8]>) -> Value {
    let ctor = crate::global_ctor(scope, "Blob");
    if !scope.is_constructor(&ctor) {
        return Value::null();
    }
    let Ok(buffer) = scope.new_array_buffer(body) else {
        return Value::null();
    };
    let parts = scope.new_array();
    let _ = scope.set_index(&parts, 0, buffer);
    let options = scope.new_object();
    if let Some(kind) = content_type.filter(|k| !k.is_empty()) {
        set_str(scope, &options, "type", kind);
    }
    scope
        .construct(&ctor, &[parts, options])
        .unwrap_or_else(|_| Value::null())
}

fn make_document(scope: &mut Scope<'_>, body: &[u8], content_type: Option<&[u8]>) -> Value {
    let ctor = crate::global_ctor(scope, "DOMParser");
    if !scope.is_constructor(&ctor) {
        return Value::null();
    }
    let Ok(parser) = scope.construct(&ctor, &[]) else {
        return Value::null();
    };
    if !parser.is_object() {
        return Value::null();
    }
    let parse = prop(scope, &parser, "parseFromString");
    if !scope.is_function(&parse) {
        return Value::null();
    }
    let xml = content_type
        .is_some_and(|k| k.windows(3).any(|w| w == b"xml") || k.windows(3).any(|w| w == b"svg"));
    let source = scope.string_from_bytes(body);
    let mime = scope.string(if xml { "application/xml" } else { "text/html" });
    scope
        .call(&parse, &parser, &[source, mime])
        .unwrap_or_else(|_| Value::null())
}

fn store_response(scope: &mut Scope<'_>, js: Js, state: &XhrState, resp: &Response) -> bool {
    let obj = &state.obj;
    let final_url = resp.final_url();
    let allow = headers::cors_allows(
        state.origin_url.as_deref(),
        final_url.as_deref(),
        resp.cors_allow_origin().as_deref(),
    ) && !crate::fetch::final_url_connect_blocked(js, final_url.as_deref());
    let code = if allow { resp.status() as i32 } else { 0 };
    set(scope, obj, "status", Value::int(code));
    set_str(
        scope,
        obj,
        "statusText",
        headers::status_text(code).as_bytes(),
    );
    let url = if allow {
        final_url.clone().unwrap_or_default()
    } else {
        Vec::new()
    };
    set_str(scope, obj, "responseURL", &url);
    let same_origin = ffi::url_same_origin(state.origin_url.as_deref(), final_url.as_deref());
    let raw = resp.raw_headers();
    let all = if !allow {
        Vec::new()
    } else if let (true, Some(raw)) = (same_origin, raw.as_deref()) {
        let mut all = raw.to_vec();
        all.extend_from_slice(&serialize_known(resp, Some(raw)));
        all
    } else {
        serialize_known(resp, None)
    };
    set_str(scope, obj, "_responseHeaders", &all);
    let body: &[u8] = if allow { resp.body() } else { &[] };
    let content_type = resp.content_type();
    let text = response_text(scope, obj, body, content_type.as_deref());
    set(scope, obj, "responseText", text);
    let kind = prop(scope, obj, "responseType");
    let kind = c_bytes(scope, &kind);
    let response = match kind.as_deref() {
        Some(b"json") if body.is_empty() => Value::null(),
        Some(b"json") => scope
            .parse_json(body, "<XHR response>")
            .unwrap_or_else(|_| Value::null()),
        Some(b"arraybuffer") => scope
            .new_array_buffer(body)
            .unwrap_or_else(|_| Value::null()),
        Some(b"blob") => make_blob(scope, body, content_type.as_deref()),
        Some(b"document") => make_document(scope, body, content_type.as_deref()),
        _ => response_text(scope, obj, body, content_type.as_deref()),
    };
    set(scope, obj, "response", response);
    allow
}

fn finish_events(scope: &mut Scope<'_>, obj: &Value, network_error: bool, length: usize) {
    if bool_prop(scope, obj, "_aborted") {
        return;
    }
    let len = length as f64;
    if !network_error {
        set_ready_state(scope, obj, 2);
        ffi::fire_event(scope, obj, c"readystatechange");
        set_ready_state(scope, obj, 3);
        ffi::fire_event(scope, obj, c"readystatechange");
        progress(scope, obj, "progress", len, len, true);
    }
    set_ready_state(scope, obj, 4);
    set(scope, obj, "_sendFlag", Value::boolean(false));
    ffi::fire_event(scope, obj, c"readystatechange");
    if network_error {
        progress(scope, obj, "error", 0.0, 0.0, false);
    } else {
        progress(scope, obj, "load", len, len, true);
    }
    let shown = if network_error { 0.0 } else { len };
    progress(scope, obj, "loadend", shown, shown, !network_error);
}

fn deliver(delivery: Delivery) {
    let Delivery {
        js,
        id,
        resp,
        failed,
    } = delivery;
    let Some(state) = crate::existing_page(js, |page| page.xhrs.remove(&id)).flatten() else {
        return;
    };
    js.with_budget(|| {
        state.realm.enter(|scope| {
            if generation(scope, &state.obj) != state.generation {
                return;
            }
            let end_us = ffi::monotonic_us();
            let start_us = if state.start_ms > 0.0 {
                js.time_origin_us() + (state.start_ms * 1000.0) as i64
            } else {
                end_us
            };
            js.add_resource_timing(
                &ffi::Timing {
                    timeline: state.timeline,
                    cors_mode: true,
                    url: &state.url,
                    initiator: c"xmlhttprequest",
                    start_us,
                    end_us,
                },
                &resp,
            );
            let mut allowed = false;
            if !resp.is_null() && !failed {
                allowed = store_response(scope, js, &state, &resp);
            } else {
                set(scope, &state.obj, "status", Value::int(0));
                set_ready_state(scope, &state.obj, 4);
            }
            let network_error = resp.is_null() || failed || resp.error().is_some() || !allowed;
            let length = if network_error { 0 } else { resp.body().len() };
            finish_events(scope, &state.obj, network_error, length);
        });
        js.drain_mutations();
    });
}

pub(crate) fn blocked(ticket: Ticket) {
    let Ticket { js, id } = ticket;
    if !is_live(js, id) {
        return;
    }
    if js.in_pump() {
        ffi::schedule_xhr_blocked_retry(Ticket { js, id });
        return;
    }
    let Some(state) = crate::existing_page(js, |page| page.xhrs.remove(&id)).flatten() else {
        return;
    };
    js.with_budget(|| {
        state.realm.enter(|scope| {
            let obj = &state.obj;
            if generation(scope, obj) != state.generation {
                return;
            }
            set(scope, obj, "status", Value::int(0));
            set_ready_state(scope, obj, 4);
            set(scope, obj, "_sendFlag", Value::boolean(false));
            if !bool_prop(scope, obj, "_aborted") {
                ffi::fire_event(scope, obj, c"readystatechange");
                progress(scope, obj, "error", 0.0, 0.0, false);
                progress(scope, obj, "loadend", 0.0, 0.0, false);
            }
        });
        js.drain_mutations();
    });
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let proto = crate::proto_of(scope, "XMLHttpRequest");
    if !proto.is_object() {
        return;
    }
    let methods: [(&str, u32, southstar_js_engine::NativeFn); 7] = [
        ("open", 2, open),
        ("send", 0, send),
        ("setRequestHeader", 2, set_request_header),
        ("getResponseHeader", 1, get_response_header),
        ("getAllResponseHeaders", 0, get_all_response_headers),
        ("abort", 0, abort),
        ("overrideMimeType", 1, override_mime_type),
    ];
    for (name, arity, f) in methods {
        let function = scope.function(name, arity, f);
        set(scope, &proto, name, function);
    }
    let getter = scope.function("get readyState", 0, get_ready_state);
    let _ = scope.define_accessor(
        &proto,
        "readyState",
        Some(&getter),
        None,
        Attributes::CONFIGURABLE,
    );
    let getter = scope.function("get responseText", 0, get_response_text);
    let _ = scope.define_accessor(
        &proto,
        "responseText",
        Some(&getter),
        None,
        Attributes::CONFIGURABLE,
    );
    let getter = scope.function("get responseType", 0, get_response_type);
    let setter = scope.function("set responseType", 1, set_response_type);
    let _ = scope.define_accessor(
        &proto,
        "responseType",
        Some(&getter),
        Some(&setter),
        Attributes::CONFIGURABLE,
    );
    let ctor = prop(scope, global, "XMLHttpRequest");
    for (i, name) in ["UNSENT", "OPENED", "HEADERS_RECEIVED", "LOADING", "DONE"]
        .into_iter()
        .enumerate()
    {
        let _ = scope.define(&ctor, name, Value::int(i as i32), Attributes::ENUMERABLE);
        let _ = scope.define(&proto, name, Value::int(i as i32), Attributes::ENUMERABLE);
    }
}
