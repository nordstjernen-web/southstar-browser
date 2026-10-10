//! Southstar — navigator.sendBeacon: a fire-and-forget POST of a small payload to an http(s) URL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::JsResult;
use crate::ffi::{self, Js};

const TEXT_PLAIN: &[u8] = b"text/plain;charset=UTF-8";

struct Payload {
    body: Vec<u8>,
    content_type: Option<Vec<u8>>,
}

fn blob_type(scope: &mut Scope<'_>, data: &Value) -> Option<Vec<u8>> {
    if !data.is_object() {
        return None;
    }
    let kind = scope.get(data, "type").ok()?;
    if !kind.is_string() {
        return None;
    }
    let kind = scope.to_bytes(&kind).ok()?;
    (!kind.is_empty() && crate::headers::value_is_safe(&kind)).then_some(kind)
}

fn payload(scope: &mut Scope<'_>, data: &Value) -> Option<Payload> {
    if let Some(form) = ffi::serialize_form_body(scope, data) {
        return Some(Payload {
            body: form.body,
            content_type: form.content_type,
        });
    }
    if data.is_string() {
        let body = scope.to_bytes(data).ok()?;
        return Some(Payload {
            body,
            content_type: Some(TEXT_PLAIN.to_vec()),
        });
    }
    let body = ffi::body_bytes(scope, data)?;
    let content_type = blob_type(scope, data);
    Some(Payload { body, content_type })
}

pub(crate) fn send_beacon(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let Some(url) = args.first().filter(|url| url.is_string()) else {
        return Ok(Value::boolean(false));
    };
    let Ok(raw_url) = scope.to_bytes(url) else {
        return Ok(Value::boolean(false));
    };
    let page = Js::of(scope).page_url();
    let resolved = match &page {
        Some(page) => ffi::url_resolve(Some(page), &raw_url),
        None => Some(raw_url),
    };
    let Some(url) = resolved else {
        return Err(scope.type_error("sendBeacon: invalid URL"));
    };
    if !ffi::url_is_http_or_https(&url) {
        return Err(scope.type_error("sendBeacon: URL scheme must be http or https"));
    }
    let payload = args
        .get(1)
        .filter(|data| !data.is_undefined() && !data.is_null())
        .and_then(|data| payload(scope, data));
    let (has_body, body, content_type) = match payload {
        Some(payload) => (true, payload.body, payload.content_type),
        None => (false, Vec::new(), None),
    };
    let request = ffi::Request {
        url: ffi::cstring(&url),
        top: page.as_deref().map(ffi::cstring),
        method: ffi::cstring(b"POST"),
        body,
        has_body,
        content_type: content_type.as_deref().map(ffi::cstring),
        headers: Vec::new(),
    };
    request.send(None, ffi::on_beacon_done, core::ptr::null_mut());
    Ok(Value::boolean(true))
}
