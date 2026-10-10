//! Southstar — crypto.getRandomValues and crypto.randomUUID over the platform CSPRNG.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ElementType, ObjectKind, Scope, Value};

use crate::{JsResult, dom_exception, error_with_code, get};

const MAX_BYTES: usize = 65536;

fn type_mismatch(scope: &mut Scope<'_>) -> Value {
    dom_exception(
        scope,
        "TypeMismatchError",
        17,
        "getRandomValues: integer typed array required",
    )
}

fn quota_exceeded(scope: &mut Scope<'_>, message: &str) -> Value {
    let global = scope.global();
    let ctor = get(scope, &global, "QuotaExceededError");
    if scope.is_function(&ctor) {
        let text = scope.string(message);
        if let Ok(error) = scope.construct(&ctor, &[text]) {
            return error;
        }
    }
    let error = error_with_code(scope, "QuotaExceededError", message, 22);
    let _ = scope.define(&error, "requested", Value::null(), crate::HIDDEN_WRITABLE);
    let _ = scope.define(&error, "quota", Value::null(), crate::HIDDEN_WRITABLE);
    error
}

pub(crate) fn get_random_values(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(array) = args.first() else {
        return Err(scope.type_error("getRandomValues: argument required"));
    };
    let element = match scope.object_kind(array) {
        Some(ObjectKind::TypedArray(element)) => element,
        Some(ObjectKind::DataView) => return Err(type_mismatch(scope)),
        _ => {
            return Err(scope.type_error("getRandomValues: integer typed array required"));
        }
    };
    if matches!(
        element,
        ElementType::Float16 | ElementType::Float32 | ElementType::Float64
    ) {
        return Err(type_mismatch(scope));
    }
    let byte_length = scope
        .typed_array_view(array)?
        .map_or(0, |view| view.length * element.size());
    if byte_length > MAX_BYTES {
        return Err(quota_exceeded(
            scope,
            "getRandomValues: the array is longer than 65536 bytes",
        ));
    }
    if byte_length == 0 {
        return Ok(array.clone());
    }
    let filled = scope.with_buffer_bytes_mut(array, |bytes| {
        bytes
            .get_mut(..byte_length)
            .is_some_and(crate::ffi::csprng_fill)
    });
    match filled {
        Some(true) => Ok(array.clone()),
        Some(false) => Err(scope.type_error("CSPRNG unavailable")),
        None => Err(scope.type_error("getRandomValues: backing buffer unavailable")),
    }
}

pub(crate) fn random_uuid(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let mut bytes = [0u8; 16];
    if !crate::ffi::csprng_fill(&mut bytes) {
        return Err(scope.type_error("CSPRNG unavailable"));
    }
    let word = |i: usize| u32::from_ne_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let r0 = word(0);
    let r1 = (word(4) & 0xffff_0fff) | 0x0000_4000;
    let r2 = (word(8) & 0x3fff_ffff) | 0x8000_0000;
    let r3 = word(12);
    let uuid = format!(
        "{r0:08x}-{:04x}-{:04x}-{:04x}-{:04x}{r3:08x}",
        r1 >> 16,
        r1 & 0xffff,
        r2 >> 16,
        r2 & 0xffff,
    );
    Ok(scope.string(&uuid))
}
