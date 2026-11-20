//! Southstar — AudioParam automation: the event list each param keeps, its value at a render block's start time, and the setValueAtTime/ramp/target/curve/cancel methods.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
//!
//! Events live in `param._ev`, a Float64Array of `[kind, value, time, timeConstant]`
//! quads sorted by time; `param._base` is the value before the first event.
//! While events are pending the renderer writes the computed value to
//! `param.value` and remembers it in `param._shown`: a script assignment to
//! `value` (which no longer matches `_shown`) cancels the automation, as
//! setting `value` does in browsers.

use southstar_js_engine::props::{get, set};
use southstar_js_engine::{ElementType, Scope, Value};

const MAX_EVENTS: usize = 4096;
const MAX_CURVE_STEPS: usize = 64;

const SET: u8 = 0;
const LINEAR: u8 = 1;
const EXPO: u8 = 2;
const TARGET: u8 = 3;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub kind: u8,
    pub value: f64,
    pub time: f64,
    pub time_constant: f64,
}

impl Event {
    fn set(value: f64, time: f64) -> Event {
        Event {
            kind: SET,
            value,
            time,
            time_constant: 0.0,
        }
    }
}

/// Inserts an event after any existing events at the same time.
pub fn insert(events: &mut Vec<Event>, event: Event) {
    if events.len() >= MAX_EVENTS {
        return;
    }
    let at = events
        .iter()
        .rposition(|e| e.time <= event.time)
        .map_or(0, |i| i + 1);
    events.insert(at, event);
}

