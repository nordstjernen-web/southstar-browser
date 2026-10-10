//! Southstar — queueMicrotask and the Notification stub.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi;

pub(crate) fn queue_microtask(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    match args.first() {
        Some(callback) if scope.is_function(callback) => {
            scope.enqueue_call(callback, &[])?;
            Ok(Value::undefined())
        }
        _ => Err(scope.type_error("queueMicrotask: argument is not a function")),
    }
}

fn notification_close(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    scope.set(this, "_closed", Value::boolean(true))?;
    ffi::fire_event(scope, this, "close");
    Ok(Value::undefined())
}

pub(crate) fn notification(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let notification = scope.new_object();
    if let Some(title) = args.first().filter(|title| title.is_string()) {
        scope.set(&notification, "title", title.clone())?;
    }
    if let Some(options) = args.get(1).filter(|options| options.is_object()) {
        let body = scope.get(options, "body")?;
        if !body.is_undefined() {
            scope.set(&notification, "body", body)?;
        }
    }
    let listeners = scope.new_array();
    scope.set(&notification, "_listeners", listeners)?;
    scope.set(&notification, "_closed", Value::boolean(false))?;
    ffi::bind_event_target(scope, &notification);
    let close = scope.function("close", 0, notification_close);
    scope.set(&notification, "close", close)?;
    Ok(notification)
}
