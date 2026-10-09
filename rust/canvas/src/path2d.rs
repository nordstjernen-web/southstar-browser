//! Southstar — Path2D: a path recorded on its own cairo context, the rounded-rectangle geometry it shares with the 2D context, and its methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::f64::consts::PI;

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::{Context, Recording};
use crate::ffi::path2d_context;

pub(crate) struct Path2D {
    pub recording: Recording,
}

fn path_of(scope: &mut Scope<'_>, this: &Value) -> Result<Context, Value> {
    path2d_context(this).ok_or_else(|| scope.type_error("Path2D expected"))
}

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn to_number_or_nan(scope: &mut Scope<'_>, value: Result<Value, Value>) -> f64 {
    value
        .ok()
        .and_then(|v| scope.to_number(&v).ok())
        .unwrap_or(f64::NAN)
}

pub(crate) fn quadratic_to(cr: Context, control: (f64, f64), to: (f64, f64)) {
    if !cr.has_current_point() {
        cr.move_to(control.0, control.1);
    }
    let (x0, y0) = cr.current_point();
    let (cpx, cpy) = control;
    let (x, y) = to;
    cr.curve_to(
        (x0 + 2.0 / 3.0 * (cpx - x0), y0 + 2.0 / 3.0 * (cpy - y0)),
        (x + 2.0 / 3.0 * (cpx - x), y + 2.0 / 3.0 * (cpy - y)),
        (x, y),
    );
}

pub(crate) fn arc_to(cr: Context, p1: (f64, f64), p2: (f64, f64), r: f64) {
    let (x1, y1) = p1;
    let (x2, y2) = p2;
    if !cr.has_current_point() {
        cr.move_to(x1, y1);
    }
    let (x0, y0) = cr.current_point();
    let (a1x, a1y) = (x0 - x1, y0 - y1);
    let (a2x, a2y) = (x2 - x1, y2 - y1);
    let (l1, l2) = (a1x.hypot(a1y), a2x.hypot(a2y));
    if l1 == 0.0 || l2 == 0.0 || r == 0.0 {
        cr.line_to(x1, y1);
        return;
    }
    let (u1x, u1y) = (a1x / l1, a1y / l1);
    let (u2x, u2y) = (a2x / l2, a2y / l2);
    let cos_t = u1x * u2x + u1y * u2y;
    if cos_t >= 1.0 || cos_t <= -1.0 {
        cr.line_to(x1, y1);
        return;
    }
    let tan_half = ((1.0 - cos_t) / (1.0 + cos_t)).sqrt();
    let dist = r / tan_half;
    let (t1x, t1y) = (x1 + u1x * dist, y1 + u1y * dist);
    let (t2x, t2y) = (x1 + u2x * dist, y1 + u2y * dist);
    let (mut bisx, mut bisy) = (u1x + u2x, u1y + u2y);
    let blen = bisx.hypot(bisy);
    if blen == 0.0 {
        cr.line_to(t1x, t1y);
        return;
    }
    bisx /= blen;
    bisy /= blen;
    let cdist = (r * r + dist * dist).sqrt();
    let (cx, cy) = (x1 + bisx * cdist, y1 + bisy * cdist);
    let ang1 = (t1y - cy).atan2(t1x - cx);
    let ang2 = (t2y - cy).atan2(t2x - cx);
    let cross = u1x * u2y - u1y * u2x;
    cr.line_to(t1x, t1y);
    cr.arc((cx, cy), r, (ang1, ang2), cross < 0.0);
}

pub(crate) struct Ellipse {
    pub center: (f64, f64),
    pub radii: (f64, f64),
    pub rotation: f64,
    pub angles: (f64, f64),
    pub ccw: bool,
}

const DEGENERATE_ELLIPSE_STEPS: u32 = 64;

