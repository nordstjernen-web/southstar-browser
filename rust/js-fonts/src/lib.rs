//! Southstar — the CSS Font Loading bindings: FontFace, document.fonts and the promises that settle once pending web fonts have loaded.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod ffi;
mod font_face;
pub mod font_set;
mod ready;

pub use ffi::install_runtime_hooks;

pub(crate) use southstar_js_engine::props::{bind, set, set_str};
use southstar_js_engine::{Scope, Value};

pub use ffi::nsr_window_fontface_ctor;

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) fn c_string(scope: &mut Scope<'_>, value: &Value) -> Value {
    let mut bytes = scope.to_bytes(value).unwrap_or_default();
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    scope.string_from_bytes(&bytes)
}

pub(crate) fn undefined(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn always_true(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(true))
}
