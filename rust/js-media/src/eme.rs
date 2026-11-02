//! Southstar — EME: navigator.requestMediaKeySystemAccess and HTMLMediaElement.setMediaKeys, which report that no key system exists.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, rejected, resolved, set};

pub(crate) fn request_access(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let error = scope.dom_exception_code(b"NotSupportedError", 9, b"No supported key system");
    rejected(scope, error)
}

pub fn set_media_keys(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.first().is_some_and(Value::is_null) {
        set(scope, this, "mediaKeys", Value::null());
        return resolved(scope, Value::undefined());
    }
    let error = scope.dom_exception_code(
        b"NotSupportedError",
        9,
        b"Southstar does not implement EME / DRM",
    );
    rejected(scope, error)
}
