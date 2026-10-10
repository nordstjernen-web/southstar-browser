//! Southstar — the bytes behind a Blob and the blob: URLs URL.createObjectURL hands out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{JsResult, arg, get, is_nullish};

fn text_bytes(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    let mut bytes = scope.to_bytes(value).unwrap_or_default();
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    bytes
}

fn array_like_bytes(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    let length = get(scope, value, "length");
    let length = scope
        .to_number(&length)
        .ok()
        .map_or(0, |n| if n.is_finite() { n as i64 as u32 } else { 0 });
    (0..length)
        .map(|i| {
            let item = scope
                .get_index(value, i)
                .unwrap_or_else(|_| Value::undefined());
            scope.to_int32(&item).unwrap_or(0) as u8
        })
        .collect()
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8> {
    let stored = get(scope, blob, "__ndBlobBytes");
    if is_nullish(&stored) {
        return text_bytes(scope, blob);
    }
    if scope.typed_array_element(&stored).is_some() {
        return scope
            .with_typed_array(&stored, |view| view.bytes.to_vec())
            .unwrap_or_default();
    }
    array_like_bytes(scope, &stored)
}

fn blob_type(scope: &mut Scope<'_>, blob: &Value) -> Option<Vec<u8>> {
    let kind = get(scope, blob, "type");
    if !kind.is_string() {
        return None;
    }
    scope.to_bytes(&kind).ok().filter(|k| !k.is_empty())
}

fn register(scope: &mut Scope<'_>, js: Js, url: &[u8], object: &Value) {
    let bytes = blob_bytes(scope, object);
    let kind = blob_type(scope, object);
    ffi::blob_urls_put(js, url, &bytes, kind.as_deref());
}

pub(crate) fn create_object_url(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = Js::of(scope);
    let origin = ffi::page_origin(js).filter(|o| !o.is_empty());
    let mut url = b"blob:".to_vec();
    url.extend_from_slice(origin.as_deref().unwrap_or(b"null"));
    url.push(b'/');
    url.extend_from_slice(&ffi::random_uuid());
    let object = arg(args, 0);
    if !js.is_null() && object.is_object() {
        register(scope, js, &url, &object);
    }
    Ok(scope.string_from_bytes(&url))
}

pub(crate) fn update_object_url(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = Js::of(scope);
    let (url, object) = (arg(args, 0), arg(args, 1));
    if js.is_null() || args.len() < 2 || !url.is_string() || !object.is_object() {
        return Ok(Value::boolean(false));
    }
    let Ok(url) = scope.to_bytes(&url) else {
        return Ok(Value::boolean(false));
    };
    if !url.starts_with(b"blob:") {
        return Ok(Value::boolean(false));
    }
    register(scope, js, &url, &object);
    ffi::mark_mutated(js);
    Ok(Value::boolean(true))
}

pub(crate) fn revoke_object_url(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = Js::of(scope);
    if let (false, Some(url)) = (js.is_null(), args.first())
        && let Ok(url) = scope.to_bytes(url)
    {
        ffi::blob_urls_remove(js, &url);
    }
    Ok(Value::undefined())
}