fn degenerate_ellipse(cr: Context, e: &Ellipse) {
    let (a0, mut a1) = e.angles;
    let full = 2.0 * PI;
    if e.ccw {
        while a1 > a0 {
            a1 -= full;
        }
    } else {
        while a1 < a0 {
            a1 += full;
        }
    }
    let (cos_r, sin_r) = (e.rotation.cos(), e.rotation.sin());
    for step in 0..=DEGENERATE_ELLIPSE_STEPS {
        let t = a0 + (a1 - a0) * f64::from(step) / f64::from(DEGENERATE_ELLIPSE_STEPS);
        let (px, py) = (e.radii.0 * t.cos(), e.radii.1 * t.sin());
        let x = e.center.0 + px * cos_r - py * sin_r;
        let y = e.center.1 + px * sin_r + py * cos_r;
        if step == 0 && !cr.has_current_point() {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
}

pub(crate) fn ellipse(cr: Context, e: &Ellipse) {
    if e.radii.0 == 0.0 || e.radii.1 == 0.0 {
        degenerate_ellipse(cr, e);
        return;
    }
    ellipse_scaled(cr, e);
}

pub(crate) fn ellipse_scaled(cr: Context, e: &Ellipse) {
    cr.save();
    cr.translate(e.center.0, e.center.1);
    cr.rotate(e.rotation);
    cr.scale(e.radii.0, e.radii.1);
    cr.arc((0.0, 0.0), 1.0, e.angles, e.ccw);
    cr.restore();
}

pub(crate) fn round_rect_subpath(cr: Context, rect: [f64; 4], radii: [f64; 4]) {
    let [mut x, mut y, w, h] = rect;
    let (aw, ah) = (w.abs(), h.abs());
    let max_r = (if aw < ah { aw } else { ah }) / 2.0;
    let clamp = |r: f64| if r > max_r { max_r } else { r };
    let [rtl, rtr, rbr, rbl] = radii.map(clamp);
    let (mut x2, mut y2) = (x + w, y + h);
    if w < 0.0 {
        core::mem::swap(&mut x, &mut x2);
    }
    if h < 0.0 {
        core::mem::swap(&mut y, &mut y2);
    }
    cr.new_sub_path();
    cr.arc((x + rtl, y + rtl), rtl, (PI, 1.5 * PI), false);
    cr.arc((x2 - rtr, y + rtr), rtr, (1.5 * PI, 0.0), false);
    cr.arc((x2 - rbr, y2 - rbr), rbr, (0.0, 0.5 * PI), false);
    cr.arc((x + rbl, y2 - rbl), rbl, (0.5 * PI, PI), false);
    cr.close_path();
}

pub(crate) fn extract_radii(scope: &mut Scope<'_>, v: &Value) -> ([f64; 4], bool) {
    let radii = if v.is_array() {
        let length = scope
            .get(v, "length")
            .ok()
            .and_then(|l| scope.to_int32(&l).ok())
            .map_or(0, |n| n as u32);
        let mut r = [0.0; 4];
        for (i, slot) in r.iter_mut().enumerate().take(length.min(4) as usize) {
            let element = scope.get_index(v, i as u32);
            *slot = to_number_or_nan(scope, element);
        }
        match length {
            1 => [r[0]; 4],
            2 => [r[0], r[1], r[0], r[1]],
            3 => [r[0], r[1], r[2], r[1]],
            _ => r,
        }
    } else {
        [scope.to_number(v).unwrap_or(f64::NAN); 4]
    };
    let valid = !radii.iter().any(|&r| r < 0.0);
    (radii, valid)
}

pub(crate) fn move_to(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 2 {
        let (x, y) = (arg(scope, args, 0), arg(scope, args, 1));
        cr.move_to(x, y);
    }
    Ok(Value::undefined())
}

pub(crate) fn line_to(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 2 {
        if !cr.has_current_point() {
            let (x, y) = (arg(scope, args, 0), arg(scope, args, 1));
            cr.move_to(x, y);
        }
        let (x, y) = (arg(scope, args, 0), arg(scope, args, 1));
        cr.line_to(x, y);
    }
    Ok(Value::undefined())
}

pub(crate) fn close_path(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    path_of(scope, this)?.close_path();
    Ok(Value::undefined())
}

pub(crate) fn bezier_curve_to(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 6 {
        let v: Vec<f64> = (0..6).map(|i| arg(scope, args, i)).collect();
        cr.curve_to((v[0], v[1]), (v[2], v[3]), (v[4], v[5]));
    }
    Ok(Value::undefined())
}

pub(crate) fn quadratic_curve_to(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 4 {
        let v: Vec<f64> = (0..4).map(|i| arg(scope, args, i)).collect();
        quadratic_to(cr, (v[0], v[1]), (v[2], v[3]));
    }
    Ok(Value::undefined())
}

pub(crate) fn arc(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 5 {
        let v: Vec<f64> = (0..5).map(|i| arg(scope, args, i)).collect();
        let ccw = args.len() >= 6 && scope.to_bool(&args[5]);
        cr.arc((v[0], v[1]), v[2], (v[3], v[4]), ccw);
    }
    Ok(Value::undefined())
}

pub(crate) fn arc_to_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 5 {
        let v: Vec<f64> = (0..5).map(|i| arg(scope, args, i)).collect();
        arc_to(cr, (v[0], v[1]), (v[2], v[3]), v[4]);
    }
    Ok(Value::undefined())
}

pub(crate) fn ellipse_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 7 {
        let v: Vec<f64> = (0..7).map(|i| arg(scope, args, i)).collect();
        let ccw = args.len() >= 8 && scope.to_bool(&args[7]);
        if v.iter().any(|n| !n.is_finite()) {
            return Ok(Value::undefined());
        }
        if v[2] < 0.0 || v[3] < 0.0 {
            return Err(crate::api::throw_dom(
                scope,
                "IndexSizeError",
                "ellipse radius must not be negative",
            ));
        }
        let e = Ellipse {
            center: (v[0], v[1]),
            radii: (v[2], v[3]),
            rotation: v[4],
            angles: (v[5], v[6]),
            ccw,
        };
        ellipse(cr, &e);
    }
    Ok(Value::undefined())
}

