//! Southstar — the Media Source Extensions natives behind the MediaSource/SourceBuffer polyfill: append, end of stream, buffered ranges, remove and byte counts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi, hooks};

fn stream_id(scope: &mut Scope<'_>, value: &Value) -> u32 {
    scope.to_int32(value).map_or(0, |id| id as u32)
}

fn track_kind(scope: &mut Scope<'_>, value: &Value) -> u8 {
    match scope.to_string(value) {
        Ok(kind) if kind.starts_with('a') => b'a',
        _ => b'v',
    }
}

fn stream(scope: &mut Scope<'_>, args: &[Value]) -> (u32, u8) {
    let id = stream_id(scope, &args[0]);
    let kind = track_kind(scope, &args[1]);
    (id, kind)
}

pub(crate) fn append(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let hooks = hooks::hooks(ffi::js_of(scope));
    let Some(hook) = hooks.mse.filter(|_| args.len() >= 3) else {
        return Ok(Value::boolean(false));
    };
    let (id, kind) = stream(scope, args);
    let appended = ffi::with_bytes(scope, &args[2], |bytes| {
        id != 0 && ffi::call_mse(hook, id, kind, Some(bytes))
    });
    Ok(Value::boolean(appended == Some(true)))
}

pub(crate) fn end_of_stream(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let hooks = hooks::hooks(ffi::js_of(scope));
    let (Some(hook), Some(id)) = (hooks.mse, args.first()) else {
        return Ok(Value::boolean(false));
    };
    let id = stream_id(scope, id);
    if id == 0 {
        return Ok(Value::boolean(false));
    }
    ffi::call_mse(hook, id, b'v', None);
    Ok(Value::boolean(true))
}

fn buffered_range(scope: &mut Scope<'_>, args: &[Value]) -> (f64, f64) {
    let hooks = hooks::hooks(ffi::js_of(scope));
    let Some(hook) = hooks.mse_buffered.filter(|_| args.len() >= 2) else {
        return (0.0, 0.0);
    };
    let (id, kind) = stream(scope, args);
    if id == 0 {
        return (0.0, 0.0);
    }
    ffi::call_mse_buffered(hook, id, kind)
}

pub(crate) fn buffered(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    Ok(Value::number(buffered_range(scope, args).1))
}

pub(crate) fn buffered_start(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    Ok(Value::number(buffered_range(scope, args).0))
}

pub(crate) fn remove(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let hooks = hooks::hooks(ffi::js_of(scope));
    let Some(hook) = hooks.mse_remove.filter(|_| args.len() >= 4) else {
        return Ok(Value::boolean(false));
    };
    let (id, kind) = stream(scope, args);
    if id == 0 {
        return Ok(Value::boolean(false));
    }
    let (Ok(start), Ok(end)) = (scope.to_number(&args[2]), scope.to_number(&args[3])) else {
        return Ok(Value::boolean(false));
    };
    Ok(Value::boolean(ffi::call_mse_remove(
        hook, id, kind, start, end,
    )))
}

pub(crate) fn bytes(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let hooks = hooks::hooks(ffi::js_of(scope));
    let Some(hook) = hooks.mse_bytes.filter(|_| args.len() >= 2) else {
        return Ok(Value::number(-1.0));
    };
    let (id, kind) = stream(scope, args);
    if id == 0 {
        return Ok(Value::number(-1.0));
    }
    Ok(Value::number(ffi::call_mse_bytes(hook, id, kind) as f64))
}
