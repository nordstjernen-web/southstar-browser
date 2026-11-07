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

pub(crate) use southstar_js_bind::make_constructor;
pub(crate) use southstar_js_engine::props::{bind, get, set, set_str, truthy};
use southstar_js_engine::{Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope
        .to_number(&length)
        .map(|n| southstar_js_engine::int64_modulo(n) as u32)
        .unwrap_or(0)
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