/// The automation's value at time `t`. With `prune`, drops the events that no
/// longer matter (render times only move forward); an empty list afterwards
/// means the param holds a plain value again.
pub fn evaluate(events: &mut Vec<Event>, base: f64, t: f64, prune: bool) -> f64 {
    let n = events.len();
    let (mut pv, mut pt) = (base, 0.0);
    // (target value, time constant, start time, value at start)
    let mut target: Option<(f64, f64, f64, f64)> = None;
    let mut settled = None;
    let mut v = base;
    let mut i = 0;
    while i < n {
        let e = events[i];
        let ramp = e.kind == LINEAR || e.kind == EXPO;
        if let Some((tv, tc, tt, tv0)) = target.take()
            && !ramp
        {
            let until = e.time.min(t);
            pv = tv + (tv0 - tv) * (-(until - tt) / tc).exp();
            pt = until;
        }
        if e.time > t {
            let span = e.time - pt;
            let frac = if span > 0.0 {
                ((t - pt) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            v = match e.kind {
                LINEAR => pv + (e.value - pv) * frac,
                EXPO if pv * e.value > 0.0 => pv * (e.value / pv).powf(frac),
                _ => pv,
            };
            break;
        }
        if e.kind == TARGET {
            let tc = if e.time_constant > 0.0 {
                e.time_constant
            } else {
                1e-9
            };
            target = Some((e.value, tc, e.time, pv));
        } else {
            pv = e.value;
            settled = Some(i);
        }
        pt = e.time;
        i += 1;
    }
    if i == n {
        v = match target {
            Some((tv, tc, tt, tv0)) => tv + (tv0 - tv) * (-(t - tt) / tc).exp(),
            None => pv,
        };
        if prune {
            match target {
                None => {
                    events.clear();
                    return v;
                }
                Some((tv, tc, tt, _)) if t - tt > 10.0 * tc => {
                    events.clear();
                    return tv;
                }
                Some(_) => {}
            }
        }
    }
    if prune && let Some(settle) = settled {
        events.drain(..settle);
        events[0].kind = SET;
    }
    v
}

/// Removes the events at or after `time`; with `hold`, keeps the value reached there.
pub fn cancel(events: &mut Vec<Event>, base: f64, time: f64, hold: bool) {
    let held = hold.then(|| evaluate(&mut events.clone(), base, time, false));
    events.retain(|e| e.time < time);
    if let Some(held) = held.filter(|v| v.is_finite()) {
        insert(events, Event::set(held, time));
    }
}

fn number(scope: &mut Scope<'_>, object: &Value, key: &str, default: f64) -> f64 {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        return default;
    }
    scope.to_number(&value).unwrap_or(default)
}

fn load(scope: &mut Scope<'_>, param: &Value) -> Vec<Event> {
    let array = get(scope, param, "_ev");
    if !array.is_object() || scope.typed_array_element(&array) != Some(ElementType::Float64) {
        return Vec::new();
    }
    scope
        .with_typed_array(&array, |view| {
            view.bytes
                .as_chunks::<32>()
                .0
                .iter()
                .map(|quad| {
                    let at = |k: usize| {
                        let mut b = [0u8; 8];
                        b.copy_from_slice(&quad[k * 8..k * 8 + 8]);
                        f64::from_ne_bytes(b)
                    };
                    Event {
                        kind: at(0) as u8,
                        value: at(1),
                        time: at(2),
                        time_constant: at(3),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn store(scope: &mut Scope<'_>, param: &Value, events: &[Event]) {
    if events.is_empty() {
        set(scope, param, "_ev", Value::null());
        return;
    }
    let global = scope.global();
    let constructor = get(scope, &global, "Float64Array");
    let Ok(array) = scope.construct(&constructor, &[Value::number((events.len() * 4) as f64)])
    else {
        return;
    };
    let bytes: Vec<u8> = events
        .iter()
        .flat_map(|e| [f64::from(e.kind), e.value, e.time, e.time_constant])
        .flat_map(f64::to_ne_bytes)
        .collect();
    scope.with_buffer_bytes_mut(&array, |out| {
        let n = out.len().min(bytes.len());
        out[..n].copy_from_slice(&bytes[..n]);
    });
    set(scope, param, "_ev", array);
}

/// Reads the pending events, dropping them if script assigned `value` meanwhile.
fn sync(scope: &mut Scope<'_>, param: &Value) -> Vec<Event> {
    let events = load(scope, param);
    if events.is_empty() {
        return events;
    }
    let shown = number(scope, param, "_shown", f64::NAN);
    let value = number(scope, param, "value", f64::NAN);
    if !shown.is_nan() && value != shown {
        set(scope, param, "_ev", Value::null());
        set(scope, param, "_shown", Value::undefined());
        return Vec::new();
    }
    events
}

/// Reads `node[name]` as an AudioParam's value at time `t`, applying its automation.
pub fn param_at(scope: &mut Scope<'_>, node: &Value, name: &str, default: f64, t: f64) -> f64 {
    let param = get(scope, node, name);
    if !param.is_object() {
        return default;
    }
    let value = number(scope, &param, "value", default);
    let mut events = sync(scope, &param);
    if events.is_empty() {
        return value;
    }
    let before = events.clone();
    let base = number(scope, &param, "_base", value);
    let mut v = evaluate(&mut events, base, t, true);
    if !v.is_finite() {
        v = default;
    }
    if events != before {
        store(scope, &param, &events);
    }
    set(scope, &param, "value", Value::number(v));
    let shown = if events.is_empty() {
        Value::undefined()
    } else {
        Value::number(v)
    };
    set(scope, &param, "_shown", shown);
    v
}

fn add(scope: &mut Scope<'_>, param: &Value, event: Event) {
    let mut events = sync(scope, param);
    if events.is_empty() {
        let current = number(scope, param, "value", 0.0);
        set(scope, param, "_base", Value::number(current));
        // A ramp or target with nothing before it starts now, from the
        // current value, not from time zero.
        if event.kind != SET {
            let context = get(scope, param, "_ac");
            let now = if context.is_object() {
                number(scope, &context, "currentTime", 0.0)
            } else {
                0.0
            };
            if now.is_finite() && now > 0.0 && now < event.time {
                insert(&mut events, Event::set(current, now));
            }
        }
    }
    insert(&mut events, event);
    store(scope, param, &events);
    let shown = number(scope, param, "value", 0.0);
    set(scope, param, "_shown", Value::number(shown));
}

fn finite_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Result<f64, Value> {
    let value = match args.get(index) {
        Some(value) => scope.to_number(value)?,
        None => f64::NAN,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(scope.type_error(&format!(
            "AudioParam: argument {} is not a finite number",
            index + 1
        )))
    }
}

fn curve(scope: &mut Scope<'_>, param: &Value, args: &[Value]) -> Result<(), Value> {
    let time = finite_arg(scope, args, 1)?;
    let duration = finite_arg(scope, args, 2)?;
    if time < 0.0 || duration <= 0.0 {
        return Err(scope.range_error("setValueCurveAtTime: bad start time or duration"));
    }
    let values = args.first().cloned().unwrap_or_else(Value::undefined);
    let length = if values.is_object() {
        let length = get(scope, &values, "length");
        scope.to_number(&length).unwrap_or(0.0)
    } else {
        0.0
    };
    let length = if length.is_finite() && length > 0.0 {
        length.min(f64::from(u32::MAX)) as usize
    } else {
        0
    };
    if length < 2 {
        return Err(scope.type_error("setValueCurveAtTime: curve needs at least two values"));
    }
    // Sample the curve into at most 64 linear segments.
    let steps = (length - 1).min(MAX_CURVE_STEPS);
    for step in 0..=steps {
        let k = step * (length - 1) / steps;
        let item = scope.get_index(&values, k as u32)?;
        let mut v = scope.to_number(&item)?;
        if !v.is_finite() {
            v = 0.0;
        }
        let kind = if step == 0 { SET } else { LINEAR };
        add(
            scope,
            param,
            Event {
                kind,
                value: v,
                time: time + duration * step as f64 / steps as f64,
                time_constant: 0.0,
            },
        );
    }
    Ok(())
}

/// The AudioParam automation methods; `which` indexes setValueAtTime,
/// linearRampToValueAtTime, exponentialRampToValueAtTime, setTargetAtTime,
/// setValueCurveAtTime, cancelScheduledValues and cancelAndHoldAtTime. Each returns `this`.
pub fn param_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    which: i32,
) -> Result<Value, Value> {
    match which {
        0..=2 => {
            let value = finite_arg(scope, args, 0)?;
            let time = finite_arg(scope, args, 1)?;
            if time < 0.0 {
                return Err(scope.range_error("AudioParam: time must not be negative"));
            }
            if which == 2 && value == 0.0 {
                return Err(
                    scope.range_error("exponentialRampToValueAtTime: value must not be zero")
                );
            }
            let kind = [SET, LINEAR, EXPO][which as usize];
            add(
                scope,
                this,
                Event {
                    kind,
                    value,
                    time,
                    time_constant: 0.0,
                },
            );
        }
        3 => {
            let value = finite_arg(scope, args, 0)?;
            let time = finite_arg(scope, args, 1)?;
            let time_constant = finite_arg(scope, args, 2)?;
            if time < 0.0 || time_constant < 0.0 {
                return Err(scope
                    .range_error("setTargetAtTime: time and timeConstant must not be negative"));
            }
            let event = if time_constant == 0.0 {
                Event::set(value, time)
            } else {
                Event {
                    kind: TARGET,
                    value,
                    time,
                    time_constant,
                }
            };
            add(scope, this, event);
        }
        4 => curve(scope, this, args)?,
        5 | 6 => {
            let time = finite_arg(scope, args, 0)?;
            if time < 0.0 {
                return Err(scope.range_error("AudioParam: time must not be negative"));
            }
            let mut events = sync(scope, this);
            if !events.is_empty() {
                let base = number(scope, this, "_base", 0.0);
                cancel(&mut events, base, time, which == 6);
                store(scope, this, &events);
            }
        }
        _ => {}
    }
    Ok(this.clone())
}