pub(crate) fn rect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() >= 4 {
        let v: Vec<f64> = (0..4).map(|i| arg(scope, args, i)).collect();
        cr.rectangle(v[0], v[1], v[2], v[3]);
    }
    Ok(Value::undefined())
}

pub(crate) fn round_rect(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    if args.len() < 4 {
        return Ok(Value::undefined());
    }
    let v = [
        arg(scope, args, 0),
        arg(scope, args, 1),
        arg(scope, args, 2),
        arg(scope, args, 3),
    ];
    let mut radii = [0.0; 4];
    if let Some(spec) = args.get(4) {
        let (r, valid) = extract_radii(scope, spec);
        if !valid {
            return Err(scope.range_error("roundRect radius must be non-negative"));
        }
        radii = r;
    }
    round_rect_subpath(cr, v, radii);
    Ok(Value::undefined())
}

const MATRIX_KEYS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

pub(crate) fn add_path(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cr = path_of(scope, this)?;
    let Some(src) = args.first().and_then(path2d_context) else {
        return Ok(Value::undefined());
    };
    let mut path = src.copy_path();
    if let Some(m) = args.get(1).filter(|v| v.is_object()) {
        let mut matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        for (slot, key) in matrix.iter_mut().zip(MATRIX_KEYS) {
            let value = scope.get(m, key);
            *slot = to_number_or_nan(scope, value);
        }
        if matrix.iter().any(|n| !n.is_finite()) {
            return Ok(Value::undefined());
        }
        path.transform(matrix);
    }
    cr.append_path(&path);
    Ok(Value::undefined())
}

pub(crate) fn construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if new_target.is_undefined() {
        return Err(crate::api::new_required(scope, "Path2D"));
    }
    let recording = Recording::new();
    let cr = recording.context();
    let proto = crate::api::api_proto_of_ctor(scope, new_target, "Path2D");
    let proto = proto.is_object().then_some(&proto);
    let obj = scope.new_host_object(proto, Path2D { recording });
    match args.first() {
        Some(source) if path2d_context(source).is_some() => {
            if let Some(src) = path2d_context(source) {
                cr.append_path(&src.copy_path());
            }
        }
        Some(d) if d.is_string() => {
            if let Ok(d) = scope.to_bytes(d) {
                let end = d.iter().position(|&c| c == 0).unwrap_or(d.len());
                let mut sink = cr;
                crate::path::parse(&mut sink, &d[..end]);
            }
        }
        _ => {}
    }
    Ok(obj)
}
