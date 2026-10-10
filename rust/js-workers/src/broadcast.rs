//! Southstar — BroadcastChannel: the realm's open channels and the messages posted to the others of the same name.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, bind, ffi, flag, get, get_index, length, push, set};

const REGISTRY: &str = "__ns_broadcast_channels";

fn brand(scope: &mut Scope<'_>, this: &Value) -> Result<(), Value> {
    if ffi::is_broadcast_channel(this) {
        Ok(())
    } else {
        Err(scope.type_error("Illegal invocation"))
    }
}

fn registry(scope: &mut Scope<'_>) -> Value {
    let global = scope.global();
    let registry = get(scope, &global, REGISTRY);
    if registry.is_array() {
        return registry;
    }
    let registry = scope.new_array();
    set(scope, &global, REGISTRY, registry.clone());
    registry
}

pub(crate) fn post_message(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    let Some(message) = args.first() else {
        return Err(scope.type_error(
            "Failed to execute 'postMessage' on 'BroadcastChannel': 1 argument required, but only 0 present.",
        ));
    };
    if flag(scope, this, "_closed") {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            c"BroadcastChannel.postMessage: channel is closed",
        ));
    }
    let global = scope.global();
    let clone = get(scope, &global, "structuredClone");
    let data = if scope.is_function(&clone) {
        scope.call(&clone, &Value::undefined(), core::slice::from_ref(message))?
    } else {
        message.clone()
    };
    let name = get(scope, this, "name");
    let name = crate::text(scope, &name);
    let registry = registry(scope);
    for i in 0..length(scope, &registry) {
        let channel = get_index(scope, &registry, i);
        if channel.same_object(this) {
            continue;
        }
        let closed = flag(scope, &channel, "_closed");
        let channel_name = get(scope, &channel, "name");
        let channel_name = crate::text(scope, &channel_name);
        let same = name.is_some() && name == channel_name;
        if !closed && same {
            ffi::queue_delivery(scope, &[channel, data.clone()]);
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn close(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    brand(scope, this)?;
    set(scope, this, "_closed", Value::boolean(true));
    let registry = registry(scope);
    for i in 0..length(scope, &registry) {
        let channel = get_index(scope, &registry, i);
        if channel.same_object(this) {
            let splice = get(scope, &registry, "splice");
            let _ = scope.call(
                &splice,
                &registry,
                &[Value::number(f64::from(i)), Value::int(1)],
            );
            break;
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn construct(scope: &mut Scope<'_>, new_target: &Value, args: &[Value]) -> JsResult {
    let channel = ffi::construct_broadcast_channel(scope, new_target)?;
    let Some(name) = args.first() else {
        return Err(scope.type_error(
            "Failed to construct 'BroadcastChannel': 1 argument required, but only 0 present.",
        ));
    };
    let name = scope.to_string(name)?;
    let global = scope.global();
    let location = get(scope, &global, "location");
    let origin = if location.is_object() {
        get(scope, &location, "origin")
    } else {
        Value::undefined()
    };
    let origin = if origin.is_string() {
        crate::text(scope, &origin)
    } else {
        None
    };
    crate::set_str(
        scope,
        &channel,
        "_origin",
        origin.as_deref().unwrap_or("null"),
    );
    crate::set_str(scope, &channel, "name", &name);
    set(scope, &channel, "_closed", Value::boolean(false));
    set(scope, &channel, "onmessage", Value::null());
    set(scope, &channel, "onmessageerror", Value::null());
    let registry = registry(scope);
    push(scope, &registry, channel.clone());
    Ok(channel)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let channel = crate::proto_of(scope, global, "BroadcastChannel");
    if channel.is_object() {
        bind(scope, &channel, "close", 0, close);
        bind(scope, &channel, "postMessage", 1, post_message);
    }
}
