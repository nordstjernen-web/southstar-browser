//! Southstar — the 2D context's rectangle and path fills and strokes, shadow compositing, and save/restore of its attributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::cairo::{Context, OwnedContext, Path, Surface};
use crate::ffi::state::CanvasState;

type Result = core::result::Result<Value, Value>;

const OPERATOR_CLEAR: i32 = 0;
const OPERATOR_OVER: i32 = 2;
const MAX_SHADOW_RADIUS: i32 = 64;

const SAVABLE: [&str; 27] = [
    "fillStyle",
    "strokeStyle",
    "font",
    "textAlign",
    "textBaseline",
    "direction",
    "globalAlpha",
    "globalCompositeOperation",
    "shadowColor",
    "shadowBlur",
    "shadowOffsetX",
    "shadowOffsetY",
    "imageSmoothingEnabled",
    "imageSmoothingQuality",
    "lineWidth",
    "lineCap",
    "lineJoin",
    "miterLimit",
    "lineDashOffset",
    "filter",
    "letterSpacing",
    "wordSpacing",
    "fontKerning",
    "fontStretch",
    "fontVariantCaps",
    "textRendering",
    "_dashes",
];

pub(crate) fn state(scope: &mut Scope<'_>, this: &Value) -> Option<&'static mut CanvasState> {
    crate::ffi::ctx_state(scope, this)
}

fn main_context(st: &CanvasState) -> Option<Context> {
    unsafe { Context::from_raw(st.cr) }
}

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

pub(crate) fn with_shadow(st: &CanvasState, mut draw: impl FnMut(Context)) {
    let Some(cr) = main_context(st) else {
        return;
    };
    if !crate::style::has_shadow(st) || st.w <= 0 || st.h <= 0 {
        draw(cr);
        return;
    }
    let off = Surface::image(st.w, st.h);
    if !off.is_ok() {
        draw(cr);
        return;
    }
    {
        let ocr = OwnedContext::on(&off);
        ocr.context().set_matrix(cr.matrix());
        draw(ocr.context());
    }
    let alpha = st.shadow[3];
    let [r, g, b] = [st.shadow[0], st.shadow[1], st.shadow[2]];
    let (sw, sh) = off.size();
    let half_blur = st.shadow_blur * 0.5 + 0.5;
    let radius = if half_blur < f64::from(MAX_SHADOW_RADIUS) {
        half_blur as i32
    } else {
        MAX_SHADOW_RADIUS
    };
    off.write_pixels(|data, stride| {
        for y in 0..sh as usize {
            let row = &mut data[y * stride..y * stride + sw as usize * 4];
            for px in row.chunks_exact_mut(4) {
                let na = (f64::from(px[3]) * alpha) as u8;
                px[0] = (b * f64::from(na)) as u8;
                px[1] = (g * f64::from(na)) as u8;
                px[2] = (r * f64::from(na)) as u8;
                px[3] = na;
            }
        }
        if radius > 0 {
            crate::raster::box_blur(data, sw, sh, stride as i32, radius);
        }
    });
    cr.save();
    cr.identity_matrix();
    cr.set_source_surface(&off, st.shadow_ox, st.shadow_oy);
    cr.paint();
    cr.restore();
    drop(off);
    draw(cr);
}

pub(crate) fn set_fill_source(scope: &mut Scope<'_>, this: &Value, st: &CanvasState) {
    let ga = crate::style::global_alpha(scope, this);
    let Some(cr) = main_context(st) else {
        return;
    };
    if st.fill_pattern.is_null() {
        let [r, g, b, a] = st.fill;
        cr.set_source_rgba([r, g, b, a * ga]);
    } else {
        cr.set_source_pattern(st.fill_pattern);
    }
}

pub(crate) fn set_stroke_source(scope: &mut Scope<'_>, this: &Value, st: &CanvasState) {
    let ga = crate::style::global_alpha(scope, this);
    let Some(cr) = main_context(st) else {
        return;
    };
    if st.stroke_pattern.is_null() {
        let [r, g, b, a] = st.stroke;
        cr.set_source_rgba([r, g, b, a * ga]);
    } else {
        cr.set_source_pattern(st.stroke_pattern);
    }
}

fn is_main(cr: Context, st: &CanvasState) -> bool {
    cr.raw() == st.cr
}

fn plain_source(scope: &mut Scope<'_>, this: &Value, cr: Context, rgba: [f64; 4]) {
    let ga = crate::style::global_alpha(scope, this);
    cr.set_source_rgba([rgba[0], rgba[1], rgba[2], rgba[3] * ga]);
}

fn on_own_path(cr: Context, draw: impl FnOnce(Context)) {
    let saved = cr.copy_path();
    cr.new_path();
    draw(cr);
    cr.new_path();
    cr.append_path(&saved);
}

fn rect_args(scope: &mut Scope<'_>, args: &[Value]) -> [f64; 4] {
    [
        arg(scope, args, 0),
        arg(scope, args, 1),
        arg(scope, args, 2),
        arg(scope, args, 3),
    ]
}

fn mark_mutated(scope: &Scope<'_>) {
    crate::ffi::mark_mutated(scope);
}

pub(crate) fn fill_rect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if args.len() < 4 {
        return Ok(Value::undefined());
    }
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    crate::style::sync(scope, this, st);
    let [x, y, w, h] = rect_args(scope, args);
    let st: &CanvasState = st;
    with_shadow(st, |cr| {
        if is_main(cr, st) {
            set_fill_source(scope, this, st);
        } else {
            plain_source(scope, this, cr, st.fill);
        }
        crate::style::apply_composite(scope, this, cr);
        on_own_path(cr, |cr| {
            cr.rectangle(x, y, w, h);
            cr.fill();
        });
        cr.set_operator(OPERATOR_OVER);
    });
    mark_mutated(scope);
    Ok(Value::undefined())
}

