//! Southstar — the 2D context's path building and transform methods, drawing into the canvas's cairo context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::Context;
use crate::path2d::{
    Ellipse, arc_to, ellipse_scaled, extract_radii, quadratic_to, round_rect_subpath,
};

type Result = core::result::Result<Value, Value>;

fn context(scope: &mut Scope<'_>, this: &Value) -> Option<Context> {
    crate::ffi::context_cairo(scope, this)
}

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn args<const N: usize>(scope: &mut Scope<'_>, values: &[Value]) -> [f64; N] {
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = arg(scope, values, i);
    }
    out
}

fn done() -> Result {
    Ok(Value::undefined())
}

fn index_size(scope: &mut Scope<'_>, message: &str) -> Value {
    crate::api::throw_dom(scope, "IndexSizeError", message)
}

pub(crate) fn begin_path(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    if let Some(cr) = context(scope, this) {
        cr.new_path();
    }
    done()
}

pub(crate) fn close_path(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    if let Some(cr) = context(scope, this) {
        cr.close_path();
    }
    done()
}

pub(crate) fn move_to(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 2 {
        return done();
    }
    let [x, y] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.move_to(x, y);
    }
    done()
}

pub(crate) fn line_to(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 2 {
        return done();
    }
    let [x, y] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.line_to(x, y);
    }
    done()
}

pub(crate) fn arc(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 5 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let [x, y, r, a0, a1] = args(scope, a);
    if r < 0.0 {
        return Err(index_size(scope, "arc radius must not be negative"));
    }
    let ccw = a.len() >= 6 && scope.to_bool(&a[5]);
    cr.arc((x, y), r, (a0, a1), ccw);
    done()
}

pub(crate) fn rect(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 4 {
        return done();
    }
    let [x, y, w, h] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.rectangle(x, y, w, h);
    }
    done()
}

pub(crate) fn translate(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 2 {
        return done();
    }
    let [x, y] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.translate(x, y);
    }
    done()
}

pub(crate) fn scale(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 2 {
        return done();
    }
    let [x, y] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.scale(x, y);
    }
    done()
}

pub(crate) fn rotate(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.is_empty() {
        return done();
    }
    let [angle] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.rotate(angle);
    }
    done()
}

pub(crate) fn quadratic_curve_to(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 4 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let [cpx, cpy, x, y] = args(scope, a);
    quadratic_to(cr, (cpx, cpy), (x, y));
    done()
}

pub(crate) fn bezier_curve_to(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 6 {
        return done();
    }
    let [x1, y1, x2, y2, x3, y3] = args(scope, a);
    if let Some(cr) = context(scope, this) {
        cr.curve_to((x1, y1), (x2, y2), (x3, y3));
    }
    done()
}

pub(crate) fn arc_to_method(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 5 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let [x1, y1, x2, y2, r] = args(scope, a);
    if r < 0.0 {
        return Err(index_size(scope, "arcTo radius must not be negative"));
    }
    arc_to(cr, (x1, y1), (x2, y2), r);
    done()
}

pub(crate) fn ellipse(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 7 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let [x, y, rx, ry, rotation, a0, a1] = args(scope, a);
    if rx < 0.0 || ry < 0.0 {
        return Err(index_size(scope, "ellipse radius must not be negative"));
    }
    let ccw = a.len() >= 8 && scope.to_bool(&a[7]);
    let e = Ellipse {
        center: (x, y),
        radii: (rx, ry),
        rotation,
        angles: (a0, a1),
        ccw,
    };
    ellipse_scaled(cr, &e);
    done()
}

pub(crate) fn round_rect(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 4 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let rect = args::<4>(scope, a);
    let mut radii = [0.0; 4];
    if let Some(spec) = a.get(4) {
        let (r, valid) = extract_radii(scope, spec);
        if !valid {
            return Err(scope.range_error("roundRect radius must be non-negative"));
        }
        radii = r;
    }
    round_rect_subpath(cr, rect, radii);
    done()
}

const MATRIX_KEYS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

fn matrix_from_object(scope: &mut Scope<'_>, v: &Value) -> Option<[f64; 6]> {
    if !v.is_object() {
        return None;
    }
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    for (slot, key) in m.iter_mut().zip(MATRIX_KEYS) {
        *slot = scope
            .get(v, key)
            .ok()
            .and_then(|value| scope.to_number(&value).ok())
            .unwrap_or(f64::NAN);
    }
    Some(m)
}

pub(crate) fn set_transform(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    let Some(cr) = context(scope, this) else {
        return done();
    };
    match a.len() {
        0 => cr.identity_matrix(),
        1 => {
            if let Some(m) = matrix_from_object(scope, &a[0]) {
                cr.set_matrix(m);
            }
        }
        2..=5 => {}
        _ => {
            let m = args::<6>(scope, a);
            cr.set_matrix(m);
        }
    }
    done()
}

pub(crate) fn transform(scope: &mut Scope<'_>, this: &Value, a: &[Value]) -> Result {
    if a.len() < 6 {
        return done();
    }
    let Some(cr) = context(scope, this) else {
        return done();
    };
    let m = args::<6>(scope, a);
    cr.transform(m);
    done()
}

pub(crate) fn reset_transform(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    if let Some(cr) = context(scope, this) {
        cr.identity_matrix();
    }
    done()
}

pub(crate) fn get_transform(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result {
    let m = match context(scope, this) {
        Some(cr) => cr.matrix(),
        None => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    Ok(crate::context::dommatrix(scope, m))
}
