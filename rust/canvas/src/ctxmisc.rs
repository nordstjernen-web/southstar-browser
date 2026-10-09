//! Southstar — the 2D context's clip, line dash, hit tests, reset and createImageData.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::draw::{prepare_path_and_rule, restore_path, state};
use crate::ffi::cairo::Context;

type Result = core::result::Result<Value, Value>;

const OPERATOR_CLEAR: i32 = 0;
const LINE_CAP_BUTT: i32 = 0;
const LINE_JOIN_MITER: i32 = 0;
const DEFAULT_MITER_LIMIT: f64 = 10.0;
const IMAGEDATA_MAX: i64 = 32767;

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    scope
        .get(array, "length")
        .ok()
        .and_then(|l| scope.to_int32(&l).ok())
        .map_or(0, |n| n as u32)
}

fn cairo_of(st: &crate::ffi::state::CanvasState) -> Option<Context> {
    unsafe { Context::from_raw(st.cr) }
}

pub(crate) fn clip(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(cr) = state(scope, this).and_then(|st| cairo_of(st)) else {
        return Ok(Value::undefined());
    };
    let saved = prepare_path_and_rule(scope, cr, args);
    cr.clip_preserve();
    restore_path(cr, saved);
    Ok(Value::undefined())
}

pub(crate) fn set_line_dash(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if state(scope, this).is_none() {
        return Ok(Value::undefined());
    }
    let stored = scope.new_array();
    if let Some(dashes) = args.first().filter(|v| v.is_array()) {
        let n = length(scope, dashes);
        let passes = if n % 2 == 1 { 2 } else { 1 };
        let mut out = 0;
        for _ in 0..passes {
            for i in 0..n {
                let d = scope
                    .get_index(dashes, i)
                    .ok()
                    .and_then(|e| scope.to_number(&e).ok())
                    .unwrap_or(f64::NAN);
                let d = if !d.is_finite() || d < 0.0 { 0.0 } else { d };
                let _ = scope.set_index(&stored, out, Value::number(d));
                out += 1;
            }
        }
    }
    crate::hidden::set(scope, this, "_dashes", stored);
    Ok(Value::undefined())
}

pub(crate) fn get_line_dash(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    let current = crate::hidden::get(scope, this, "_dashes")?;
    let out = scope.new_array();
    if !current.is_array() {
        return Ok(out);
    }
    let n = length(scope, &current);
    for i in 0..n {
        let e = scope
            .get_index(&current, i)
            .unwrap_or_else(|_| Value::undefined());
        let _ = scope.set_index(&out, i, e);
    }
    Ok(out)
}

fn replay_path(cr: Context, path: &Value) -> crate::ffi::cairo::Path {
    let saved = cr.copy_path();
    if let Some(src) = crate::ffi::path2d_context(path) {
        let copy = src.copy_path();
        cr.new_path();
        cr.append_path(&copy);
    }
    saved
}

pub(crate) fn is_point_in_path(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::boolean(false));
    };
    let Some(cr) = cairo_of(st).filter(|_| args.len() >= 2) else {
        return Ok(Value::boolean(false));
    };
    let mut i = 0;
    let mut saved = None;
    if crate::ffi::is_path2d(&args[0]) {
        if args.len() < 3 {
            return Ok(Value::boolean(false));
        }
        saved = Some(replay_path(cr, &args[0]));
        i = 1;
    }
    let x = arg(scope, args, i);
    let y = arg(scope, args, i + 1);
    let rule = args
        .get(i + 2)
        .filter(|v| v.is_string())
        .and_then(|v| scope.to_bytes(v).ok())
        .map(|mut b| {
            if let Some(nul) = b.iter().position(|&c| c == 0) {
                b.truncate(nul);
            }
            b
        });
    let previous = cr.fill_rule();
    cr.set_fill_rule(crate::raster::fill_rule(rule.as_deref()));
    let inside = cr.in_fill(x, y);
    cr.set_fill_rule(previous);
    restore_path(cr, saved);
    Ok(Value::boolean(inside))
}

pub(crate) fn is_point_in_stroke(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::boolean(false));
    };
    if args.len() < 2 {
        return Ok(Value::boolean(false));
    }
    crate::style::sync(scope, this, st);
    let Some(cr) = cairo_of(st) else {
        return Ok(Value::boolean(false));
    };
    let mut i = 0;
    let mut saved = None;
    if crate::ffi::is_path2d(&args[0]) {
        if args.len() < 3 {
            return Ok(Value::boolean(false));
        }
        saved = Some(replay_path(cr, &args[0]));
        i = 1;
    }
    let x = arg(scope, args, i);
    let y = arg(scope, args, i + 1);
    cr.set_line_width(st.line_width);
    let inside = cr.in_stroke(x, y);
    restore_path(cr, saved);
    Ok(Value::boolean(inside))
}

pub(crate) fn reset(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    if let Some(cr) = cairo_of(st) {
        cr.save();
        cr.identity_matrix();
        cr.set_operator(OPERATOR_CLEAR);
        cr.paint();
        cr.restore();
        cr.identity_matrix();
        cr.new_path();
        cr.reset_clip();
        cr.set_dash(&[], 0.0);
        cr.set_line_width(1.0);
        cr.set_line_cap(LINE_CAP_BUTT);
        cr.set_line_join(LINE_JOIN_MITER);
        cr.set_miter_limit(DEFAULT_MITER_LIMIT);
    }
    crate::api::ctx2d_init_state(scope, this);
    crate::ffi::state::set_font(st, b"10px sans-serif");
    st.fill = [0.0, 0.0, 0.0, 1.0];
    st.stroke = [0.0, 0.0, 0.0, 1.0];
    st.line_width = 1.0;
    crate::ffi::state::clear_patterns(st);
    st.shadow = [0.0; 4];
    st.shadow_blur = 0.0;
    st.shadow_ox = 0.0;
    st.shadow_oy = 0.0;
    crate::ffi::mark_mutated(scope);
    Ok(Value::undefined())
}

pub(crate) fn create_image_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let first = args.first().cloned().unwrap_or_else(Value::undefined);
    let (w, h) = if first.is_object() && !first.is_number() {
        let wv = scope
            .get(&first, "width")
            .unwrap_or_else(|_| Value::undefined());
        let hv = scope
            .get(&first, "height")
            .unwrap_or_else(|_| Value::undefined());
        (
            scope.to_int32(&wv).unwrap_or(0),
            scope.to_int32(&hv).unwrap_or(0),
        )
    } else if args.len() >= 2 {
        (
            scope.to_int32(&args[0]).unwrap_or(0),
            scope.to_int32(&args[1]).unwrap_or(0),
        )
    } else {
        return Err(scope.type_error(
            "Failed to execute 'createImageData' on 'CanvasRenderingContext2D': 2 arguments \
             required, but only 1 present.",
        ));
    };
    let aw = i64::from(w).abs();
    let ah = i64::from(h).abs();
    if aw == 0 || ah == 0 {
        let message = if aw == 0 {
            "The source width is zero or not a number."
        } else {
            "The source height is zero or not a number."
        };
        return Err(crate::api::throw_dom(scope, "IndexSizeError", message));
    }
    if aw > IMAGEDATA_MAX || ah > IMAGEDATA_MAX {
        return Err(scope.range_error("ImageData too large"));
    }
    crate::ffi::new_imagedata(scope, this, (aw as i32, ah as i32))
}
