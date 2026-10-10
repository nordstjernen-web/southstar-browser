//! Southstar — MessagePort and MessageChannel: entangled ports, their message queues, and moving a port to another realm in a transfer list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{HIDDEN, JsResult, PLAIN, arg, bind, ffi, flag, get, get_index, length, push, set};

pub(crate) struct Transfer {
    pub old: Value,
    pub new: Value,
}

fn illegal(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

fn brand(scope: &mut Scope<'_>, this: &Value) -> Result<(), Value> {
    if ffi::is_message_port(this) {
        Ok(())
    } else {
        Err(illegal(scope))
    }
}

fn port_message(scope: &mut Scope<'_>, data: &Value, ports: Value) -> Value {
    let message = scope.new_object();
    set(scope, &message, "_portMessage", Value::boolean(true));
    set(scope, &message, "data", data.clone());
    set(scope, &message, "ports", ports);
    message
}

fn freeze(scope: &mut Scope<'_>, array: &Value) {
    let _ = scope.freeze(array);
}

pub(crate) fn deliver(scope: &mut Scope<'_>, port: &Value, data: &Value, ports: Option<&Value>) {
    let shipped_to = get(scope, port, "_shipped_to");
    if shipped_to.is_object() {
        if flag(scope, &shipped_to, "_started") {
            let undefined = Value::undefined();
            deliver(scope, &shipped_to, data, Some(ports.unwrap_or(&undefined)));
            return;
        }
        let queue = get(scope, &shipped_to, "_queue");
        if queue.is_array() {
            let ports = match ports {
                Some(ports) => ports.clone(),
                None => scope.new_array(),
            };
            let message = port_message(scope, data, ports);
            push(scope, &queue, message);
        }
        return;
    }
    if flag(scope, port, "_closed") {
        return;
    }
    ffi::with_receiving_realm(scope, port, |scope, realm, other_realm| {
        let mut data = data.clone();
        if other_realm && data.is_object() {
            let transfer = ports
                .filter(|ports| ports.is_array())
                .cloned()
                .unwrap_or_else(Value::undefined);
            let none = Value::undefined();
            if let Ok(adopted) =
                southstar_js_clone::clone_transfer(realm, &data, &transfer, (&none, &none))
            {
                data = adopted;
            }
        }
        let origin = get(scope, port, "_origin");
        let origin = if origin.is_string() {
            crate::text(scope, &origin)
        } else {
            None
        };
        let event = ffi::event_new(realm);
        crate::set_str(scope, &event, "type", "message");
        set(scope, &event, "data", data);
        crate::set_str(scope, &event, "origin", origin.as_deref().unwrap_or(""));
        crate::set_str(scope, &event, "lastEventId", "");
        set(scope, &event, "source", Value::null());
        let event_ports = realm.new_array();
        if let Some(ports) = ports.filter(|ports| ports.is_array()) {
            for i in 0..length(scope, ports) {
                let item = get_index(scope, ports, i);
                crate::set_index(realm, &event_ports, i, item);
            }
        }
        freeze(realm, &event_ports);
        set(scope, &event, "ports", event_ports);
        set(scope, &event, "target", port.clone());
        set(scope, &event, "currentTarget", port.clone());
        set(scope, &event, "defaultPrevented", Value::boolean(false));
        let _ = scope.define(&event, "isTrusted", Value::boolean(true), PLAIN);
        set(scope, &event, "bubbles", Value::boolean(false));
        set(scope, &event, "cancelable", Value::boolean(false));
        set(scope, &event, "composed", Value::boolean(false));
        set(scope, &event, "_is_trusted", Value::boolean(true));
        ffi::define_cancel_bubble(realm, &event);
        let budget = ffi::Budget::enter(scope);
        ffi::dispatch_with_event(scope, port, "message", &event);
        drop(budget);
    });
}

pub(crate) fn enable(scope: &mut Scope<'_>, port: &Value) {
    let was = flag(scope, port, "_started");
    set(scope, port, "_started", Value::boolean(true));
    if was {
        return;
    }
    let queue = get(scope, port, "_queue");
    if !queue.is_array() {
        return;
    }
    for i in 0..length(scope, &queue) {
        let item = get_index(scope, &queue, i);
        if flag(scope, &item, "_portMessage") {
            let data = get(scope, &item, "data");
            let ports = get(scope, &item, "ports");
            ffi::queue_delivery(scope, &[port.clone(), data, ports]);
        } else {
            ffi::queue_delivery(scope, &[port.clone(), item]);
        }
    }
    let fresh = scope.new_array();
    set(scope, port, "_queue", fresh);
}

pub(crate) fn start(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    brand(scope, this)?;
    enable(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn onmessage_get(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(state) = ffi::port_state(scope, this) else {
        return Err(illegal(scope));
    };
    let handler = get(scope, &state, "onmessage");
    Ok(if handler.is_undefined() {
        Value::null()
    } else {
        handler
    })
}

pub(crate) fn onmessage_set(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(state) = ffi::port_state(scope, this) else {
        return Err(illegal(scope));
    };
    let value = arg(args, 0);
    let handler = if value.is_object() {
        value
    } else {
        Value::null()
    };
    set(scope, &state, "onmessage", handler);
    enable(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn bridge_id(scope: &mut Scope<'_>, port: &Value) -> u64 {
    let id = get(scope, port, "_bridge_id");
    if !id.is_number() {
        return 0;
    }
    match scope.to_number(&id) {
        Ok(d) if d > 0.0 => d as u64,
        _ => 0,
    }
}

pub(crate) fn post_message(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    let Some(data) = args.first() else {
        return Err(scope.type_error(
            "Failed to execute 'postMessage' on 'MessagePort': 1 argument required, but only 0 present.",
        ));
    };
    if flag(scope, this, "_closed") {
        return Ok(Value::undefined());
    }
    let bridge = bridge_id(scope, this);
    if bridge != 0 {
        return crate::worker::bridge_send(scope, this, bridge, data);
    }
    let options = arg(args, 1);
    let transfer = if options.is_array() {
        options
    } else if options.is_object() {
        scope.get(&options, "transfer")?
    } else {
        Value::undefined()
    };
    let pair = get(scope, this, "_pair");
    let prepared = if pair.is_object() {
        ffi::with_port_realm(scope, &pair, |realm| {
            transfer_prepare(realm, &transfer, Some(this))
        })
    } else {
        transfer_prepare(scope, &transfer, Some(this))
    };
    let Some(moved) = prepared else {
        return Err(transfer_error(scope));
    };
    let cloned =
        southstar_js_clone::clone_transfer(scope, data, &transfer, (&moved.old, &moved.new))?;
    transfer_commit(scope, &moved);
    let pair = get(scope, this, "_pair");
    if pair.is_undefined() || pair.is_null() {
        return Ok(Value::undefined());
    }
    if flag(scope, &pair, "_started") {
        ffi::queue_delivery(scope, &[pair, cloned, moved.new]);
    } else {
        let mut queue = get(scope, &pair, "_queue");
        if !queue.is_array() {
            queue = scope.new_array();
            set(scope, &pair, "_queue", queue.clone());
        }
        let message = port_message(scope, &cloned, moved.new);
        push(scope, &queue, message);
    }
    Ok(Value::undefined())
}

fn signal_aborted(scope: &mut Scope<'_>, signal: &Value) -> bool {
    signal.is_object() && flag(scope, signal, "aborted")
}

fn entry_signal_aborted(scope: &mut Scope<'_>, entry: &Value) -> bool {
    if !entry.is_object() {
        return false;
    }
    let signal = get(scope, entry, "signal");
    signal_aborted(scope, &signal)
}

pub(crate) fn add_event_listener(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let callback = arg(args, 1);
    if args.len() < 2 || !callback.is_object() {
        return Ok(Value::undefined());
    }
    let Some(kind) = crate::text(scope, &args[0]) else {
        return Ok(Value::undefined());
    };
    let (once, signal) = match args.get(2) {
        Some(options) => ffi::parse_listener_options(scope, options)?,
        None => (false, Value::null()),
    };
    if signal_aborted(scope, &signal) {
        return Ok(Value::undefined());
    }
    let mut listeners = get(scope, this, "_listeners");
    if !listeners.is_array() {
        listeners = scope.new_array();
        set(scope, this, "_listeners", listeners.clone());
    }
    let len = length(scope, &listeners);
    for i in 0..len {
        let entry = get_index(scope, &listeners, i);
        let entry_type = get(scope, &entry, "type");
        let entry_callback = get(scope, &entry, "cb");
        let duplicate = crate::text(scope, &entry_type).is_some_and(|t| t == kind)
            && entry_callback.same_object(&callback)
            && !entry_signal_aborted(scope, &entry);
        if duplicate {
            return Ok(Value::undefined());
        }
    }
    let entry = scope.new_object();
    crate::set_str(scope, &entry, "type", &kind);
    set(scope, &entry, "cb", callback);
    if once {
        set(scope, &entry, "once", Value::boolean(true));
    }
    if signal.is_object() {
        set(scope, &entry, "signal", signal);
    }
    crate::set_index(scope, &listeners, len, entry);
    Ok(Value::undefined())
}

pub(crate) fn remove_event_listener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let Some(kind) = crate::text(scope, &args[0]) else {
        return Ok(Value::undefined());
    };
    let listeners = get(scope, this, "_listeners");
    if !listeners.is_array() {
        return Ok(Value::undefined());
    }
    for i in 0..length(scope, &listeners) {
        let entry = get_index(scope, &listeners, i);
        let entry_type = get(scope, &entry, "type");
        let entry_callback = get(scope, &entry, "cb");
        let matches = crate::text(scope, &entry_type).is_some_and(|t| t == kind)
            && entry_callback.same_object(&args[1]);
        if matches {
            set(scope, &entry, "_dead", Value::boolean(true));
            ffi::compact_dead_listeners(scope, this);
            break;
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn close(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    brand(scope, this)?;
    set(scope, this, "_closed", Value::boolean(true));
    Ok(Value::undefined())
}

fn realm_marker(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn new_port(scope: &mut Scope<'_>) -> JsResult {
    let port = ffi::new_port_object(scope)?;
    set(scope, &port, "onmessage", Value::null());
    set(scope, &port, "onmessageerror", Value::null());
    let marker = scope.function("", 0, realm_marker);
    let _ = scope.define(&port, "_realm", marker, HIDDEN);
    set(scope, &port, "_is_port", Value::boolean(true));
    set(scope, &port, "_closed", Value::boolean(false));
    set(scope, &port, "_started", Value::boolean(false));
    let queue = scope.new_array();
    set(scope, &port, "_queue", queue);
    Ok(port)
}

fn ship(scope: &mut Scope<'_>, port: &Value, shipped: &Value) {
    let pair = get(scope, port, "_pair");
    if pair.is_object() {
        set(scope, &pair, "_pair", shipped.clone());
        set(scope, shipped, "_pair", pair);
    }
    let origin = get(scope, port, "_origin");
    if origin.is_string() {
        set(scope, shipped, "_origin", origin);
    }
    let closed = get(scope, port, "_closed");
    set(scope, shipped, "_closed", closed);
    let queue = get(scope, port, "_queue");
    if queue.is_array() {
        set(scope, shipped, "_queue", queue);
    }
    let fresh = scope.new_array();
    set(scope, port, "_queue", fresh);
    set(scope, port, "_pair", Value::null());
    set(scope, port, "_closed", Value::boolean(true));
    set(scope, port, "_shipped", Value::boolean(true));
    let _ = scope.define(port, "_shipped_to", shipped.clone(), HIDDEN);
}

pub(crate) fn transfer_error(scope: &mut Scope<'_>) -> Value {
    ffi::dom_exception(
        scope,
        c"DataCloneError",
        25,
        c"Failed to execute 'postMessage': a MessagePort in the transfer list is the source port, a duplicate, or already transferred.",
    )
}

pub(crate) fn transfer_prepare(
    realm: &mut Scope<'_>,
    transfer: &Value,
    source: Option<&Value>,
) -> Option<Transfer> {
    let old = realm.new_array();
    let new = realm.new_array();
    let len = if transfer.is_array() {
        length(realm, transfer)
    } else {
        0
    };
    let mut seen: Vec<Value> = Vec::new();
    let mut moved = 0;
    for i in 0..len {
        let item = get_index(realm, transfer, i);
        if !ffi::is_message_port(&item) {
            continue;
        }
        let bad = seen.iter().any(|s| s.same_object(&item))
            || source.is_some_and(|source| source.same_object(&item))
            || flag(realm, &item, "_shipped");
        seen.push(item.clone());
        if bad {
            return None;
        }
        let shipped = if bridge_id(realm, &item) != 0 {
            item.clone()
        } else {
            new_port(realm).unwrap_or_else(|_| Value::undefined())
        };
        crate::set_index(realm, &new, moved, shipped);
        crate::set_index(realm, &old, moved, item);
        moved += 1;
    }
    Some(Transfer { old, new })
}

pub(crate) fn transfer_commit(scope: &mut Scope<'_>, moved: &Transfer) {
    if !moved.old.is_array() {
        return;
    }
    for i in 0..length(scope, &moved.old) {
        let from = get_index(scope, &moved.old, i);
        let to = get_index(scope, &moved.new, i);
        if !from.same_object(&to) {
            ship(scope, &from, &to);
        }
    }
}

pub(crate) fn message_channel(
    scope: &mut Scope<'_>,
    new_target: &Value,
    _args: &[Value],
) -> JsResult {
    let channel = ffi::construct_message_channel(scope, new_target)?;
    let port1 = new_port(scope)?;
    let port2 = new_port(scope)?;
    set(scope, &port1, "_pair", port2.clone());
    set(scope, &port2, "_pair", port1.clone());
    set(scope, &channel, "port1", port1);
    set(scope, &channel, "port2", port2);
    Ok(channel)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let port = crate::proto_of(scope, global, "MessagePort");
    if port.is_object() {
        bind(scope, &port, "close", 0, close);
        bind(scope, &port, "postMessage", 1, post_message);
        bind(scope, &port, "start", 0, start);
        let getter = scope.function("get onmessage", 0, onmessage_get);
        let setter = scope.function("set onmessage", 1, onmessage_set);
        let attributes = southstar_js_engine::Attributes {
            writable: false,
            enumerable: true,
            configurable: true,
        };
        let _ = scope.define_accessor(&port, "onmessage", Some(&getter), Some(&setter), attributes);
    }
    crate::broadcast::install(scope, global);
}
