//! Southstar — the text control selection APIs, setRangeText, textLength and stepUp/stepDown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::controls::{self, Step};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Page};
use crate::{Element, JsResult, length, named, to_uint32, type_of, uint32, value};

const EXCLUDED: [&str; 17] = [
    "hidden",
    "email",
    "datetime-local",
    "date",
    "month",
    "week",
    "time",
    "number",
    "range",
    "color",
    "checkbox",
    "radio",
    "file",
    "submit",
    "image",
    "reset",
    "button",
];

pub(crate) fn applies(el: Element) -> bool {
    if named(el, "textarea") {
        return true;
    }
    if !named(el, "input") {
        return false;
    }
    match type_of(el).filter(|t| !t.is_empty()) {
        None => true,
        Some(ty) => !EXCLUDED
            .iter()
            .any(|e| ty.to_bytes().eq_ignore_ascii_case(e.as_bytes())),
    }
}

fn applies_to(this: &Value) -> bool {
    ffi::element(this).is_some_and(applies)
}

fn value_length(scope: &mut Scope<'_>, this: &Value) -> u32 {
    match value::get_value(scope, this, &[]) {
        Ok(v) => length(scope, &v),
        Err(_) => 0,
    }
}

fn set_state(scope: &mut Scope<'_>, this: &Value, start: u32, end: u32, direction: &str) {
    let _ = scope.set(this, "_selStart", Value::number(f64::from(start)));
    let _ = scope.set(this, "_selEnd", Value::number(f64::from(end)));
    let direction = scope.string(direction);
    let _ = scope.set(this, "_selDir", direction);
}

fn same_string(scope: &mut Scope<'_>, a: &Value, b: &Value) -> bool {
    if !a.is_string() || !b.is_string() {
        return false;
    }
    let a = scope.to_bytes(a).ok();
    let b = scope.to_bytes(b).ok();
    a.is_some() && a == b
}

pub(crate) fn value_changed(scope: &mut Scope<'_>, this: &Value, old_value: &Value) {
    let Ok(new_value) = value::get_value(scope, this, &[]) else {
        return;
    };
    if !same_string(scope, old_value, &new_value) {
        let len = length(scope, &new_value);
        set_state(scope, this, len, len, "none");
    }
}

fn stored(scope: &mut Scope<'_>, this: &Value, property: &str) -> u32 {
    let value = scope
        .get(this, property)
        .unwrap_or_else(|_| Value::undefined());
    if value.is_undefined() {
        0
    } else {
        uint32(scope, &value)
    }
}

fn position(scope: &mut Scope<'_>, this: &Value, property: &str) -> u32 {
    let len = value_length(scope, this);
    stored(scope, this, property).min(len)
}

fn direction_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<&'static str> {
    let text = crate::until_nul(scope.to_bytes(value)?);
    Ok(match text.as_slice() {
        b"forward" => "forward",
        b"backward" => "backward",
        _ => "none",
    })
}

fn invalid_state(scope: &mut Scope<'_>) -> Value {
    ffi::dom_exception(
        scope,
        c"InvalidStateError",
        11,
        "The element does not support text selection.",
    )
}

fn dispatch(scope: &mut Scope<'_>, this: &Value) {
    let (Some(el), Some(page)) = (ffi::element(this), Page::of(scope)) else {
        return;
    };
    if page.halted() || page.in_pump() {
        return;
    }
    page.dispatch_select(scope, el);
}

fn arg(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn selection_start(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Ok(Value::null());
    }
    Ok(Value::number(f64::from(position(scope, this, "_selStart"))))
}

pub(crate) fn selection_end(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Ok(Value::null());
    }
    Ok(Value::number(f64::from(position(scope, this, "_selEnd"))))
}

pub(crate) fn selection_direction(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Ok(Value::null());
    }
    let stored = scope
        .get(this, "_selDir")
        .unwrap_or_else(|_| Value::undefined());
    let text = if stored.is_string() {
        scope.to_bytes(&stored).map(crate::until_nul).ok()
    } else {
        None
    };
    let direction = match text.as_deref() {
        Some(b"forward") => "forward",
        Some(b"backward") => "backward",
        _ => "none",
    };
    Ok(scope.string(direction))
}

