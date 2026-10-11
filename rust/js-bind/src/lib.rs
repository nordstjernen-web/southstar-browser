//! Southstar — generic binding helpers: functions, constructors, namespace objects, constants, stock natives and the host-object interfaces.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod class;
mod ffi;
pub mod host;
pub mod natives;

use southstar_js_engine::{Attributes, Scope, Value};

pub use host::HostKind;

const PROTOTYPE_SLOT: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

pub fn bind_function(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    function: Value,
) -> Result<(), Value> {
    if name.starts_with("__") {
        scope.define(object, name, function, Attributes::METHOD)
    } else {
        scope.set(object, name, function)
    }
}

pub fn bind_function_if_missing(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    make: impl FnOnce(&mut Scope<'_>) -> Value,
) -> Result<(), Value> {
    if scope.has_property(object, name).unwrap_or(false) {
        return Ok(());
    }
    let function = make(scope);
    bind_function(scope, object, name, function)
}

pub fn bind_prototype_function_if_missing(
    scope: &mut Scope<'_>,
    global: &Value,
    constructor_name: &str,
    name: &str,
    make: impl FnOnce(&mut Scope<'_>) -> Value,
) -> Result<(), Value> {
    let constructor = scope.get(global, constructor_name)?;
    let prototype = scope.get(&constructor, "prototype")?;
    if !prototype.is_object() {
        return Ok(());
    }
    bind_function_if_missing(scope, &prototype, name, make)
}

pub fn attach_prototype(
    scope: &mut Scope<'_>,
    constructor: &Value,
    name: &str,
) -> Result<(), Value> {
    let prototype = scope.new_object();
    scope.define(
        &prototype,
        "constructor",
        constructor.clone(),
        Attributes::METHOD,
    )?;
    let _ = scope.define_to_string_tag(&prototype, name);
    scope.define(constructor, "prototype", prototype, PROTOTYPE_SLOT)
}

pub fn bind_int_constants<'n>(
    scope: &mut Scope<'_>,
    global: &Value,
    constructor_name: &str,
    constants: impl IntoIterator<Item = (&'n str, i32)>,
) -> Result<(), Value> {
    let constructor = scope.get(global, constructor_name)?;
    if !constructor.is_object() {
        return Ok(());
    }
    let prototype = scope
        .get(&constructor, "prototype")
        .unwrap_or_else(|_| Value::undefined());
    for (name, value) in constants {
        let _ = scope.define(
            &constructor,
            name,
            Value::int(value),
            Attributes::ENUMERABLE,
        );
        if prototype.is_object() {
            let _ = scope.define(&prototype, name, Value::int(value), Attributes::ENUMERABLE);
        }
    }
    Ok(())
}

pub fn has_defined_own(scope: &mut Scope<'_>, object: &Value, name: &str) -> bool {
    let key = scope.string(name);
    match scope.own_property(object, &key) {
        Ok(Some(descriptor)) => descriptor.accessor || !descriptor.value.is_undefined(),
        _ => false,
    }
}

pub fn prototype_of(scope: &mut Scope<'_>, global: &Value, constructor_name: &str) -> Value {
    match scope.get(global, constructor_name) {
        Ok(constructor) if constructor.is_object() => scope
            .get(&constructor, "prototype")
            .unwrap_or_else(|_| Value::undefined()),
        _ => Value::undefined(),
    }
}
