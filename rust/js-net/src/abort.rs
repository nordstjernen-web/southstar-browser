//! Southstar — AbortController and AbortSignal, with AbortSignal.abort(), any() and timeout().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, HO_ABORT_CONTROLLER, HO_ABORT_SIGNAL, Js, Realm};
use crate::{JsResult, bool_prop, is_nullish, prop, set, set_str};

pub(crate) struct AbortTimeout {
    realm: Realm,
    signal: Value,
}

pub(crate) struct TimeoutTicket {
    js: Js,
    id: u32,
}

fn illegal(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

pub(crate) fn make_signal(scope: &mut Scope<'_>, aborted: bool, reason: &Value) -> JsResult {
    let signal = ffi::host_new(scope, HO_ABORT_SIGNAL)?;
    set(scope, &signal, "aborted", Value::boolean(aborted));
    let reason = if !aborted {
        Value::undefined()
    } else if reason.is_undefined() {
        ffi::abort_error(scope)
    } else {
        reason.clone()
    };
    set(scope, &signal, "reason", reason);
    set(scope, &signal, "onabort", Value::null());
    Ok(signal)
}

pub(crate) fn controller_ctor(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let controller = ffi::host_construct(scope, this, HO_ABORT_CONTROLLER)?;
    let signal = make_signal(scope, false, &Value::undefined())?;
    set(scope, &controller, "signal", signal);
    Ok(controller)
}

fn throw_if_aborted(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_ABORT_SIGNAL) {
        return Err(illegal(scope));
    }
    if !bool_prop(scope, this, "aborted") {
        return Ok(Value::undefined());
    }
    let reason = prop(scope, this, "reason");
    if is_nullish(&reason) {
        return Err(scope.type_error("signal is aborted"));
    }
    Err(reason)
}

fn controller_abort(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_ABORT_CONTROLLER) {
        return Err(illegal(scope));
    }
    let signal = prop(scope, this, "signal");
    if signal.is_object() && !bool_prop(scope, &signal, "aborted") {
        set(scope, &signal, "aborted", Value::boolean(true));
        let reason = match args.first().filter(|r| !r.is_undefined()) {
            Some(reason) => reason.clone(),
            None => ffi::abort_error(scope),
        };
        set(scope, &signal, "reason", reason);
        ffi::fire_event(scope, &signal, c"abort");
    }
    Ok(Value::undefined())
}

fn static_abort(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let reason = args.first().cloned().unwrap_or_else(Value::undefined);
    make_signal(scope, true, &reason)
}

fn propagate(scope: &mut Scope<'_>, combined: &Value, source: &Value) {
    if bool_prop(scope, combined, "aborted") {
        return;
    }
    let mut reason = if source.is_object() {
        prop(scope, source, "reason")
    } else {
        Value::undefined()
    };
    if is_nullish(&reason) {
        reason = ffi::abort_error(scope);
    }
    set(scope, combined, "aborted", Value::boolean(true));
    set(scope, combined, "reason", reason);
    ffi::fire_event(scope, combined, c"abort");
}

fn any_handler(scope: &mut Scope<'_>, _this: &Value, _args: &[Value], data: &[Value]) -> JsResult {
    propagate(scope, &data[0], &data[1]);
    Ok(Value::undefined())
}

fn static_any(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let combined = make_signal(scope, false, &Value::undefined())?;
    let Some(list) = args.first().filter(|l| l.is_object()) else {
        return Ok(combined);
    };
    let len = crate::array_length(scope, list);
    for i in 0..len {
        let signal = scope
            .get_index(list, i)
            .unwrap_or_else(|_| Value::undefined());
        if !signal.is_object() {
            continue;
        }
        if bool_prop(scope, &signal, "aborted") {
            propagate(scope, &combined, &signal);
            continue;
        }
        let handler = scope.bound_function("", 0, any_handler, &[combined.clone(), signal.clone()]);
        let kind = scope.string("abort");
        crate::call_method(scope, &signal, "addEventListener", &[kind, handler]);
    }
    Ok(combined)
}

fn static_timeout(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let ms = args
        .first()
        .map_or(0, |v| scope.to_int64(v).unwrap_or(0))
        .max(0);
    let signal = make_signal(scope, false, &Value::undefined())?;
    let js = Js::of(scope);
    if js.is_null() {
        return Ok(signal);
    }
    let id = crate::next_id();
    let entry = AbortTimeout {
        realm: Realm::of(scope),
        signal: signal.clone(),
    };
    crate::with_page(js, |page| page.aborts.insert(id, entry));
    ffi::schedule_abort_timeout(js, ms.min(u32::MAX as i64) as u32, TimeoutTicket { js, id });
    Ok(signal)
}

pub(crate) fn timeout_fired(ticket: TimeoutTicket) {
    let TimeoutTicket { js, id } = ticket;
    let live = crate::existing_page(js, |page| page.aborts.contains_key(&id)).unwrap_or(false);
    if !live {
        return;
    }
    if js.in_pump() {
        ffi::schedule_abort_timeout(js, 4, TimeoutTicket { js, id });
        return;
    }
    let Some(entry) = crate::existing_page(js, |page| page.aborts.remove(&id)).flatten() else {
        return;
    };
    entry.realm.enter(|scope| {
        if bool_prop(scope, &entry.signal, "aborted") {
            return;
        }
        set(scope, &entry.signal, "aborted", Value::boolean(true));
        let reason = scope.new_error();
        set_str(scope, &reason, "name", b"TimeoutError");
        set_str(scope, &reason, "message", b"signal timed out");
        set(scope, &entry.signal, "reason", reason);
        ffi::fire_event(scope, &entry.signal, c"abort");
    });
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    ffi::bind_illegal_ctor(scope, global, c"AbortSignal");
    let ctor = prop(scope, global, "AbortSignal");
    let abort = scope.function("abort", 0, static_abort);
    set(scope, &ctor, "abort", abort);
    let timeout = scope.function("timeout", 1, static_timeout);
    set(scope, &ctor, "timeout", timeout);
    let any = scope.function("any", 1, static_any);
    set(scope, &ctor, "any", any);
    let proto = prop(scope, &ctor, "prototype");
    let throw = scope.function("throwIfAborted", 0, throw_if_aborted);
    set(scope, &proto, "throwIfAborted", throw);
    let _ = scope.define_to_string_tag(&proto, "AbortSignal");
    let controller = crate::proto_of(scope, "AbortController");
    if controller.is_object() {
        let abort = scope.function("abort", 0, controller_abort);
        set(scope, &controller, "abort", abort);
    }
}
