//! Southstar — WebGL argument conversion: numbers, GL object names and float or int sequences out of JS values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{ElementType, Scope, Value};

use crate::context::{WebGl, active, reassert};
use crate::ffi::host;

pub(crate) const KIND_SYNC: u8 = 8;
pub(crate) const KIND_LOCATION: u8 = 11;

pub(crate) type JsResult = Result<Value, Value>;

pub(crate) fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn int(s: &mut Scope<'_>, args: &[Value], i: usize) -> i32 {
    let Some(v) = args.get(i) else {
        return 0;
    };
    let keep = active();
    let n = s.to_int32(v).unwrap_or(0);
    if v.is_object() {
        reassert(&keep);
    }
    n
}

pub(crate) fn uint(s: &mut Scope<'_>, args: &[Value], i: usize) -> u32 {
    int(s, args, i) as u32
}

pub(crate) fn num(s: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    let Some(v) = args.get(i) else {
        return 0.0;
    };
    let keep = active();
    let n = s.to_number(v).unwrap_or(f64::NAN);
    if v.is_object() {
        reassert(&keep);
    }
    n
}

pub(crate) fn float(s: &mut Scope<'_>, args: &[Value], i: usize) -> f32 {
    num(s, args, i) as f32
}

pub(crate) fn boolean(s: &mut Scope<'_>, args: &[Value], i: usize) -> bool {
    args.get(i).is_some_and(|v| s.to_bool(v))
}

pub(crate) fn bytes(s: &mut Scope<'_>, v: &Value) -> Option<Vec<u8>> {
    s.to_bytes(v).ok()
}

pub(crate) fn name(args: &[Value], i: usize) -> u32 {
    args.get(i).and_then(host::object_of).map_or(0, |(_, n)| n)
}

pub(crate) fn location(args: &[Value], i: usize) -> i32 {
    match args.get(i).and_then(host::object_of) {
        Some((KIND_LOCATION, n)) => n as i32,
        _ => -1,
    }
}

pub(crate) fn sync_id(v: &Value) -> Option<i32> {
    match host::object_of(v) {
        Some((KIND_SYNC, n)) => Some(n as i32),
        _ => None,
    }
}

pub(crate) fn context(s: &mut Scope<'_>, this: &Value) -> Result<Rc<WebGl>, Value> {
    host::context_of(this).ok_or_else(|| s.type_error("Illegal invocation"))
}

pub(crate) fn current(s: &mut Scope<'_>, this: &Value) -> Result<Rc<WebGl>, Value> {
    let g = context(s, this)?;
    g.enter();
    Ok(g)
}

fn sequence_len(s: &mut Scope<'_>, v: &Value) -> i32 {
    match s.get(v, "length") {
        Ok(len) => s.to_int32(&len).unwrap_or(0),
        Err(_) => 0,
    }
}

pub(crate) fn with_view<R>(
    s: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(Option<&mut [u8]>) -> R,
) -> R {
    let mut f = Some(f);
    let result = s.with_buffer_bytes_mut(value, |bytes| f.take().map(|f| f(Some(bytes))));
    match (result.flatten(), f) {
        (Some(result), _) => result,
        (None, Some(f)) => f(None),
        (None, None) => unreachable!(),
    }
}

fn typed_words(s: &mut Scope<'_>, v: &Value, max: usize) -> Vec<[u8; 4]> {
    s.with_buffer_bytes_mut(v, |b| {
        b.as_chunks::<4>().0.iter().take(max).copied().collect()
    })
    .unwrap_or_default()
}

pub(crate) fn floats(s: &mut Scope<'_>, v: &Value, out: &mut [f32]) -> i32 {
    if s.typed_array_element(v) == Some(ElementType::Float32) {
        let words = typed_words(s, v, out.len());
        for (o, w) in out.iter_mut().zip(&words) {
            *o = f32::from_ne_bytes(*w);
        }
        return words.len() as i32;
    }
    let keep = active();
    let count = sequence_len(s, v).min(out.len() as i32);
    for i in 0..count.max(0) {
        let d = match s.get_index(v, i as u32) {
            Ok(e) => s.to_number(&e).unwrap_or(f64::NAN),
            Err(_) => f64::NAN,
        };
        out[i as usize] = d as f32;
    }
    reassert(&keep);
    count
}

pub(crate) fn ints(s: &mut Scope<'_>, v: &Value, out: &mut [i32]) -> i32 {
    let kind = s.typed_array_element(v);
    if kind == Some(ElementType::Int32) || kind == Some(ElementType::Uint32) {
        let words = typed_words(s, v, out.len());
        for (o, w) in out.iter_mut().zip(&words) {
            *o = i32::from_ne_bytes(*w);
        }
        return words.len() as i32;
    }
    let keep = active();
    let count = sequence_len(s, v).min(out.len() as i32);
    for i in 0..count.max(0) {
        let d = match s.get_index(v, i as u32) {
            Ok(e) => s.to_int32(&e).unwrap_or(0),
            Err(_) => 0,
        };
        out[i as usize] = d;
    }
    reassert(&keep);
    count
}

pub(crate) fn uints(s: &mut Scope<'_>, v: &Value, out: &mut [u32]) -> i32 {
    let mut signed = vec![0i32; out.len()];
    let n = ints(s, v, &mut signed);
    for (o, i) in out.iter_mut().zip(signed).take(n.max(0) as usize) {
        *o = i as u32;
    }
    n
}

pub(crate) fn string_list(s: &mut Scope<'_>, v: &Value, n: u32) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| match s.get_index(v, i) {
            Ok(e) => s.to_bytes(&e).unwrap_or_default(),
            Err(_) => Vec::new(),
        })
        .collect()
}

pub(crate) fn list_len(s: &mut Scope<'_>, v: &Value) -> u32 {
    sequence_len(s, v) as u32
}
