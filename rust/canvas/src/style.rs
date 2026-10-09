//! Southstar — carrying a 2D context's style attributes into its drawing state: colours and patterns, line style, shadows, dashes, alpha and compositing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::Context;
use crate::ffi::state::CanvasState;

const LINE_CAP_BUTT: i32 = 0;
const LINE_CAP_ROUND: i32 = 1;
const LINE_CAP_SQUARE: i32 = 2;
const LINE_JOIN_MITER: i32 = 0;
const LINE_JOIN_ROUND: i32 = 1;
const LINE_JOIN_BEVEL: i32 = 2;
const MAX_DASHES: u32 = 64;

fn attr(scope: &mut Scope<'_>, this: &Value, name: &str) -> Value {
    crate::hidden::get(scope, this, name).unwrap_or_else(|_| Value::undefined())
}

fn number(scope: &mut Scope<'_>, v: &Value) -> Option<f64> {
    scope.to_number(v).ok()
}

fn text(scope: &mut Scope<'_>, v: &Value) -> Option<Vec<u8>> {
    if !v.is_string() {
        return None;
    }
    let mut bytes = scope.to_bytes(v).ok()?;
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    Some(bytes)
}

pub(crate) fn global_alpha(scope: &mut Scope<'_>, this: &Value) -> f64 {
    let v = attr(scope, this, "globalAlpha");
    let ga = scope.to_number(&v).unwrap_or(f64::NAN);
    ga.clamp(0.0, 1.0)
}

pub(crate) fn apply_composite(scope: &mut Scope<'_>, this: &Value, cr: Context) {
    let v = attr(scope, this, "globalCompositeOperation");
    let name = text(scope, &v);
    cr.set_operator(crate::raster::composite_operator(name.as_deref()));
}

pub(crate) fn image_smoothing(scope: &mut Scope<'_>, this: &Value) -> bool {
    let v = attr(scope, this, "imageSmoothingEnabled");
    if v.is_undefined() || v.is_null() {
        return true;
    }
    scope.to_bool(&v)
}

fn style(
    scope: &mut Scope<'_>,
    this: &Value,
    name: &str,
    st: &mut CanvasState,
    color: &mut [f64; 4],
) -> *mut core::ffi::c_void {
    let v = attr(scope, this, name);
    if let Some(css) = text(scope, &v) {
        if let Some(rgba) = crate::color::parse(&css) {
            *color = rgba;
        }
        return core::ptr::null_mut();
    }
    if v.is_object() {
        let (pattern, clean) = crate::gradient::build_pattern(scope, &v);
        if !clean {
            st.origin_clean = 0;
        }
        return pattern;
    }
    core::ptr::null_mut()
}

fn dashes(scope: &mut Scope<'_>, this: &Value) -> Option<Vec<f64>> {
    let v = attr(scope, this, "_dashes");
    if !v.is_array() {
        return None;
    }
    let length = scope.get(&v, "length").ok()?;
    let n = scope
        .to_int32(&length)
        .map_or(0, |n| n as u32)
        .min(MAX_DASHES);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let d = scope
            .get_index(&v, i)
            .ok()
            .and_then(|e| scope.to_number(&e).ok())
            .unwrap_or(f64::NAN);
        out.push(if d < 0.0 { 0.0 } else { d });
    }
    Some(out)
}

pub(crate) fn sync(scope: &mut Scope<'_>, this: &Value, st: &mut CanvasState) {
    crate::ffi::state::clear_patterns(st);
    let cr = unsafe { Context::from_raw(st.cr) };
    let mut fill = st.fill;
    st.fill_pattern = style(scope, this, "fillStyle", st, &mut fill);
    st.fill = fill;
    let mut stroke = st.stroke;
    st.stroke_pattern = style(scope, this, "strokeStyle", st, &mut stroke);
    st.stroke = stroke;
    let v = attr(scope, this, "lineWidth");
    if let Some(lw) = number(scope, &v).filter(|&lw| lw > 0.0) {
        st.line_width = lw;
    }
    let v = attr(scope, this, "font");
    if let Some(font) = text(scope, &v) {
        crate::ffi::state::set_font(st, &font);
    }
    let v = attr(scope, this, "lineCap");
    if let (Some(cap), Some(cr)) = (text(scope, &v), cr) {
        cr.set_line_cap(match cap.as_slice() {
            b"round" => LINE_CAP_ROUND,
            b"square" => LINE_CAP_SQUARE,
            _ => LINE_CAP_BUTT,
        });
    }
    let v = attr(scope, this, "lineJoin");
    if let (Some(join), Some(cr)) = (text(scope, &v), cr) {
        cr.set_line_join(match join.as_slice() {
            b"round" => LINE_JOIN_ROUND,
            b"bevel" => LINE_JOIN_BEVEL,
            _ => LINE_JOIN_MITER,
        });
    }
    let v = attr(scope, this, "miterLimit");
    if let (Some(ml), Some(cr)) = (number(scope, &v).filter(|&ml| ml > 0.0), cr) {
        cr.set_miter_limit(ml);
    }
    st.shadow = [0.0; 4];
    st.shadow_blur = 0.0;
    st.shadow_ox = 0.0;
    st.shadow_oy = 0.0;
    let v = attr(scope, this, "shadowColor");
    if let Some(css) = text(scope, &v) {
        if let Some(rgba) = crate::color::parse(&css) {
            st.shadow = rgba;
        }
    }
    let v = attr(scope, this, "shadowBlur");
    if let Some(blur) = number(scope, &v).filter(|&b| b >= 0.0) {
        st.shadow_blur = blur;
    }
    let v = attr(scope, this, "shadowOffsetX");
    if let Some(ox) = number(scope, &v) {
        st.shadow_ox = ox;
    }
    let v = attr(scope, this, "shadowOffsetY");
    if let Some(oy) = number(scope, &v) {
        st.shadow_oy = oy;
    }
    let v = attr(scope, this, "lineDashOffset");
    let offset = scope.to_number(&v).unwrap_or(f64::NAN);
    let Some(cr) = cr else {
        return;
    };
    match dashes(scope, this) {
        Some(d) if !d.is_empty() && d.iter().sum::<f64>() > 0.0 => cr.set_dash(&d, offset),
        _ => cr.set_dash(&[], 0.0),
    }
}

pub(crate) fn has_shadow(st: &CanvasState) -> bool {
    if st.shadow[3] <= 0.0 {
        return false;
    }
    st.shadow_ox != 0.0 || st.shadow_oy != 0.0 || st.shadow_blur > 0.0
}
