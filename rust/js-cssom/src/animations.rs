//! Southstar — the Web Animations hooks the polyfill calls: listing, querying, seeking and controlling CSS animations and transitions, their keyframes, and starting script animations.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_anim::{AnimInfo, AnimTiming};
use southstar_js_engine::{Scope, Value};
use southstar_style::PropId;

use crate::ffi;

const BAD_PROP: i32 = -2;

fn strtol_prefix(text: &str) -> Option<i64> {
    let t = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let end = digits
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    let magnitude = digits[..end].parse::<i64>().unwrap_or(i64::MAX);
    Some(if negative { -magnitude } else { magnitude })
}

fn prop_arg(scope: &mut Scope<'_>, value: &Value) -> i32 {
    if value.is_null() || value.is_undefined() {
        return -1;
    }
    let Ok(name) = scope.to_string(value) else {
        return BAD_PROP;
    };
    if let Some(index) = name.strip_prefix('@') {
        return match strtol_prefix(index) {
            Some(idx) if (0..1024).contains(&idx) => -1 - idx as i32,
            _ => BAD_PROP,
        };
    }
    ffi::prop_id(&name).map_or(BAD_PROP, |id| id as i32)
}

fn set_number(scope: &mut Scope<'_>, object: &Value, key: &str, value: f64) -> Result<(), Value> {
    scope.set(object, key, Value::number(value))
}

fn set_text(scope: &mut Scope<'_>, object: &Value, key: &str, value: &str) -> Result<(), Value> {
    let value = scope.string(value);
    scope.set(object, key, value)
}

fn info_to_js(scope: &mut Scope<'_>, info: &AnimInfo) -> Result<Value, Value> {
    let o = scope.new_object();
    set_number(scope, &o, "currentMs", info.current_ms)?;
    set_number(scope, &o, "durationMs", info.duration_ms)?;
    set_number(scope, &o, "delayMs", info.delay_ms)?;
    set_number(scope, &o, "iterations", info.iterations)?;
    scope.set(&o, "active", Value::boolean(info.active))?;
    scope.set(&o, "paused", Value::boolean(info.paused))?;
    scope.set(&o, "pending", Value::boolean(info.pending))?;
    scope.set(&o, "finished", Value::boolean(info.finished))?;
    scope.set(&o, "generation", Value::int(info.generation as i32))?;
    scope.set(&o, "run", Value::int(info.run))?;
    set_text(scope, &o, "fill", info.fill)?;
    set_text(scope, &o, "direction", info.direction)?;
    set_text(scope, &o, "easing", &info.easing)?;
    let name = match &info.name {
        Some(name) => scope.string(name),
        None => Value::null(),
    };
    scope.set(&o, "name", name)?;
    let prop = match usize::try_from(info.prop).ok().and_then(ffi::prop_name) {
        Some(name) => scope.string(name),
        None => Value::null(),
    };
    scope.set(&o, "prop", prop)?;
    Ok(o)
}

pub(crate) fn list(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let arr = scope.new_array();
    let js = ffi::js_of(scope);
    let Some(anim) = ffi::anim(js) else {
        return Ok(arr);
    };
    ffi::flush_style(js);
    let filter = args.first().filter(|v| !v.is_null() && !v.is_undefined());
    let node = filter.and_then(ffi::unwrap_element);
    if filter.is_some() && node.is_none() {
        return Ok(arr);
    }
    for (i, info) in anim.visit(node).iter().enumerate() {
        let o = info_to_js(scope, info)?;
        let el = ffi::make_element(scope, info.node.as_ptr());
        scope.set(&o, "el", el)?;
        scope.set_index(&arr, i as u32, o)?;
    }
    Ok(arr)
}

pub(crate) fn query(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let (Some(anim), [node, prop, ..]) = (ffi::anim(js), args) else {
        return Ok(Value::null());
    };
    let node = ffi::unwrap_element(node);
    let prop = prop_arg(scope, prop);
    let Some(node) = node.filter(|_| prop != BAD_PROP) else {
        return Ok(Value::null());
    };
    ffi::flush_style(js);
    match anim.info_for(node, prop) {
        Some(info) => info_to_js(scope, &info),
        None => Ok(Value::null()),
    }
}

pub(crate) fn seek(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let (Some(anim), [node, prop, ms, ..]) = (ffi::anim(js), args) else {
        return Ok(Value::boolean(false));
    };
    let node = ffi::unwrap_element(node);
    let prop = prop_arg(scope, prop);
    let (Some(node), true) = (node, prop != BAD_PROP) else {
        return Ok(Value::boolean(false));
    };
    let Ok(ms) = scope.to_number(ms) else {
        return Ok(Value::boolean(false));
    };
    let ok = anim.seek(node, prop, ms);
    if ok {
        ffi::mark_mutated(js);
    }
    Ok(Value::boolean(ok))
}

pub(crate) fn control(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let (Some(anim), [node, prop, op, ..]) = (ffi::anim(js), args) else {
        return Ok(Value::boolean(false));
    };
    let node = ffi::unwrap_element(node);
    let prop = prop_arg(scope, prop);
    let op = scope.to_string(op).ok();
    let (Some(node), true, Some(op)) = (node, prop != BAD_PROP, op) else {
        return Ok(Value::boolean(false));
    };
    let ok = anim.control(node, prop, op.as_bytes());
    if ok {
        ffi::mark_mutated(js);
    }
    Ok(Value::boolean(ok))
}