pub(crate) fn set_selection_start(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Err(invalid_state(scope));
    }
    let wanted = to_uint32(scope, &arg(args))?;
    let start = wanted.min(value_length(scope, this));
    let end = position(scope, this, "_selEnd").max(start);
    let _ = scope.set(this, "_selStart", Value::number(f64::from(start)));
    let _ = scope.set(this, "_selEnd", Value::number(f64::from(end)));
    dispatch(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn set_selection_end(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Err(invalid_state(scope));
    }
    let wanted = to_uint32(scope, &arg(args))?;
    let end = wanted.min(value_length(scope, this));
    let start = position(scope, this, "_selStart").min(end);
    let _ = scope.set(this, "_selStart", Value::number(f64::from(start)));
    let _ = scope.set(this, "_selEnd", Value::number(f64::from(end)));
    dispatch(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn set_selection_direction(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if !applies_to(this) {
        return Err(invalid_state(scope));
    }
    let direction = direction_arg(scope, &arg(args))?;
    let direction = scope.string(direction);
    let _ = scope.set(this, "_selDir", direction);
    dispatch(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn text_length(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !ffi::element(this).is_some_and(|el| named(el, "input") || named(el, "textarea")) {
        return Ok(Value::undefined());
    }
    Ok(Value::number(f64::from(value_length(scope, this))))
}

pub(crate) fn select(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !applies_to(this) {
        return Ok(Value::undefined());
    }
    let len = value_length(scope, this);
    set_state(scope, this, 0, len, "none");
    dispatch(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn set_selection_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        let message = format!("2 arguments required, but only {} present", args.len());
        return Err(scope.type_error(&message));
    }
    if !applies_to(this) {
        return Err(invalid_state(scope));
    }
    let start = to_uint32(scope, &args[0])?;
    let end = to_uint32(scope, &args[1])?;
    let len = value_length(scope, this);
    let end = end.min(len);
    let start = if end <= start.min(len) {
        end
    } else {
        start.min(len)
    };
    let direction = match args.get(2) {
        Some(d) => direction_arg(scope, d)?,
        None => "none",
    };
    set_state(scope, this, start, end, direction);
    dispatch(scope, this);
    Ok(Value::undefined())
}

fn slice(scope: &mut Scope<'_>, string: &Value, start: u32, end: u32) -> JsResult {
    let method = scope.get(string, "slice")?;
    scope.call(
        &method,
        string,
        &[
            Value::number(f64::from(start)),
            Value::number(f64::from(end)),
        ],
    )
}

pub(crate) fn set_range_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    }
    if args.len() == 2 {
        return Err(scope.type_error("3 arguments required, but only 2 present"));
    }
    if !applies_to(this) {
        return Err(invalid_state(scope));
    }
    let replacement = scope.to_string_value(&args[0])?;
    let value = value::get_value(scope, this, &[])?;
    let len = value_length(scope, this);
    let old_start = position(scope, this, "_selStart");
    let old_end = position(scope, this, "_selEnd");
    let (mut start, mut end) = (old_start, old_end);
    if args.len() >= 3 {
        start = to_uint32(scope, &args[1])?;
        end = to_uint32(scope, &args[2])?;
    }
    if start > end {
        return Err(ffi::dom_exception(
            scope,
            c"IndexSizeError",
            1,
            "The start value is greater than the end value.",
        ));
    }
    let start = start.min(len);
    let end = end.min(len);
    let mode = match args.get(3).filter(|m| !m.is_undefined()) {
        None => b"preserve".to_vec(),
        Some(m) => {
            let requested = crate::until_nul(scope.to_bytes(m)?);
            if !matches!(
                requested.as_slice(),
                b"select" | b"start" | b"end" | b"preserve"
            ) {
                return Err(scope.type_error("Invalid value for SelectionMode"));
            }
            requested
        }
    };
    let replacement_length = length(scope, &replacement);
    let prefix = slice(scope, &value, 0, start)?;
    let suffix = slice(scope, &value, end, len)?;
    let concat = scope.get(&prefix, "concat")?;
    let new_value = scope.call(&concat, &prefix, &[replacement, suffix])?;
    value::set_value(scope, this, &[new_value])?;
    let replacement_end = start.wrapping_add(replacement_length);
    let new_length = len
        .wrapping_sub(end - start)
        .wrapping_add(replacement_length);
    let (new_start, new_end) = match mode.as_slice() {
        b"select" => (start, replacement_end),
        b"start" => (start, start),
        b"end" => (replacement_end, replacement_end),
        _ => {
            let delta = i64::from(replacement_length) - i64::from(end - start);
            let shift = |old: u32, collapsed: u32| {
                if old > end {
                    (i64::from(old) + delta) as u32
                } else if old > start {
                    collapsed
                } else {
                    old
                }
            };
            (shift(old_start, start), shift(old_end, replacement_end))
        }
    };
    set_state(
        scope,
        this,
        new_start.min(new_length),
        new_end.min(new_length),
        "none",
    );
    dispatch(scope, this);
    Ok(Value::undefined())
}

pub(crate) fn step(scope: &mut Scope<'_>, this: &Value, args: &[Value], sign: i32) -> JsResult {
    let Some(el) = ffi::element(this).filter(|e| named(*e, "input")) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    let n = match args.first() {
        Some(a) => scope.to_int32(a)?,
        None => 1,
    };
    match controls::step_apply(el, sign, f64::from(n), 96) {
        Step::NotApplicable => Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            "stepUp/stepDown does not apply to this input type",
        )),
        Step::NoStep => Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            "the input has no allowed value step",
        )),
        Step::Applied(text) => {
            let text = scope.string_from_bytes(&text);
            let _ = value::set_value(scope, this, &[text]);
            Ok(Value::undefined())
        }
        Step::Unchanged => Ok(Value::undefined()),
    }
}

pub(crate) fn step_up(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    step(scope, this, args, 1)
}

pub(crate) fn step_down(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    step(scope, this, args, -1)
}