pub(crate) fn stroke_rect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if args.len() < 4 {
        return Ok(Value::undefined());
    }
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    crate::style::sync(scope, this, st);
    let [x, y, w, h] = rect_args(scope, args);
    let st: &CanvasState = st;
    let lw = st.line_width;
    with_shadow(st, |cr| {
        if is_main(cr, st) {
            set_stroke_source(scope, this, st);
        } else {
            plain_source(scope, this, cr, st.stroke);
        }
        crate::style::apply_composite(scope, this, cr);
        cr.set_line_width(lw);
        on_own_path(cr, |cr| {
            cr.rectangle(x, y, w, h);
            cr.stroke();
        });
        cr.set_operator(OPERATOR_OVER);
    });
    mark_mutated(scope);
    Ok(Value::undefined())
}

pub(crate) fn clear_rect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    if args.len() < 4 {
        return Ok(Value::undefined());
    }
    let [x, y, w, h] = rect_args(scope, args);
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    if let Some(cr) = main_context(st) {
        cr.save();
        cr.set_operator(OPERATOR_CLEAR);
        on_own_path(cr, |cr| {
            cr.rectangle(x, y, w, h);
            cr.fill();
        });
        cr.restore();
    }
    mark_mutated(scope);
    Ok(Value::undefined())
}

fn rule_text(scope: &mut Scope<'_>, v: &Value) -> Option<Vec<u8>> {
    let mut bytes = scope.to_bytes(v).ok()?;
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    Some(bytes)
}

pub(crate) fn prepare_path_and_rule(
    scope: &mut Scope<'_>,
    cr: Context,
    args: &[Value],
) -> Option<Path> {
    let mut path = None;
    let mut rule = None;
    if let Some(first) = args.first() {
        if crate::ffi::is_path2d(first) {
            path = Some(first);
            if let Some(second) = args.get(1).filter(|v| v.is_string()) {
                rule = rule_text(scope, second);
            }
        } else if first.is_string() {
            rule = rule_text(scope, first);
        }
    }
    let saved = path.map(|p| {
        let saved = cr.copy_path();
        if let Some(src) = crate::ffi::path2d_context(p) {
            let copy = src.copy_path();
            cr.new_path();
            cr.append_path(&copy);
        }
        saved
    });
    cr.set_fill_rule(crate::raster::fill_rule(rule.as_deref()));
    saved
}

