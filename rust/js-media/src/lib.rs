//! Southstar — the media bindings: HTMLMediaElement, media type support, the MSE natives, text tracks, the Web Audio API surface over rust/webaudio and the EME entry points.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod audio;
pub mod element;
pub mod eme;
pub(crate) mod ffi;
mod hooks;
mod mse;
pub mod support;
pub mod tracks;

pub use ffi::{
    AudioFn, Element, MseBufferedFn, MseBytesFn, MseFn, MseRemoveFn, SeekFn, ToggleFn, VolumeFn,
    install_runtime_hooks, set_audio_hook, set_mse_buffered_hook, set_mse_bytes_hook, set_mse_hook,
    set_mse_remove_hook, set_muted_hook, set_play_hook, set_seek_hook, set_volume_hook,
    video_event,
};

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

const PROTOTYPE_ONLY_WRITABLE: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope
        .to_number(&length)
        .map(|n| southstar_js_engine::int64_modulo(n) as u32)
        .unwrap_or(0)
}

pub(crate) fn make_ctor(scope: &mut Scope<'_>, name: &str, arity: u32, f: NativeFn) -> Value {
    let function = scope.constructor_or_function(name, arity, f);
    let proto = scope.new_object();
    let _ = scope.define(&proto, "constructor", function.clone(), Attributes::METHOD);
    let _ = scope.define_to_string_tag(&proto, name);
    let _ = scope.define(&function, "prototype", proto, PROTOTYPE_ONLY_WRITABLE);
    function
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    scope.dom_exception_code(name.as_bytes(), 9, message.as_bytes())
}

pub(crate) fn resolved(scope: &mut Scope<'_>, value: Value) -> JsResult {
    let (promise, resolve, _) = scope.new_promise()?;
    let _ = scope.call(&resolve, &Value::undefined(), &[value]);
    Ok(promise)
}

pub(crate) fn rejected(scope: &mut Scope<'_>, reason: Value) -> JsResult {
    scope.rejected_promise(&reason)
}