pub(crate) fn keyframes(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let arr = scope.new_array();
    let js = ffi::js_of(scope);
    let (Some(anim), [node, prop, ..]) = (ffi::anim(js), args) else {
        return Ok(arr);
    };
    let node = ffi::unwrap_element(node);
    let prop = prop_arg(scope, prop);
    let Some(node) = node.filter(|_| prop < 0 && prop != BAD_PROP) else {
        return Ok(arr);
    };
    ffi::flush_style(js);
    let run_easing = anim
        .info_for(node, prop)
        .map_or_else(|| "ease".to_string(), |i| i.easing);
    for (i, frame) in anim.keyframes(node, prop).iter().enumerate() {
        let run = scope.string(&run_easing);
        let o = scope.new_object();
        let props = scope.new_object();
        let mut composite = "auto".to_string();
        let mut count = 0;
        for decl in frame.decls.iter().flat_map(|decls| decls.iter()) {
            let (prop, value) = (decl.prop(), decl.value());
            let Some(name) = usize::try_from(prop).ok().and_then(ffi::prop_name) else {
                continue;
            };
            if prop == PropId::AnimationComposition as i32
                && let Some(kw) = value.keyword()
            {
                composite = kw.to_string_lossy().into_owned();
                continue;
            }
            if name.starts_with("animation") || name.starts_with("transition") {
                continue;
            }
            let text = ffi::serialize_value(value);
            if !text.is_empty() {
                set_text(scope, &props, name, &text)?;
                count += 1;
            }
        }
        set_number(scope, &o, "offset", frame.offset)?;
        set_text(scope, &o, "easing", &frame.easing)?;
        set_text(scope, &o, "composite", &composite)?;
        scope.set(&o, "count", Value::int(count))?;
        scope.set(&o, "props", props)?;
        scope.set(&o, "runEasing", run)?;
        scope.set_index(&arr, i as u32, o)?;
    }
    Ok(arr)
}

pub(crate) fn base_value(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let (Some(anim), [node, name, ..]) = (ffi::anim(js), args) else {
        return Ok(Value::null());
    };
    let node = ffi::unwrap_element(node);
    let name = scope.to_string(name).ok();
    let (Some(node), Some(name)) = (node, name) else {
        return Ok(Value::null());
    };
    let Some(prop) = ffi::prop_id(&name) else {
        return Ok(Value::null());
    };
    ffi::flush_style(js);
    let Some(value) = anim.base_value(node, prop as i32) else {
        let initial = ffi::prop_name(prop).and_then(ffi::initial_value);
        return Ok(match initial {
            Some(initial) => scope.string(initial),
            None => Value::null(),
        });
    };
    Ok(scope.string(&ffi::serialize_value(&value)))
}

fn number_or(scope: &mut Scope<'_>, value: &Value, fallback: f64) -> f64 {
    scope.to_number(value).unwrap_or(fallback)
}

fn string_option(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    if value.is_string() {
        scope.to_string(value).ok()
    } else {
        None
    }
}

pub(crate) fn animate(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let (Some(anim), [node, stops, timing, ..]) = (ffi::anim(js), args) else {
        return Ok(Value::null());
    };
    let Some(node) = ffi::unwrap_element(node) else {
        return Ok(Value::null());
    };
    if !stops.is_object() || !timing.is_object() {
        return Ok(Value::null());
    }
    ffi::flush_style(js);
    let length = scope.get(stops, "length")?;
    let n = scope.to_int32(&length).unwrap_or(0).clamp(0, 256);
    let mut list = Vec::with_capacity(n as usize);
    for i in 0..n {
        let stop = scope.get_index(stops, i as u32)?;
        let css = scope.get(&stop, "css")?;
        let offset = scope.get(&stop, "offset")?;
        let css = scope.to_string(&css).ok();
        let offset = number_or(scope, &offset, 0.0);
        list.push((css, offset));
    }
    let duration = scope.get(timing, "duration")?;
    let delay = scope.get(timing, "delay")?;
    let iterations = scope.get(timing, "iterations")?;
    let direction = scope.get(timing, "direction")?;
    let fill = scope.get(timing, "fill")?;
    let easing = scope.get(timing, "easing")?;
    let iterations = number_or(scope, &iterations, 1.0);
    let direction = string_option(scope, &direction);
    let fill = string_option(scope, &fill);
    let easing = string_option(scope, &easing);
    let t = AnimTiming {
        duration_ms: number_or(scope, &duration, 0.0),
        delay_ms: number_or(scope, &delay, 0.0),
        iterations: if iterations.is_nan() { 1.0 } else { iterations },
        direction: direction.as_deref(),
        fill: fill.as_deref(),
        easing: easing.as_deref(),
    };
    let stops: Vec<(Option<&str>, f64)> = list
        .iter()
        .map(|(css, offset)| (css.as_deref(), *offset))
        .collect();
    let Some((prop, generation)) = anim.script_start(node, &stops, &t) else {
        return Ok(Value::null());
    };
    ffi::mark_mutated(js);
    let o = scope.new_object();
    scope.set(&o, "run", Value::int(-1 - prop))?;
    scope.set(&o, "generation", Value::int(generation as i32))?;
    Ok(o)
}
