//! Southstar — the host-object class: platform objects whose engine state lives in a hidden object that engine code reads and writes as their own properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{
    ClassHooks, HostClass, OwnProperty, PropertyDefinition, PropertyKey, Scope, SetFlags, Trace,
    Value,
};

pub(crate) static HOST_OBJECT_CLASS: HostClass = HostClass::new(
    c"Object",
    ClassHooks {
        get_own_property: Some(get_own),
        own_keys: Some(own_keys),
        delete: Some(delete),
        define_own_property: Some(define_own),
        set: Some(set),
        ..ClassHooks::NONE
    },
);

struct HostObject {
    kind: i32,
    state: Value,
}

impl Trace for HostObject {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.state);
    }
}

pub(crate) fn kind_of(value: &Value) -> Option<i32> {
    value.with_instance(&HOST_OBJECT_CLASS, |host: &HostObject| host.kind)
}

pub(crate) fn state_of(value: &Value) -> Option<Value> {
    value.with_instance(&HOST_OBJECT_CLASS, |host: &HostObject| host.state.clone())
}

pub(crate) fn kind_state(value: &Value, kind: i32) -> Option<Value> {
    value
        .with_instance(&HOST_OBJECT_CLASS, |host: &HostObject| {
            (host.kind == kind).then(|| host.state.clone())
        })
        .flatten()
}

pub(crate) fn new_host_object(
    scope: &mut Scope<'_>,
    kind: i32,
    proto: &Value,
) -> Result<Value, Value> {
    scope.register_class(&HOST_OBJECT_CLASS);
    let proto = if proto.is_object() {
        proto.clone()
    } else {
        let plain = scope.new_object();
        scope
            .get_prototype(&plain)
            .unwrap_or_else(|_| Value::undefined())
    };
    let state = scope.new_object_with_proto(&Value::null());
    scope.new_traced_instance(&HOST_OBJECT_CLASS, Some(&proto), HostObject { kind, state })
}

fn get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let Some(state) = state_of(obj) else {
        return Ok(None);
    };
    if !scope.is_host_access() {
        return Ok(None);
    }
    scope.own_property_of(&state, key)
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
        return scope.define_forward(&state, key, definition);
    }
    scope.define_ordinary(obj, key, definition)
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
    {
        if let Some(own) = scope.own_property_of(&state, key)?
            && own.accessor
        {
            if scope.is_function(&own.setter) {
                scope.call(&own.setter, obj, core::slice::from_ref(value))?;
            }
            return Ok(true);
        }
        return scope.set_with_receiver(&state, key, value, &state, flags);
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
