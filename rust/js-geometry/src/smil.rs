//! Southstar — the SVG animation DOM: pausing and seeking an svg element's timeline, and begin, end, start time, duration and target of an animation element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_svg::smil;

use crate::{JsResult, ffi};

fn seconds(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<f64> {
    match args.first() {
        Some(value) => scope.to_number(value),
        None => Ok(0.0),
    }
}

fn set_paused(scope: &mut Scope<'_>, this: &Value, paused: bool) -> JsResult {
    if let Some(svg) = ffi::unwrap_node(this) {
        smil::pause(svg, paused);
        ffi::request_repaint(ffi::js_of(scope));
    }
    Ok(Value::undefined())
}

pub fn pause_animations(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    set_paused(scope, this, true)
}

pub fn unpause_animations(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    set_paused(scope, this, false)
}

pub fn animations_paused(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::boolean(
        ffi::unwrap_node(this).is_some_and(smil::paused),
    ))
}

pub fn get_current_time(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::number(
        ffi::unwrap_node(this).map_or(0.0, smil::current_time),
    ))
}

pub fn set_current_time(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let at = seconds(scope, args)?;
    if let Some(svg) = ffi::unwrap_node(this) {
        smil::set_current_time(svg, at);
        ffi::request_repaint(ffi::js_of(scope));
    }
    Ok(Value::undefined())
}

pub fn begin_element(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let offset = seconds(scope, args)?;
    if ffi::unwrap_node(this).is_some_and(|anim| smil::begin_element(anim, offset)) {
        ffi::request_repaint(ffi::js_of(scope));
    }
    Ok(Value::undefined())
}

pub fn end_element(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let offset = seconds(scope, args)?;
    if ffi::unwrap_node(this).is_some_and(|anim| smil::end_element(anim, offset)) {
        ffi::request_repaint(ffi::js_of(scope));
    }
    Ok(Value::undefined())
}

pub fn get_start_time(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    match ffi::unwrap_node(this).and_then(smil::start_time) {
        Some(start) => Ok(Value::number(start)),
        None => Err(scope.dom_exception("InvalidStateError", "No current interval")),
    }
}

pub fn get_simple_duration(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    match ffi::unwrap_node(this).and_then(smil::simple_duration) {
        Some(duration) => Ok(Value::number(duration)),
        None => Err(scope.dom_exception("NotSupportedError", "The simple duration is not defined")),
    }
}

pub fn get_target_element(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let target = ffi::unwrap_node(this).and_then(smil::target);
    Ok(target.map_or(Value::null(), |target| ffi::wrap_node(scope, target)))
}
