//! Southstar — the WindowProperties host class: the object in the window's prototype chain that answers named access to elements and frames.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ClassHooks, HostClass, OwnProperty, PropertyKey, Scope, Value};

use crate::named;

pub(crate) static WINDOW_NAMED_CLASS: HostClass = HostClass::new(
    c"WindowProperties",
    ClassHooks {
        get_own_property: Some(get_own),
        get_own_property_receiver: Some(get_own_receiver),
        ..ClassHooks::NONE
    },
);

pub fn register(scope: &mut Scope<'_>) {
    scope.register_class(&WINDOW_NAMED_CLASS);
}

pub(crate) fn new_window_named(scope: &mut Scope<'_>) -> Result<Value, Value> {
    scope.new_instance(&WINDOW_NAMED_CLASS, None)
}

fn fill(scope: &mut Scope<'_>, window: &Value, key: &PropertyKey) -> Option<OwnProperty> {
    let key = key.to_value(scope);
    let value = named::named_property(scope, window, &key);
    value
        .is_object()
        .then(|| OwnProperty::data(value, true, false, true))
}

fn get_own(
    scope: &mut Scope<'_>,
    _obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    let global = scope.global();
    Ok(fill(scope, &global, key))
}

fn get_own_receiver(
    scope: &mut Scope<'_>,
    _obj: &Value,
    key: &PropertyKey,
    receiver: &Value,
) -> Result<Option<OwnProperty>, Value> {
    Ok(fill(scope, receiver, key))
}
