//! Southstar — CanvasGradient and CanvasPattern: their creation, addColorStop, and turning them into cairo patterns when a style uses them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::Pattern;
use crate::raster::ConicStop;

type Result = core::result::Result<Value, Value>;

const EXTEND_NONE: i32 = 0;
const EXTEND_REPEAT: i32 = 1;

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn hnum(scope: &mut Scope<'_>, obj: &Value, name: &str) -> f64 {
    crate::hidden::get(scope, obj, name)
        .ok()
        .and_then(|v| scope.to_number(&v).ok())
        .unwrap_or(f64::NAN)
}

fn field(scope: &mut Scope<'_>, obj: &Value, name: &str) -> f64 {
    scope
        .get(obj, name)
        .ok()
        .and_then(|v| scope.to_number(&v).ok())
        .unwrap_or(f64::NAN)
}

fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    scope
        .get(array, "length")
        .ok()
        .and_then(|l| scope.to_int32(&l).ok())
        .map_or(0, |n| n as u32)
}

fn stops(scope: &mut Scope<'_>, obj: &Value) -> Vec<ConicStop> {
    let Ok(stops) = crate::hidden::get(scope, obj, "_stops") else {
        return Vec::new();
    };
    if !stops.is_array() {
        return Vec::new();
    }
    let n = length(scope, &stops);
    let mut out = Vec::new();
    for i in 0..n {
        let Ok(stop) = scope.get_index(&stops, i) else {
            continue;
        };
        if !stop.is_object() {
            continue;
        }
        out.push(ConicStop {
            pos: field(scope, &stop, "pos"),
            r: field(scope, &stop, "r"),
            g: field(scope, &stop, "g"),
            b: field(scope, &stop, "b"),
            a: field(scope, &stop, "a"),
        });
    }
    out
}

fn conic(scope: &mut Scope<'_>, obj: &Value) -> Option<Pattern> {
    let cx = hnum(scope, obj, "_x0");
    let cy = hnum(scope, obj, "_y0");
    let angle = hnum(scope, obj, "_angle");
    let mut stops = stops(scope, obj);
    if stops.is_empty() {
        return None;
    }
    crate::raster::sort_stops(&mut stops);
    let sectors = crate::raster::conic_sectors((cx, cy), angle, &stops);
    Some(Pattern::conic((cx, cy), &sectors))
}

fn image_pattern(scope: &mut Scope<'_>, obj: &Value, origin_clean: &mut bool) -> Option<Pattern> {
    let node = crate::hidden::get(scope, obj, "_node").unwrap_or_else(|_| Value::undefined());
    let source = crate::ffi::drawimage_source(scope, &node)?;
    *origin_clean = source.origin_clean;
    let pattern = Pattern::for_surface(&source.surface);
    drop(source);
    let rep = crate::hidden::get(scope, obj, "_rep").unwrap_or_else(|_| Value::undefined());
    let extend = if rep.is_string() {
        match scope.to_bytes(&rep).map(c_text).as_deref() {
            Ok(b"no-repeat") => EXTEND_NONE,
            _ => EXTEND_REPEAT,
        }
    } else {
        EXTEND_REPEAT
    };
    pattern.set_extend(extend);
    let matrix = crate::hidden::get(scope, obj, "_matrix").unwrap_or_else(|_| Value::undefined());
    if matrix.is_array() {
        let mut m = [0.0; 6];
        for (i, slot) in m.iter_mut().enumerate() {
            *slot = scope
                .get_index(&matrix, i as u32)
                .ok()
                .and_then(|v| scope.to_number(&v).ok())
                .unwrap_or(f64::NAN);
        }
        pattern.set_inverse_matrix(m);
    }
    Some(pattern)
}

