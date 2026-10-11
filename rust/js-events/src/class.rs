//! Southstar — the Event host class: the engine's event state lives in a hidden object that engine code reads and writes as the event's own properties, while page scripts see only isTrusted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyDefinition, PropertyKey, Scope, SetFlags, Trace,
    Value,
};

pub(crate) static EVENT_CLASS: HostClass = HostClass::new(
    c"Event",
    ClassHooks {
        get_own_property: Some(get_own),
        own_keys: Some(own_keys),
        delete: Some(delete),
        define_own_property: Some(define_own),
        set: Some(set),
        ..ClassHooks::NONE
    },
);

struct EventData {
    state: Value,
}

impl Trace for EventData {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.state);
    }
}

pub(crate) fn state_of(value: &Value) -> Option<Value> {
    value.with_instance(&EVENT_CLASS, |data: &EventData| data.state.clone())
}

pub(crate) fn new_event(scope: &mut Scope<'_>, proto: Option<&Value>) -> Result<Value, Value> {
    scope.register_class(&EVENT_CLASS);
    let object_proto = match proto.filter(|proto| proto.is_object()) {
        Some(proto) => proto.clone(),
        None => {
            let plain = scope.new_object();
            scope
                .get_prototype(&plain)
                .unwrap_or_else(|_| Value::undefined())
        }
    };
    let state = scope.new_object_with_proto(&Value::null());
    scope.new_traced_instance(&EVENT_CLASS, Some(&object_proto), EventData { state })
}

fn keeps_own(scope: &mut Scope<'_>, key: &PropertyKey) -> bool {
    key.to_c_bytes(scope)
        .is_some_and(|name| name == b"isTrusted")
}

fn get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(state) = state_of(obj) else {
        return Ok(None);
    };
    if scope.is_host_access() {
        return scope.own_property_of(&state, key);
    }
    let Some(own) = scope.own_property_of(&state, key)? else {
        return Ok(None);
    };
    let mut method = !own.accessor && scope.is_function(&own.value);
    if method {
        let proto = scope
            .get_prototype(obj)
            .unwrap_or_else(|_| Value::undefined());
        let inherited = if proto.is_object() {
            scope.has_key(&proto, key).map_or(-1, i32::from)
        } else {
            0
        };
        method = inherited == 0;
    }
    Ok(method.then_some(own))
}

fn own_keys(scope: &mut Scope<'_>, obj: &Value) -> Result<Vec<PropertyKey>, Value> {
    let Some(state) = state_of(obj) else {
        return Ok(Vec::new());
    };
    if !scope.is_host_access() {
        return Ok(Vec::new());
    }
    scope.own_keys_of(&state)
}

fn delete(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> Result<bool, Value> {
    let Some(state) = state_of(obj) else {
        return Ok(true);
    };
    if !scope.is_host_access() {
        return Ok(true);
    }
    scope.delete_key_of(&state, key)
}

fn define_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    definition: &PropertyDefinition,
) -> Result<bool, Value> {
    if let Some(state) = state_of(obj)
        && scope.is_host_access()
    {
        if !keeps_own(scope, key) {
            return scope.define_forward(&state, key, definition);
        }
        let mut own = definition.clone();
        own.clear_configurable();
        return scope.define_ordinary(obj, key, &own);
    }
    scope.define_ordinary(obj, key, definition)
}

fn set_state(
    scope: &mut Scope<'_>,
    state: &Value,
    obj: &Value,
    key: &PropertyKey,
    value: &Value,
    flags: SetFlags,
) -> Result<bool, Value> {
    if let Some(own) = scope.own_property_of(state, key)?
        && own.accessor
    {
        if scope.is_function(&own.setter) {
            scope.call(&own.setter, obj, core::slice::from_ref(value))?;
        }
        return Ok(true);
    }
    scope.set_with_receiver(state, key, value, state, flags)
}

fn set(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    value: &Value,
    receiver: &Value,
    flags: SetFlags,
) -> Result<bool, Value> {
    if let Some(state) = state_of(obj)
        && scope.is_host_access()
        && receiver.same_object(obj)
        && !keeps_own(scope, key)
    {
        return set_state(scope, &state, obj, key, value, flags);
    }
    let proto = scope
        .get_prototype(obj)
        .unwrap_or_else(|_| Value::undefined());
    if proto.is_object() {
        scope.set_with_receiver(&proto, key, value, receiver, flags)
    } else {
        scope.define_value_key(receiver, key, value)
    }
}
