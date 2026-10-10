//! Southstar — the WebRTC surface: RTCPeerConnection and RTCDataChannel objects that negotiate nothing and never connect.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::{ffi, make_ctor, resolved};

const EVENT_HANDLERS: &[&str] = &[
    "onconnectionstatechange",
    "ondatachannel",
    "onicecandidate",
    "onicecandidateerror",
    "oniceconnectionstatechange",
    "onicegatheringstatechange",
    "onnegotiationneeded",
    "onsignalingstatechange",
    "ontrack",
];

const NULL_PROPERTIES: &[&str] = &[
    "canTrickleIceCandidates",
    "localDescription",
    "remoteDescription",
    "currentLocalDescription",
    "currentRemoteDescription",
    "pendingLocalDescription",
    "pendingRemoteDescription",
    "sctp",
];

fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) -> Result<(), Value> {
    let value = scope.string(text);
    scope.set(object, key, value)
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    let _ = scope.set(object, name, function);
}

fn object_with_prototype(scope: &mut Scope<'_>, constructor: &str) -> Value {
    let object = scope.new_object();
    let global = scope.global();
    let ctor = scope
        .get(&global, constructor)
        .unwrap_or_else(|_| Value::undefined());
    let proto = if ctor.is_undefined() || ctor.is_null() {
        Value::undefined()
    } else {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    };
    if proto.is_object() {
        let _ = scope.set_prototype(&object, &proto);
    }
    object
}

fn invalid_state(scope: &mut Scope<'_>, message: &str) -> Value {
    let error = scope.new_error();
    let name = scope.string("InvalidStateError");
    let _ = scope.define(&error, "name", name, Attributes::METHOD);
    let message = scope.string(message);
    let _ = scope.define(&error, "message", message, Attributes::METHOD);
    let _ = scope.define(&error, "code", Value::int(11), Attributes::METHOD);
    let global = scope.global();
    let dom_exception = scope
        .get(&global, "DOMException")
        .unwrap_or_else(|_| Value::undefined());
    if dom_exception.is_object() {
        let proto = scope
            .get(&dom_exception, "prototype")
            .unwrap_or_else(|_| Value::undefined());
        if proto.is_object() {
            let _ = scope.set_prototype(&error, &proto);
        }
    }
    error
}

fn channel_close(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    set_str(scope, this, "readyState", "closed")?;
    Ok(Value::undefined())
}

fn channel_send(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Err(invalid_state(scope, "RTCDataChannel is not open"))
}

fn create_data_channel(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = match args.first() {
        Some(label) => scope.to_string(label)?,
        None => String::new(),
    };
    let channel = object_with_prototype(scope, "RTCDataChannel");
    set_str(scope, &channel, "label", &label)?;
    set_str(scope, &channel, "readyState", "connecting")?;
    scope.set(&channel, "bufferedAmount", Value::int(0))?;
    scope.set(&channel, "bufferedAmountLowThreshold", Value::int(0))?;
    set_str(scope, &channel, "binaryType", "arraybuffer")?;
    scope.set(&channel, "ordered", Value::boolean(true))?;
    scope.set(&channel, "maxPacketLifeTime", Value::null())?;
    scope.set(&channel, "maxRetransmits", Value::null())?;
    scope.set(&channel, "negotiated", Value::boolean(false))?;
    scope.set(&channel, "id", Value::null())?;
    set_str(scope, &channel, "protocol", "")?;
    ffi::bind_event_target(scope, &channel);
    bind(scope, &channel, "close", 0, channel_close);
    bind(scope, &channel, "send", 1, channel_send);
    Ok(channel)
}

fn description(scope: &mut Scope<'_>, kind: &str) -> Result<Value, Value> {
    let description = scope.new_object();
    set_str(scope, &description, "type", kind)?;
    set_str(scope, &description, "sdp", "v=0\r\n")?;
    resolved(scope, description)
}

fn create_offer(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    description(scope, "offer")
}

fn create_answer(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    description(scope, "answer")
}

fn set_local_description(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let description = args.first().cloned().unwrap_or_else(Value::null);
    let _ = scope.set(this, "localDescription", description);
    resolved(scope, Value::undefined())
}

fn resolved_undefined(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    resolved(scope, Value::undefined())
}

fn resolved_empty_array(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let array = scope.new_array();
    resolved(scope, array)
}

fn empty_array(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(scope.new_array())
}

fn close(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    for key in ["connectionState", "iceConnectionState", "signalingState"] {
        set_str(scope, this, key, "closed")?;
    }
    Ok(Value::undefined())
}

fn peer_connection(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let connection = object_with_prototype(scope, "RTCPeerConnection");
    set_str(scope, &connection, "connectionState", "new")?;
    set_str(scope, &connection, "iceConnectionState", "new")?;
    set_str(scope, &connection, "iceGatheringState", "new")?;
    set_str(scope, &connection, "signalingState", "stable")?;
    for key in NULL_PROPERTIES {
        scope.set(&connection, key, Value::null())?;
    }
    let configuration = match args.first() {
        Some(configuration) if configuration.is_object() => configuration.clone(),
        _ => scope.new_object(),
    };
    scope.set(&connection, "_configuration", configuration)?;
    ffi::bind_event_target(scope, &connection);
    for key in EVENT_HANDLERS {
        scope.set(&connection, key, Value::null())?;
    }
    Ok(connection)
}

fn illegal_constructor(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Err(scope.type_error("Illegal constructor"))
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let ctor = make_ctor(scope, "RTCPeerConnection", 1, peer_connection);
    let proto = scope
        .get(&ctor, "prototype")
        .unwrap_or_else(|_| Value::undefined());
    let _ = scope.set(global, "RTCPeerConnection", ctor);
    let channel = make_ctor(scope, "RTCDataChannel", 0, illegal_constructor);
    let _ = scope.set(global, "RTCDataChannel", channel);
    let methods: [(&str, u32, NativeFn); 11] = [
        ("createDataChannel", 2, create_data_channel),
        ("createOffer", 1, create_offer),
        ("createAnswer", 1, create_answer),
        ("setLocalDescription", 1, set_local_description),
        ("setRemoteDescription", 1, resolved_undefined),
        ("addIceCandidate", 1, resolved_undefined),
        ("getStats", 1, resolved_empty_array),
        ("getSenders", 0, empty_array),
        ("getReceivers", 0, empty_array),
        ("getTransceivers", 0, empty_array),
        ("close", 0, close),
    ];
    for (name, arity, f) in methods {
        bind(scope, &proto, name, arity, f);
    }
}