pub(crate) fn restore_path(cr: Context, saved: Option<Path>) {
    if let Some(saved) = saved {
        cr.new_path();
        cr.append_path(&saved);
    }
}

pub(crate) fn fill(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    crate::style::sync(scope, this, st);
    let Some(main) = main_context(st) else {
        return Ok(Value::undefined());
    };
    let saved = prepare_path_and_rule(scope, main, args);
    let st: &CanvasState = st;
    let snapshot = crate::style::has_shadow(st).then(|| main.copy_path());
    let fill_rule = main.fill_rule();
    with_shadow(st, |cr| {
        if is_main(cr, st) {
            set_fill_source(scope, this, st);
        } else {
            cr.new_path();
            if let Some(snapshot) = &snapshot {
                cr.append_path(snapshot);
            }
            plain_source(scope, this, cr, st.fill);
        }
        cr.set_fill_rule(fill_rule);
        crate::style::apply_composite(scope, this, cr);
        cr.fill_preserve();
        cr.set_operator(OPERATOR_OVER);
    });
    drop(snapshot);
    restore_path(main, saved);
    mark_mutated(scope);
    Ok(Value::undefined())
}

pub(crate) fn stroke(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    crate::style::sync(scope, this, st);
    let Some(main) = main_context(st) else {
        return Ok(Value::undefined());
    };
    let saved = match args.first() {
        Some(first) if crate::ffi::is_path2d(first) => {
            let saved = main.copy_path();
            if let Some(src) = crate::ffi::path2d_context(first) {
                let copy = src.copy_path();
                main.new_path();
                main.append_path(&copy);
            }
            Some(saved)
        }
        _ => None,
    };
    let st: &CanvasState = st;
    let snapshot = crate::style::has_shadow(st).then(|| main.copy_path());
    let lw = st.line_width;
    with_shadow(st, |cr| {
        if is_main(cr, st) {
            set_stroke_source(scope, this, st);
        } else {
            cr.new_path();
            if let Some(snapshot) = &snapshot {
                cr.append_path(snapshot);
            }
            plain_source(scope, this, cr, st.stroke);
        }
        crate::style::apply_composite(scope, this, cr);
        cr.set_line_width(lw);
        cr.stroke_preserve();
        cr.set_operator(OPERATOR_OVER);
    });
    drop(snapshot);
    restore_path(main, saved);
    mark_mutated(scope);
    Ok(Value::undefined())
}

const OWN: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    scope
        .get(array, "length")
        .ok()
        .and_then(|l| scope.to_int32(&l).ok())
        .map_or(0, |n| n as u32)
}

pub(crate) fn save(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    if let Some(cr) = main_context(st) {
        cr.save();
    }
    let mut stack = crate::hidden::get(scope, this, "_stateStack")?;
    if !stack.is_array() {
        stack = scope.new_array();
        crate::hidden::set(scope, this, "_stateStack", stack.clone());
    }
    let snapshot = scope.new_object_with_proto(&Value::null());
    for name in SAVABLE {
        let v = crate::hidden::get(scope, this, name)?;
        let _ = scope.define(&snapshot, name, v, OWN);
    }
    let n = array_length(scope, &stack);
    let _ = scope.define(&stack, &n.to_string(), snapshot, OWN);
    Ok(Value::undefined())
}

pub(crate) fn restore(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    let stack = crate::hidden::get(scope, this, "_stateStack")?;
    if !stack.is_array() {
        return Ok(Value::undefined());
    }
    let n = array_length(scope, &stack);
    if n == 0 {
        return Ok(Value::undefined());
    }
    let snapshot = scope.get_index(&stack, n - 1)?;
    for name in SAVABLE {
        let v = scope
            .get(&snapshot, name)
            .unwrap_or_else(|_| Value::undefined());
        crate::hidden::set(scope, this, name, v);
    }
    let _ = scope.set(&stack, "length", Value::int64(i64::from(n - 1)));
    if let Some(cr) = main_context(st) {
        cr.restore();
    }
    Ok(Value::undefined())
}