pub(crate) fn build_pattern(scope: &mut Scope<'_>, obj: &Value) -> (*mut c_void, bool) {
    let mut origin_clean = true;
    if !obj.is_object() {
        return (core::ptr::null_mut(), origin_clean);
    }
    let kind = crate::hidden::get(scope, obj, "_type").unwrap_or_else(|_| Value::undefined());
    if !kind.is_string() {
        return (core::ptr::null_mut(), origin_clean);
    }
    let Ok(kind) = scope.to_bytes(&kind).map(c_text) else {
        return (core::ptr::null_mut(), origin_clean);
    };
    let pattern = match kind.as_slice() {
        b"linear" => {
            let x0 = hnum(scope, obj, "_x0");
            let y0 = hnum(scope, obj, "_y0");
            let x1 = hnum(scope, obj, "_x1");
            let y1 = hnum(scope, obj, "_y1");
            Pattern::linear((x0, y0), (x1, y1))
        }
        b"radial" => {
            let x0 = hnum(scope, obj, "_x0");
            let y0 = hnum(scope, obj, "_y0");
            let r0 = hnum(scope, obj, "_r0");
            let x1 = hnum(scope, obj, "_x1");
            let y1 = hnum(scope, obj, "_y1");
            let r1 = hnum(scope, obj, "_r1");
            Pattern::radial((x0, y0, r0), (x1, y1, r1))
        }
        b"pattern" => {
            let pattern = image_pattern(scope, obj, &mut origin_clean);
            return (
                pattern.map_or(core::ptr::null_mut(), Pattern::into_raw),
                origin_clean,
            );
        }
        b"conic" => {
            let pattern = conic(scope, obj);
            return (
                pattern.map_or(core::ptr::null_mut(), Pattern::into_raw),
                origin_clean,
            );
        }
        _ => return (core::ptr::null_mut(), origin_clean),
    };
    for stop in stops(scope, obj) {
        pattern.add_color_stop(stop.pos, [stop.r, stop.g, stop.b, stop.a]);
    }
    (pattern.into_raw(), origin_clean)
}

pub(crate) fn add_color_stop(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let pos = arg(scope, args, 0);
    if !(0.0..=1.0).contains(&pos) {
        return Err(crate::api::throw_dom(
            scope,
            "IndexSizeError",
            "addColorStop offset must be in the range [0, 1]",
        ));
    }
    let color = scope
        .to_bytes(&args[1])
        .ok()
        .map(c_text)
        .and_then(|c| crate::color::parse(&c));
    let Some([r, g, b, a]) = color else {
        return Err(crate::api::throw_dom(
            scope,
            "SyntaxError",
            "addColorStop color could not be parsed",
        ));
    };
    let mut stops = crate::hidden::get(scope, this, "_stops")?;
    if !stops.is_array() {
        stops = scope.new_array();
        crate::hidden::set(scope, this, "_stops", stops.clone());
    }
    let n = length(scope, &stops);
    let entry = scope.new_object();
    for (key, v) in [("pos", pos), ("r", r), ("g", g), ("b", b), ("a", a)] {
        let _ = scope.set(&entry, key, Value::number(v));
    }
    let _ = scope.set_index(&stops, n, entry);
    Ok(Value::undefined())
}

pub(crate) fn create_pattern(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(source) = args.first().filter(|v| v.is_object()) else {
        return Ok(Value::null());
    };
    if crate::ffi::drawimage_source(scope, source).is_none() {
        return Ok(Value::null());
    }
    let repetition = args
        .get(1)
        .filter(|v| v.is_string())
        .and_then(|v| scope.to_bytes(v).ok())
        .map(c_text)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| b"repeat".to_vec());
    Ok(crate::ffi::new_pattern(scope, this, source, &repetition))
}

fn store(scope: &mut Scope<'_>, obj: &Value, name: &str, v: f64) {
    crate::hidden::set(scope, obj, name, Value::number(v));
}

pub(crate) fn create_linear_gradient(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result {
    if args.len() < 4 {
        return Ok(Value::null());
    }
    let obj = crate::ffi::new_gradient(scope, this, b"linear");
    for (i, name) in ["_x0", "_y0", "_x1", "_y1"].into_iter().enumerate() {
        let v = arg(scope, args, i);
        store(scope, &obj, name, v);
    }
    Ok(obj)
}

pub(crate) fn create_radial_gradient(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result {
    if args.len() < 6 {
        return Ok(Value::null());
    }
    let r0 = arg(scope, args, 2);
    let r1 = arg(scope, args, 5);
    if r0 < 0.0 || r1 < 0.0 {
        return Err(crate::api::throw_dom(
            scope,
            "IndexSizeError",
            "createRadialGradient radius must not be negative",
        ));
    }
    let obj = crate::ffi::new_gradient(scope, this, b"radial");
    let x0 = arg(scope, args, 0);
    store(scope, &obj, "_x0", x0);
    let y0 = arg(scope, args, 1);
    store(scope, &obj, "_y0", y0);
    store(scope, &obj, "_r0", r0);
    let x1 = arg(scope, args, 3);
    store(scope, &obj, "_x1", x1);
    let y1 = arg(scope, args, 4);
    store(scope, &obj, "_y1", y1);
    store(scope, &obj, "_r1", r1);
    Ok(obj)
}

pub(crate) fn create_conic_gradient(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if args.len() < 3 {
        return Ok(Value::null());
    }
    let obj = crate::ffi::new_gradient(scope, this, b"conic");
    for (i, name) in ["_angle", "_x0", "_y0"].into_iter().enumerate() {
        let v = arg(scope, args, i);
        store(scope, &obj, name, v);
    }
    Ok(obj)
}
