//! Southstar — Element.setPointerCapture(), releasePointerCapture() and hasPointerCapture() over the element's list of captured pointer ids.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

const CAPTURES: &str = "_pointerCaptures";

fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    scope
        .get(array, "length")
        .and_then(|length| scope.to_number(&length))
        .map_or(0, |length| length as u32)
}

fn capture_index(scope: &mut Scope<'_>, captures: &Value, id: i32) -> Option<u32> {
    if !captures.is_array() {
        return None;
    }
    (0..array_length(scope, captures)).find(|&index| {
        scope
            .get_index(captures, index)
            .and_then(|entry| scope.to_int32(&entry))
            .unwrap_or(0)
            == id
    })
}

fn pointer_id(scope: &mut Scope<'_>, args: &[Value]) -> Option<i32> {
    let id = args.first()?;
    Some(scope.to_int32(id).unwrap_or(0))
}

fn captures_of(scope: &mut Scope<'_>, element: &Value) -> Value {
    scope
        .get(element, CAPTURES)
        .unwrap_or_else(|_| Value::undefined())
}

pub fn set(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(id) = pointer_id(scope, args) else {
        return Ok(Value::undefined());
    };
    let mut captures = captures_of(scope, this);
    if !captures.is_array() {
        captures = scope.new_array();
        let _ = scope.set(this, CAPTURES, captures.clone());
    }
    if capture_index(scope, &captures, id).is_none() {
        let length = array_length(scope, &captures);
        let _ = scope.set_index(&captures, length, Value::int(id));
    }
    Ok(Value::undefined())
}

pub fn has(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(id) = pointer_id(scope, args) else {
        return Ok(Value::boolean(false));
    };
    let captures = captures_of(scope, this);
    Ok(Value::boolean(
        capture_index(scope, &captures, id).is_some(),
    ))
}

pub fn release(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(id) = pointer_id(scope, args) else {
        return Ok(Value::undefined());
    };
    let captures = captures_of(scope, this);
    if let Some(index) = capture_index(scope, &captures, id) {
        let splice = scope
            .get(&captures, "splice")
            .unwrap_or_else(|_| Value::undefined());
        let _ = scope.call(
            &splice,
            &captures,
            &[Value::int(index as i32), Value::int(1)],
        );
    }
    Ok(Value::undefined())
}
