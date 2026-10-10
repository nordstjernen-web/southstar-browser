//! Southstar — CSS transforms: transform lists, transform-origin and the translate, rotate and scale properties parsed into the ns_css_transform css.c stores, their computed and canonical specified spellings, whether they are 3D, and the matrix they apply.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use std::ffi::CString;

use southstar_mat4::Mat4;

use crate::calc::{self, Parsed};
use crate::ffi;
use crate::math;
use crate::position;
use crate::scan::{is_ws, scan_until, skip_ws, split_ws_limit, starts_with_ci};
use crate::text::{add_leading_zeros, normalize_negative_zero, split_top_level_commas};
use crate::units::{self, NUMBER, PERCENT};

pub(crate) const OPS_MAX: usize = 8;

pub(crate) const TRANSLATE: u32 = 0;
pub(crate) const ROTATE: u32 = 1;
pub(crate) const SCALE: u32 = 2;
pub(crate) const SKEW: u32 = 3;
pub(crate) const MATRIX: u32 = 4;
pub(crate) const MATRIX3D: u32 = 5;
pub(crate) const ROTATE3D: u32 = 6;
pub(crate) const PERSPECTIVE: u32 = 7;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Individual {
    Translate,
    Rotate,
    Scale,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Op {
    pub kind: u32,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
    pub m3d: [f64; 16],
    pub a_is_percent: i32,
    pub b_is_percent: i32,
    pub e_is_percent: i32,
    pub f_is_percent: i32,
    pub a_pct: f64,
    pub b_pct: f64,
    pub em: [f64; 3],
    pub rem: [f64; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Transform {
    pub n_ops: i32,
    pub ops: [Op; OPS_MAX],
}

const _: () = assert!(
    core::mem::size_of::<Op>() == 264
        && core::mem::offset_of!(Op, m3d) == 56
        && core::mem::offset_of!(Op, a_is_percent) == 184
        && core::mem::offset_of!(Op, a_pct) == 200
        && core::mem::offset_of!(Op, rem) == 240
        && core::mem::size_of::<Transform>() == 2120
);

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn strtod_all(text: &[u8]) -> f64 {
    ffi::strtod(&c_text(text), 0).0
}

fn flag(value: bool) -> i32 {
    i32::from(value)
}

fn parse_transform_len(s: &[u8]) -> Option<(f64, bool)> {
    let (ok, resolved) = calc::resolve_to_px_pct(s, false);
    if ok {
        return Some(if resolved.pct != 0.0 && resolved.px == 0.0 {
            (resolved.pct, true)
        } else {
            (resolved.px, false)
        });
    }
    let text = c_text(s);
    let bytes = text.to_bytes();
    let (v, end) = ffi::strtod(&text, 0);
    if end == 0 {
        return None;
    }
    let end = skip_ws(bytes, end, bytes.len());
    Some((v, bytes.get(end) == Some(&b'%')))
}

fn parse_translate_len(s: &[u8], op: &mut Op, axis: usize) -> bool {
    let (ok, resolved) = calc::resolve_to_px_pct(s, true);
    if !ok {
        let Some((value, percent)) = parse_transform_len(s) else {
            return false;
        };
        match axis {
            0 => {
                op.a = value;
                op.a_is_percent = flag(percent);
            }
            1 => {
                op.b = value;
                op.b_is_percent = flag(percent);
            }
            _ => op.c = value,
        }
        return axis < 2 || !percent;
    }
    op.em[axis] = resolved.em;
    op.rem[axis] = resolved.rem;
    if axis == 2 {
        op.c = resolved.px;
        return resolved.pct == 0.0;
    }
    let pure_percent =
        resolved.pct != 0.0 && resolved.px == 0.0 && resolved.em == 0.0 && resolved.rem == 0.0;
    let (field, percent) = if axis == 0 {
        (&mut op.a, &mut op.a_is_percent)
    } else {
        (&mut op.b, &mut op.b_is_percent)
    };
    if pure_percent {
        *field = resolved.pct;
        *percent = 1;
        return true;
    }
    *field = resolved.px;
    *percent = 0;
    if axis == 0 {
        op.a_pct = resolved.pct;
    } else {
        op.b_pct = resolved.pct;
    }
    true
}

const INVERSE_TRIG: &[&[u8]] = &[b"atan2(", b"atan(", b"asin(", b"acos("];
const ANGLE_MATH: &[&[u8]] = &[
    b"calc(", b"min(", b"max(", b"clamp(", b"round(", b"mod(", b"rem(", b"abs(", b"sign(",
    b"hypot(", b"pow(", b"sqrt(", b"sin(", b"cos(", b"tan(", b"exp(", b"log(",
];

pub(crate) fn parse_angle_any(s: &[u8]) -> Option<f64> {
    let s = &s[skip_ws(s, 0, s.len())..];
    if s.is_empty() {
        return None;
    }
    if INVERSE_TRIG.iter().any(|name| starts_with_ci(s, name)) {
        let (ok, resolved) = calc::resolve_to_px_pct(s, false);
        return ok.then(|| resolved.px * 180.0 / PI);
    }
    if ANGLE_MATH.iter().any(|name| starts_with_ci(s, name)) {
        let rewritten = units::angle_expr_rewrite(&c_text(s), false);
        let (ok, resolved) = calc::resolve_to_px_pct(&rewritten, false);
        return ok.then_some(resolved.px);
    }
    let (_, end) = ffi::strtod(&c_text(s), 0);
    if end == 0 {
        return None;
    }
    Some(crate::gradient::parse_angle_deg(s))
}

fn parse_scale_number(s: &[u8]) -> Option<f64> {
    let s = &s[skip_ws(s, 0, s.len())..];
    if s.is_empty() {
        return None;
    }
    let text = c_text(s);
    let bytes = text.to_bytes();
    let (v, end) = ffi::strtod(&text, 0);
    if end != 0 {
        let p = skip_ws(bytes, end, bytes.len());
        if p == bytes.len() {
            return Some(v);
        }
        if bytes[p] == b'%' && p + 1 == bytes.len() {
            return Some(v / 100.0);
        }
    }
    let (ok, resolved) = calc::resolve_to_px_pct(bytes, false);
    if ok {
        return Some(resolved.px + resolved.pct / 100.0);
    }
    let parsed = calc::parse_calc(&text).or_else(|| {
        let mut wrapped = b"calc(".to_vec();
        wrapped.extend_from_slice(bytes);
        wrapped.push(b')');
        calc::parse_calc(&c_text(&wrapped))
    });
    match parsed {
        Some(Parsed::Length(v, unit)) => {
            let value = if unit == PERCENT { v / 100.0 } else { v };
            Some(if value.is_nan() { 0.0 } else { value })
        }
        _ => None,
    }
}

fn split_args(args: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut seg = 0;
    let mut depth = 0i32;
    for q in 0..=args.len() {
        let c = args.get(q).copied().unwrap_or(0);
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
        }
        if (c == b',' && depth == 0) || c == 0 {
            if out.len() < 16 {
                out.push(crate::scan::strip(&args[seg..q]).to_vec());
            }
            if c == 0 {
                break;
            }
            seg = q + 1;
        }
    }
    out
}

pub(crate) fn parse_transform(text: &[u8]) -> Option<Transform> {
    let text = &text[skip_ws(text, 0, text.len())..];
    let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
    if text.is_empty() || starts_with_ci(text, b"none") {
        return None;
    }
    let mut tf = Transform::default();
    let mut p = 0;
    while p < text.len() && (tf.n_ops as usize) < OPS_MAX {
        while p < text.len() && (is_ws(text[p]) || text[p] == b',') {
            p += 1;
        }
        if p >= text.len() {
            break;
        }
        let name_start = p;
        while p < text.len() && text[p] != b'(' {
            p += 1;
        }
        if p >= text.len() {
            break;
        }
        let name = crate::scan::strip(&text[name_start..p]).to_ascii_lowercase();
        p += 1;
        let args_start = p;
        let mut depth = 1;
        while p < text.len() && depth > 0 {
            if text[p] == b'(' {
                depth += 1;
            } else if text[p] == b')' {
                depth -= 1;
            }
            if depth > 0 {
                p += 1;
            }
        }
        if depth != 0 {
            break;
        }
        let args = &text[args_start..p];
        if p < text.len() && text[p] == b')' {
            p += 1;
        }
        let targs = split_args(args);
        let nt = targs.len();
        let op = &mut tf.ops[tf.n_ops as usize];
        let accept = parse_function(&name, &targs, nt, op);
        if accept {
            tf.n_ops += 1;
        }
    }
    (tf.n_ops > 0).then_some(tf)
}

fn parse_function(name: &[u8], targs: &[Vec<u8>], nt: usize, op: &mut Op) -> bool {
    match name {
        b"translate" | b"translatex" | b"translatey" | b"translatez" => {
            op.kind = TRANSLATE;
            op.a = 0.0;
            op.b = 0.0;
            op.c = 0.0;
            op.a_is_percent = 0;
            op.b_is_percent = 0;
            if name == b"translatey" {
                if nt >= 1 {
                    parse_translate_len(&targs[0], op, 1);
                }
            } else if name == b"translatez" {
                if nt >= 1 {
                    parse_translate_len(&targs[0], op, 2);
                }
            } else {
                if nt >= 1 {
                    parse_translate_len(&targs[0], op, 0);
                }
                if nt >= 2 {
                    parse_translate_len(&targs[1], op, 1);
                }
            }
            true
        }
        b"rotate" | b"rotatez" => {
            op.kind = ROTATE;
            op.a = 0.0;
            op.b = 0.0;
            nt == 1 && parse_angle_into(&targs[0], &mut op.a)
        }
        b"rotatex" | b"rotatey" => {
            op.kind = ROTATE3D;
            op.a = if name == b"rotatex" { 1.0 } else { 0.0 };
            op.b = if name == b"rotatey" { 1.0 } else { 0.0 };
            op.c = 0.0;
            op.d = 0.0;
            nt == 1 && parse_angle_into(&targs[0], &mut op.d)
        }
        b"rotate3d" if nt == 4 => {
            op.kind = ROTATE3D;
            op.a = strtod_all(&targs[0]);
            op.b = strtod_all(&targs[1]);
            op.c = strtod_all(&targs[2]);
            op.d = 0.0;
            parse_angle_into(&targs[3], &mut op.d)
        }
        b"perspective" if nt >= 1 => {
            op.kind = PERSPECTIVE;
            op.a = 0.0;
            if let Some((value, _)) = parse_transform_len(&targs[0]) {
                op.a = value;
            }
            true
        }
        b"scale" | b"scalex" | b"scaley" | b"scalez" => {
            op.kind = SCALE;
            let mut sa = 1.0;
            if nt >= 1 {
                sa = parse_scale_number(&targs[0]).unwrap_or(0.0);
            }
            let mut sb = sa;
            if nt >= 2 {
                sb = parse_scale_number(&targs[1]).unwrap_or(0.0);
            }
            op.c = 1.0;
            match name {
                b"scalex" => {
                    op.a = sa;
                    op.b = 1.0;
                }
                b"scaley" => {
                    op.a = 1.0;
                    op.b = sa;
                }
                b"scalez" => {
                    op.a = 1.0;
                    op.b = 1.0;
                    op.c = sa;
                }
                _ => {
                    op.a = sa;
                    op.b = sb;
                }
            }
            true
        }
        b"skew" | b"skewx" | b"skewy" => {
            op.kind = SKEW;
            let mut aa = 0.0;
            let mut bb = 0.0;
            if nt >= 1 {
                parse_angle_into(&targs[0], &mut aa);
            }
            if nt >= 2 {
                parse_angle_into(&targs[1], &mut bb);
            }
            match name {
                b"skewx" => {
                    op.a = aa;
                    op.b = 0.0;
                }
                b"skewy" => {
                    op.a = 0.0;
                    op.b = aa;
                }
                _ => {
                    op.a = aa;
                    op.b = bb;
                }
            }
            true
        }
        b"matrix" if nt == 6 => {
            op.kind = MATRIX;
            op.a = strtod_all(&targs[0]);
            op.b = strtod_all(&targs[1]);
            op.c = strtod_all(&targs[2]);
            op.d = strtod_all(&targs[3]);
            op.e = strtod_all(&targs[4]);
            op.f = strtod_all(&targs[5]);
            true
        }
        b"matrix3d" if nt == 16 => {
            op.kind = MATRIX3D;
            for (slot, arg) in op.m3d.iter_mut().zip(targs) {
                *slot = strtod_all(arg);
            }
            true
        }
        b"translate3d" if nt >= 2 => {
            op.kind = TRANSLATE;
            op.a = 0.0;
            op.b = 0.0;
            op.c = 0.0;
            op.a_is_percent = 0;
            op.b_is_percent = 0;
            parse_translate_len(&targs[0], op, 0);
            parse_translate_len(&targs[1], op, 1);
            if nt >= 3 {
                parse_translate_len(&targs[2], op, 2);
            }
            true
        }
        b"scale3d" if nt >= 2 => {
            op.kind = SCALE;
            op.a = 0.0;
            op.b = 0.0;
            op.c = 1.0;
            if let Some(v) = parse_scale_number(&targs[0]) {
                op.a = v;
            }
            if let Some(v) = parse_scale_number(&targs[1]) {
                op.b = v;
            }
            if nt >= 3 {
                if let Some(v) = parse_scale_number(&targs[2]) {
                    op.c = v;
                }
            }
            true
        }
        _ => false,
    }
}

fn parse_angle_into(s: &[u8], out: &mut f64) -> bool {
    match parse_angle_any(s) {
        Some(deg) => {
            *out = deg;
            true
        }
        None => false,
    }
}

fn parse_origin_axis(tok: &[u8], is_y: bool, out: &mut f64, is_percent: &mut i32) -> bool {
    if tok.is_empty() {
        return false;
    }
    let lc = tok.to_ascii_lowercase();
    let keyword = match (lc.as_slice(), is_y) {
        (b"center", _) => Some(50.0),
        (b"left", false) | (b"top", true) => Some(0.0),
        (b"right", false) | (b"bottom", true) => Some(100.0),
        _ => None,
    };
    if let Some(value) = keyword {
        *out = value;
        *is_percent = 1;
        return true;
    }
    match parse_transform_len(tok) {
        Some((value, percent)) => {
            *out = value;
            *is_percent = flag(percent);
            true
        }
        None => false,
    }
}

fn ws_token_count(s: &[u8]) -> usize {
    let mut n = 0;
    let mut p = 0;
    while p < s.len() {
        p = skip_ws(s, p, s.len());
        if p >= s.len() {
            break;
        }
        p = scan_until(s, p, s.len(), b" \t\n\r\x0c").0;
        n += 1;
    }
    n
}

fn origin_op() -> Op {
    Op {
        kind: TRANSLATE,
        a: 50.0,
        b: 50.0,
        c: 0.0,
        a_is_percent: 1,
        b_is_percent: 1,
        ..Op::default()
    }
}

pub(crate) fn parse_transform_origin(text: &[u8]) -> Option<Transform> {
    if text.is_empty() {
        return None;
    }
    let mut tf = Transform {
        n_ops: 1,
        ..Transform::default()
    };
    let edge_canon = if ws_token_count(text) >= 3 {
        position::canonical_ex(text, true, false)
    } else {
        None
    };
    if let Some(canon) = edge_canon {
        let (xs, ys) = position::split(&canon);
        let mut op = origin_op();
        let (mut a, mut ap) = (op.a, op.a_is_percent);
        parse_origin_axis(&xs, false, &mut a, &mut ap);
        let (mut b, mut bp) = (op.b, op.b_is_percent);
        parse_origin_axis(&ys, true, &mut b, &mut bp);
        op.a = a;
        op.a_is_percent = ap;
        op.b = b;
        op.b_is_percent = bp;
        tf.ops[0] = op;
        return Some(tf);
    }
    let toks = split_ws_limit(text, 3);
    let a = toks.first().copied();
    let b = toks.get(1).copied();
    let zc = toks.get(2).copied();
    let mut op = origin_op();
    let swap = a.is_some_and(|a| {
        let lc = a.to_ascii_lowercase();
        lc == b"top" || lc == b"bottom"
    });
    let (mut ax, mut axp, mut bx, mut bxp) = (op.a, op.a_is_percent, op.b, op.b_is_percent);
    if swap {
        if let Some(a) = a {
            parse_origin_axis(a, true, &mut bx, &mut bxp);
        }
        if let Some(b) = b {
            parse_origin_axis(b, false, &mut ax, &mut axp);
        }
    } else {
        if let Some(a) = a {
            parse_origin_axis(a, false, &mut ax, &mut axp);
        }
        if let Some(b) = b {
            parse_origin_axis(b, true, &mut bx, &mut bxp);
        } else if let Some(a) = a {
            let lc = a.to_ascii_lowercase();
            if lc == b"left" || lc == b"right" {
                bx = 50.0;
                bxp = 1;
            }
        }
    }
    op.a = ax;
    op.a_is_percent = axp;
    op.b = bx;
    op.b_is_percent = bxp;
    if let Some(zc) = zc {
        if let Some((value, percent)) = parse_transform_len(zc) {
            op.c = if percent { 0.0 } else { value };
        }
    }
    tf.ops[0] = op;
    Some(tf)
}

fn one_op(op: Op) -> Transform {
    let mut tf = Transform {
        n_ops: 1,
        ..Transform::default()
    };
    tf.ops[0] = op;
    tf
}

fn trimmed_not_none(text: &[u8]) -> Option<&[u8]> {
    let text = &text[skip_ws(text, 0, text.len())..];
    (!text.is_empty() && !text.eq_ignore_ascii_case(b"none")).then_some(text)
}

pub(crate) fn parse_translate_prop(text: &[u8]) -> Option<Transform> {
    let text = trimmed_not_none(text)?;
    let toks = split_ws_limit(text, 3);
    if toks.is_empty() {
        return None;
    }
    let mut op = Op {
        kind: TRANSLATE,
        ..Op::default()
    };
    let mut ok = parse_translate_len(toks[0], &mut op, 0);
    if ok && toks.len() >= 2 {
        ok = parse_translate_len(toks[1], &mut op, 1);
    }
    if ok && toks.len() >= 3 {
        ok = parse_translate_len(toks[2], &mut op, 2);
    }
    ok.then(|| one_op(op))
}

pub(crate) fn parse_rotate_prop(text: &[u8]) -> Option<Transform> {
    let text = trimmed_not_none(text)?;
    let toks = split_ws_limit(text, 4);
    let mut op = Op::default();
    match toks.len() {
        1 => {
            op.kind = ROTATE;
            op.a = parse_angle_any(toks[0])?;
            Some(one_op(op))
        }
        2 => {
            op.kind = ROTATE3D;
            match toks[0].to_ascii_lowercase().as_slice() {
                b"x" => op.a = 1.0,
                b"y" => op.b = 1.0,
                b"z" => op.c = 1.0,
                _ => return None,
            }
            op.d = parse_angle_any(toks[1])?;
            if op.c == 1.0 && op.a == 0.0 && op.b == 0.0 {
                op.kind = ROTATE;
                op.a = op.d;
                op.b = 0.0;
                op.c = 0.0;
                op.d = 0.0;
            }
            Some(one_op(op))
        }
        4 => {
            op.kind = ROTATE3D;
            op.a = strtod_all(toks[0]);
            op.b = strtod_all(toks[1]);
            op.c = strtod_all(toks[2]);
            op.d = parse_angle_any(toks[3])?;
            Some(one_op(op))
        }
        _ => None,
    }
}

pub(crate) fn parse_scale_prop(text: &[u8]) -> Option<Transform> {
    let text = trimmed_not_none(text)?;
    let toks = split_ws_limit(text, 3);
    if toks.is_empty() {
        return None;
    }
    let mut op = Op {
        kind: SCALE,
        ..Op::default()
    };
    let first = parse_scale_number(toks[0]);
    op.a = first.unwrap_or(0.0);
    let mut ok = first.is_some();
    op.b = op.a;
    op.c = 1.0;
    if ok && toks.len() >= 2 {
        match parse_scale_number(toks[1]) {
            Some(v) => op.b = v,
            None => ok = false,
        }
    }
    if ok && toks.len() >= 3 {
        match parse_scale_number(toks[2]) {
            Some(v) => op.c = v,
            None => ok = false,
        }
    }
    ok.then(|| one_op(op))
}

fn append_scale_number(out: &mut Vec<u8>, n: f64) {
    if n.is_nan() {
        out.push(b'0');
    } else if !n.is_finite() {
        out.extend_from_slice(if n < 0.0 {
            b"calc(-infinity)"
        } else {
            b"calc(infinity)"
        });
    } else {
        out.extend_from_slice(&units::number_text(n));
    }
}

fn axis_pct(op: &Op, axis: usize) -> f64 {
    match axis {
        0 => op.a_pct,
        1 => op.b_pct,
        _ => 0.0,
    }
}

fn axis_is_mixed(op: &Op, axis: usize) -> bool {
    axis_pct(op, axis) != 0.0 || op.em[axis] != 0.0 || op.rem[axis] != 0.0
}

fn append_translate_length(out: &mut Vec<u8>, op: &Op, axis: usize) {
    let v = match axis {
        0 => op.a,
        1 => op.b,
        _ => op.c,
    };
    let is_percent = match axis {
        0 => op.a_is_percent != 0,
        1 => op.b_is_percent != 0,
        _ => false,
    };
    if is_percent || !axis_is_mixed(op, axis) {
        out.extend_from_slice(&units::number_text(v));
        out.extend_from_slice(if is_percent { b"%" } else { b"px" });
        return;
    }
    let parts = [axis_pct(op, axis), v, op.em[axis], op.rem[axis]];
    let suffixes: [&[u8]; 4] = [b"%", b"px", b"em", b"rem"];
    let mut first = true;
    out.extend_from_slice(b"calc(");
    for (part, suffix) in parts.into_iter().zip(suffixes) {
        if part == 0.0 {
            continue;
        }
        if !first {
            out.extend_from_slice(if part < 0.0 { b" - " } else { b" + " });
        }
        out.extend_from_slice(&units::number_text(if first { part } else { part.abs() }));
        out.extend_from_slice(suffix);
        first = false;
    }
    out.push(b')');
}

pub(crate) fn individual_serialize(tf: &Transform, prop: Individual) -> Option<Vec<u8>> {
    if tf.n_ops < 1 {
        return None;
    }
    let op = &tf.ops[0];
    let mut out = Vec::new();
    match prop {
        Individual::Scale => {
            let same = op.a == op.b || (op.a.is_nan() && op.b.is_nan());
            append_scale_number(&mut out, op.a);
            if op.c != 1.0 {
                out.push(b' ');
                append_scale_number(&mut out, op.b);
                out.push(b' ');
                append_scale_number(&mut out, op.c);
            } else if !same {
                out.push(b' ');
                append_scale_number(&mut out, op.b);
            }
        }
        Individual::Rotate => {
            if op.kind == ROTATE3D {
                let axis: Option<&[u8]> = if op.a == 1.0 && op.b == 0.0 && op.c == 0.0 {
                    Some(b"x")
                } else if op.a == 0.0 && op.b == 1.0 && op.c == 0.0 {
                    Some(b"y")
                } else {
                    None
                };
                if let Some(axis) = axis {
                    out.extend_from_slice(axis);
                    out.push(b' ');
                } else if !(op.a == 0.0 && op.b == 0.0 && op.c == 1.0) {
                    for v in [op.a, op.b, op.c] {
                        append_scale_number(&mut out, v);
                        out.push(b' ');
                    }
                }
                append_scale_number(&mut out, op.d);
            } else {
                append_scale_number(&mut out, op.a);
            }
            out.extend_from_slice(b"deg");
        }
        Individual::Translate => {
            append_translate_length(&mut out, op, 0);
            let need_c = op.c != 0.0 || axis_is_mixed(op, 2);
            let need_b = need_c || op.b != 0.0 || op.b_is_percent != 0 || axis_is_mixed(op, 1);
            if need_b {
                out.push(b' ');
                append_translate_length(&mut out, op, 1);
            }
            if need_c {
                out.push(b' ');
                append_translate_length(&mut out, op, 2);
            }
        }
    }
    Some(out)
}

fn g(v: f64) -> Vec<u8> {
    ffi::format_double(c"%g", v)
}

pub(crate) fn serialize(tf: &Transform) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..(tf.n_ops.max(0) as usize).min(OPS_MAX) {
        let op = &tf.ops[i];
        if i > 0 {
            out.push(b' ');
        }
        match op.kind {
            TRANSLATE => {
                if axis_is_mixed(op, 0) || axis_is_mixed(op, 1) || axis_is_mixed(op, 2) {
                    let three_d = op.c != 0.0 || axis_is_mixed(op, 2);
                    out.extend_from_slice(if three_d {
                        b"translate3d("
                    } else {
                        b"translate("
                    });
                    append_translate_length(&mut out, op, 0);
                    out.extend_from_slice(b", ");
                    append_translate_length(&mut out, op, 1);
                    if three_d {
                        out.extend_from_slice(b", ");
                        append_translate_length(&mut out, op, 2);
                    }
                    out.push(b')');
                } else {
                    out.extend_from_slice(b"translate(");
                    out.extend_from_slice(&g(op.a));
                    out.extend_from_slice(if op.a_is_percent != 0 { b"%" } else { b"px" });
                    out.extend_from_slice(b", ");
                    out.extend_from_slice(&g(op.b));
                    out.extend_from_slice(if op.b_is_percent != 0 { b"%" } else { b"px" });
                    out.push(b')');
                }
            }
            ROTATE => {
                out.extend_from_slice(b"rotate(");
                out.extend_from_slice(&g(op.a));
                out.extend_from_slice(b"deg)");
            }
            SCALE => {
                out.extend_from_slice(b"scale(");
                out.extend_from_slice(&g(op.a));
                out.extend_from_slice(b", ");
                out.extend_from_slice(&g(op.b));
                out.push(b')');
            }
            SKEW => {
                out.extend_from_slice(b"skew(");
                out.extend_from_slice(&g(op.a));
                out.extend_from_slice(b"deg, ");
                out.extend_from_slice(&g(op.b));
                out.extend_from_slice(b"deg)");
            }
            MATRIX => {
                out.extend_from_slice(b"matrix(");
                for (k, v) in [op.a, op.b, op.c, op.d, op.e, op.f].into_iter().enumerate() {
                    if k > 0 {
                        out.extend_from_slice(b", ");
                    }
                    out.extend_from_slice(&g(v));
                }
                out.push(b')');
            }
            MATRIX3D => {
                out.extend_from_slice(b"matrix3d(");
                for (k, &v) in op.m3d.iter().enumerate() {
                    if k > 0 {
                        out.extend_from_slice(b", ");
                    }
                    out.extend_from_slice(&g(v));
                }
                out.push(b')');
            }
            ROTATE3D => {
                out.extend_from_slice(b"rotate3d(");
                for v in [op.a, op.b, op.c] {
                    out.extend_from_slice(&g(v));
                    out.extend_from_slice(b", ");
                }
                out.extend_from_slice(&g(op.d));
                out.extend_from_slice(b"deg)");
            }
            PERSPECTIVE => {
                out.extend_from_slice(b"perspective(");
                out.extend_from_slice(&g(op.a));
                out.extend_from_slice(b"px)");
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn is_3d(tf: &Transform) -> bool {
    tf.ops
        .iter()
        .take((tf.n_ops.max(0) as usize).min(OPS_MAX))
        .any(|op| match op.kind {
            TRANSLATE => op.c != 0.0 || axis_is_mixed(op, 2),
            SCALE => op.c != 0.0 && op.c != 1.0,
            ROTATE3D => op.d.abs() > 1e-12 && (op.a != 0.0 || op.b != 0.0),
            MATRIX3D | PERSPECTIVE => true,
            _ => false,
        })
}

pub(crate) fn to_mat4(tf: &Transform, bw: f64, bh: f64) -> Mat4 {
    let mut out = Mat4::IDENTITY;
    for i in 0..(tf.n_ops.max(0) as usize).min(OPS_MAX) {
        let mut op = tf.ops[i];
        let default = if op.kind == SCALE { 1.0 } else { 0.0 };
        for field in [
            &mut op.a,
            &mut op.b,
            &mut op.c,
            &mut op.d,
            &mut op.e,
            &mut op.f,
            &mut op.a_pct,
            &mut op.b_pct,
        ]
        .into_iter()
        .chain(op.em.iter_mut())
        .chain(op.rem.iter_mut())
        {
            if !field.is_finite() {
                *field = default;
            }
        }
        match op.kind {
            TRANSLATE => {
                let dx = (if op.a_is_percent != 0 {
                    op.a / 100.0 * bw
                } else {
                    op.a
                }) + op.a_pct / 100.0 * bw
                    + (op.em[0] + op.rem[0]) * 16.0;
                let dy = (if op.b_is_percent != 0 {
                    op.b / 100.0 * bh
                } else {
                    op.b
                }) + op.b_pct / 100.0 * bh
                    + (op.em[1] + op.rem[1]) * 16.0;
                let dz = op.c + (op.em[2] + op.rem[2]) * 16.0;
                out.translate(dx, dy, dz);
            }
            ROTATE => out.rotate_axis(0.0, 0.0, 1.0, op.a),
            ROTATE3D => out.rotate_axis(op.a, op.b, op.c, op.d),
            SCALE => out.scale(op.a, op.b, if op.c == 0.0 { 1.0 } else { op.c }),
            SKEW => out.skew(op.a, op.b),
            MATRIX => out.affine2d(op.a, op.b, op.c, op.d, op.e, op.f),
            MATRIX3D => {
                let mut t = Mat4 { m: [0.0; 16] };
                for row in 0..4 {
                    for col in 0..4 {
                        t.m[row * 4 + col] = op.m3d[col * 4 + row];
                    }
                }
                out = out.multiply(&t);
            }
            PERSPECTIVE => out.perspective(op.a),
            _ => {}
        }
    }
    out
}

pub(crate) fn is_math_fn_start(s: &[u8]) -> bool {
    [
        &b"calc("[..],
        b"min(",
        b"max(",
        b"clamp(",
        b"round(",
        b"mod(",
        b"rem(",
        b"abs(",
        b"sign(",
        b"hypot(",
        b"pow(",
        b"sqrt(",
        b"sin(",
        b"cos(",
        b"tan(",
        b"exp(",
        b"log(",
        b"atan2(",
        b"atan(",
        b"asin(",
        b"acos(",
        b"progress(",
    ]
    .iter()
    .any(|name| starts_with_ci(s, name))
}

fn calc_wrap(number: f64, nan: &[u8], neg_inf: &[u8], inf: &[u8], suffix: &[u8]) -> Vec<u8> {
    if number.is_nan() {
        return nan.to_vec();
    }
    if number.is_infinite() {
        return if number < 0.0 { neg_inf } else { inf }.to_vec();
    }
    let mut out = b"calc(".to_vec();
    out.extend_from_slice(&units::number_text(number));
    out.extend_from_slice(suffix);
    out.push(b')');
    out
}

fn serialize_calc_angle(deg: f64) -> Vec<u8> {
    calc_wrap(
        deg,
        b"calc(NaN * 1deg)",
        b"calc(-infinity * 1deg)",
        b"calc(infinity * 1deg)",
        b"deg",
    )
}

fn serialize_calc_number(n: f64) -> Vec<u8> {
    calc_wrap(n, b"calc(NaN)", b"calc(-infinity)", b"calc(infinity)", b"")
}

fn eval_calc_number(arg: &[u8]) -> Option<f64> {
    match calc::parse_calc(&c_text(arg))? {
        Parsed::Length(v, NUMBER) => Some(v),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ArgType {
    None,
    Angle,
    Number,
    Length,
}

fn arg_type(name: &[u8], idx: usize) -> ArgType {
    match name {
        b"rotate" | b"rotatex" | b"rotatey" | b"rotatez" | b"skewx" | b"skewy" => {
            if idx == 0 {
                ArgType::Angle
            } else {
                ArgType::None
            }
        }
        b"skew" => {
            if idx < 2 {
                ArgType::Angle
            } else {
                ArgType::None
            }
        }
        b"rotate3d" => match idx {
            3 => ArgType::Angle,
            0..=2 => ArgType::Number,
            _ => ArgType::None,
        },
        b"scale" | b"scalex" | b"scaley" | b"scalez" | b"scale3d" | b"matrix" | b"matrix3d" => {
            ArgType::Number
        }
        b"translate" | b"translatex" | b"translatey" | b"translatez" | b"translate3d"
        | b"perspective" => ArgType::Length,
        _ => ArgType::None,
    }
}

fn canonicalize_arg(arg: &[u8], kind: ArgType) -> Option<Vec<u8>> {
    match kind {
        ArgType::Angle => Some(serialize_calc_angle(parse_angle_any(arg)?)),
        ArgType::Number => {
            if units::has_relative_unit(arg) {
                return None;
            }
            Some(serialize_calc_number(eval_calc_number(arg)?))
        }
        ArgType::Length => {
            if units::has_relative_unit(arg) {
                return None;
            }
            let (ok, resolved) = calc::resolve_to_px_pct(arg, false);
            if !ok
                || !resolved.px.is_finite()
                || !resolved.pct.is_finite()
                || (resolved.px != 0.0 && resolved.pct != 0.0)
            {
                return None;
            }
            let mut out = b"calc(".to_vec();
            if resolved.pct != 0.0 {
                out.extend_from_slice(&units::number_text(resolved.pct));
                out.push(b'%');
            } else {
                out.extend_from_slice(&units::number_text(resolved.px));
                out.extend_from_slice(b"px");
            }
            out.push(b')');
            Some(out)
        }
        ArgType::None => None,
    }
}

pub(crate) fn transform_canonical(value: &[u8]) -> Option<Vec<u8>> {
    let value = &value[..value.iter().position(|&c| c == 0).unwrap_or(value.len())];
    let scan = &value[skip_ws(value, 0, value.len())..];
    if scan.is_empty() || starts_with_ci(scan, b"none") {
        return None;
    }
    let mut out = Vec::with_capacity(value.len());
    let mut changed = false;
    let mut p = 0;
    let end = value.len();
    while p < end {
        if is_ws(value[p]) || value[p] == b',' {
            out.push(value[p]);
            p += 1;
            continue;
        }
        let name_start = p;
        while p < end && value[p] != b'(' && !is_ws(value[p]) && value[p] != b',' {
            p += 1;
        }
        out.extend_from_slice(&value[name_start..p]);
        let name = value[name_start..p].to_ascii_lowercase();
        while p < end && is_ws(value[p]) {
            out.push(value[p]);
            p += 1;
        }
        if p >= end || value[p] != b'(' {
            continue;
        }
        out.push(b'(');
        p += 1;
        let args_start = p;
        let mut depth = 1;
        while p < end && depth > 0 {
            if value[p] == b'(' {
                depth += 1;
            } else if value[p] == b')' {
                depth -= 1;
            }
            if depth > 0 {
                p += 1;
            }
        }
        let args_end = p;
        let mut seg = args_start;
        let mut depth = 0i32;
        let mut index = 0;
        for q in args_start..=args_end {
            let c = value.get(q).copied().unwrap_or(0);
            if q < args_end && c == b'(' {
                depth += 1;
            } else if q < args_end && c == b')' {
                depth -= 1;
            }
            if q == args_end || (c == b',' && depth == 0) {
                let mut s = seg;
                while s < q && is_ws(value[s]) {
                    s += 1;
                }
                let mut e = q;
                while e > s && is_ws(value[e - 1]) {
                    e -= 1;
                }
                let arg = &value[s..e];
                let kind = arg_type(&name, index);
                let canon = if kind != ArgType::None && is_math_fn_start(arg) {
                    canonicalize_arg(arg, kind)
                } else {
                    None
                };
                match canon {
                    Some(canon) => {
                        out.extend_from_slice(&value[seg..s]);
                        out.extend_from_slice(&canon);
                        out.extend_from_slice(&value[e..q]);
                        changed = true;
                    }
                    None => out.extend_from_slice(&value[seg..q]),
                }
                if q < args_end {
                    out.push(b',');
                }
                index += 1;
                seg = q + 1;
            }
        }
        if p < end && value[p] == b')' {
            out.push(b')');
            p += 1;
        }
    }
    changed.then_some(out)
}

const TX_NUMBER: u32 = 1;
const TX_PERCENT: u32 = 2;
const TX_LENGTH: u32 = 4;
const TX_ANGLE: u32 = 8;

fn leading_zeros_text(arg: &[u8]) -> Vec<u8> {
    add_leading_zeros(arg)
}

fn math_canonical_or_zeros(arg: &[u8]) -> Vec<u8> {
    math::math_canonical(&c_text(arg)).unwrap_or_else(|| leading_zeros_text(arg))
}

fn arg_canonical(arg: &[u8], want: u32, scale_percent: bool) -> Option<Vec<u8>> {
    if is_math_fn_start(arg) {
        let legacy = if want & TX_ANGLE != 0 {
            ArgType::Angle
        } else if want & TX_LENGTH != 0 {
            ArgType::Length
        } else if want & TX_NUMBER != 0 && !scale_percent {
            ArgType::Number
        } else {
            ArgType::None
        };
        if legacy != ArgType::None {
            if let Some(canon) = canonicalize_arg(arg, legacy) {
                return Some(canon);
            }
            if want & TX_PERCENT == 0 || want & TX_ANGLE != 0 {
                let probe = calc::parse_calc(&c_text(arg)).is_some();
                let angle_ok = want & TX_ANGLE != 0 && parse_angle_any(arg).is_some();
                if !probe && !angle_ok {
                    return None;
                }
                return Some(leading_zeros_text(arg));
            }
        }
        if scale_percent && want & TX_NUMBER != 0 {
            if units::has_relative_unit(arg) {
                return Some(leading_zeros_text(arg));
            }
            if let Some(n) = eval_calc_number(arg) {
                return Some(serialize_calc_number(n));
            }
            let (ok, resolved) = calc::resolve_to_px_pct(arg, false);
            if ok && resolved.px == 0.0 {
                return Some(math_canonical_or_zeros(arg));
            }
            return None;
        }
        if let Some(parsed) = calc::parse_calc(&c_text(arg)) {
            let pct = matches!(parsed, Parsed::Calc(c) if c.pct != 0.0);
            let number = matches!(parsed, Parsed::Length(_, NUMBER));
            if want & TX_LENGTH != 0 || want & TX_PERCENT != 0 || (want & TX_NUMBER != 0 && number)
            {
                if pct && want & TX_PERCENT == 0 {
                    return None;
                }
                return Some(math_canonical_or_zeros(arg));
            }
        }
        if want & TX_ANGLE != 0 && parse_angle_any(arg).is_some() {
            return Some(math_canonical_or_zeros(arg));
        }
        if want & TX_NUMBER != 0 {
            if let Some(n) = eval_calc_number(arg) {
                return Some(serialize_calc_number(n));
            }
        }
        return None;
    }
    let text = c_text(arg);
    let bytes = text.to_bytes();
    if want & TX_ANGLE != 0 {
        let (num, end) = ffi::strtod(&text, 0);
        let rest = &bytes[end..];
        if end != 0
            && (rest.is_empty()
                || [&b"deg"[..], b"grad", b"rad", b"turn"]
                    .iter()
                    .any(|unit| rest.eq_ignore_ascii_case(unit)))
        {
            if rest.is_empty() && num != 0.0 {
                return None;
            }
            if rest.is_empty() {
                return Some(b"0deg".to_vec());
            }
            return Some(normalize_negative_zero(&add_leading_zeros(bytes)).to_ascii_lowercase());
        }
        if want & !TX_ANGLE == 0 {
            return None;
        }
    }
    let (num, unit) = units::parse_length(&text)?;
    if unit == NUMBER {
        if want & TX_NUMBER != 0 {
            return Some(units::number_text(num));
        }
        if num == 0.0 && want & TX_LENGTH != 0 {
            return Some(b"0px".to_vec());
        }
        return None;
    }
    if unit == PERCENT {
        if scale_percent && want & TX_NUMBER != 0 {
            return Some(units::number_text(num / 100.0));
        }
        if want & TX_PERCENT == 0 {
            return None;
        }
    } else if want & TX_LENGTH == 0 {
        return None;
    }
    let mut canon = normalize_negative_zero(&add_leading_zeros(bytes));
    let suffix = canon.len()
        - canon
            .iter()
            .rev()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
    canon[suffix..].make_ascii_lowercase();
    Some(canon)
}

struct Spec {
    name: &'static [u8],
    min: usize,
    max: usize,
    types: &'static [u32],
}

const LP: u32 = TX_LENGTH | TX_PERCENT;
const N16: [u32; 16] = [TX_NUMBER; 16];

const SPECS: &[Spec] = &[
    Spec {
        name: b"translate",
        min: 1,
        max: 2,
        types: &[LP, LP],
    },
    Spec {
        name: b"translatex",
        min: 1,
        max: 1,
        types: &[LP],
    },
    Spec {
        name: b"translatey",
        min: 1,
        max: 1,
        types: &[LP],
    },
    Spec {
        name: b"translatez",
        min: 1,
        max: 1,
        types: &[TX_LENGTH],
    },
    Spec {
        name: b"translate3d",
        min: 3,
        max: 3,
        types: &[LP, LP, TX_LENGTH],
    },
    Spec {
        name: b"scale",
        min: 1,
        max: 2,
        types: &[TX_NUMBER, TX_NUMBER],
    },
    Spec {
        name: b"scalex",
        min: 1,
        max: 1,
        types: &[TX_NUMBER],
    },
    Spec {
        name: b"scaley",
        min: 1,
        max: 1,
        types: &[TX_NUMBER],
    },
    Spec {
        name: b"scalez",
        min: 1,
        max: 1,
        types: &[TX_NUMBER],
    },
    Spec {
        name: b"scale3d",
        min: 3,
        max: 3,
        types: &[TX_NUMBER; 3],
    },
    Spec {
        name: b"rotate",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"rotatex",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"rotatey",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"rotatez",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"rotate3d",
        min: 4,
        max: 4,
        types: &[TX_NUMBER, TX_NUMBER, TX_NUMBER, TX_ANGLE],
    },
    Spec {
        name: b"skew",
        min: 1,
        max: 2,
        types: &[TX_ANGLE, TX_ANGLE],
    },
    Spec {
        name: b"skewx",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"skewy",
        min: 1,
        max: 1,
        types: &[TX_ANGLE],
    },
    Spec {
        name: b"matrix",
        min: 6,
        max: 6,
        types: &[TX_NUMBER; 6],
    },
    Spec {
        name: b"matrix3d",
        min: 16,
        max: 16,
        types: &N16,
    },
    Spec {
        name: b"perspective",
        min: 1,
        max: 1,
        types: &[TX_LENGTH],
    },
];

fn function_canonical(name: &[u8], args: &[Vec<u8>]) -> Option<Vec<u8>> {
    let spec = SPECS.iter().find(|spec| spec.name == name)?;
    let n = args.len();
    if n < spec.min || n > spec.max {
        return None;
    }
    let mut out = name.to_vec();
    out.push(b'(');
    let scale = name.starts_with(b"scale");
    for (i, arg) in args.iter().enumerate() {
        let canon = if name == b"perspective" && arg.eq_ignore_ascii_case(b"none") {
            b"none".to_vec()
        } else {
            arg_canonical(arg, spec.types[i], scale)?
        };
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(&canon);
    }
    out.push(b')');
    Some(out)
}

pub(crate) fn list_canonical(value: &[u8]) -> Option<Vec<u8>> {
    let mut p = skip_ws(value, 0, value.len());
    if p >= value.len() {
        return None;
    }
    if value[p..].eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    let mut out = Vec::new();
    while p < value.len() {
        p = skip_ws(value, p, value.len());
        if p >= value.len() {
            break;
        }
        let name_start = p;
        while p < value.len()
            && (value[p].is_ascii_alphanumeric() || value[p] == b'-' || value[p] == b'_')
        {
            p += 1;
        }
        if p >= value.len() || value[p] != b'(' || p == name_start {
            return None;
        }
        let written = &value[name_start..p];
        let name = written.to_ascii_lowercase();
        p += 1;
        let args_start = p;
        let mut depth = 1;
        while p < value.len() && depth > 0 {
            if value[p] == b'(' {
                depth += 1;
            } else if value[p] == b')' {
                depth -= 1;
            }
            if depth > 0 {
                p += 1;
            }
        }
        if depth != 0 {
            return None;
        }
        let parts = split_top_level_commas(&value[args_start..p]);
        p += 1;
        let parts: &[Vec<u8>] = if parts.len() == 1 && parts[0].is_empty() {
            &[]
        } else {
            &parts
        };
        let canon = function_canonical(&name, parts)?;
        if !out.is_empty() {
            out.push(b' ');
        }
        if name.starts_with(b"translate") {
            out.extend_from_slice(written);
            out.extend_from_slice(&canon[name.len()..]);
        } else {
            out.extend_from_slice(&canon);
        }
        p = skip_ws(value, p, value.len());
        if p < value.len() && value[p] == b',' {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn keep_join(parts: &[Vec<u8>], keep: usize) -> Vec<u8> {
    parts[..keep].join(&b' ')
}

pub(crate) fn individual_canonical(value: &[u8], prop: Individual) -> Option<Vec<u8>> {
    let value = &value[skip_ws(value, 0, value.len())..];
    if value.eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    let tokens = split_ws_limit(value, 5);
    let n = tokens.len();
    match prop {
        Individual::Translate if (1..=3).contains(&n) => {
            let mut c = Vec::new();
            for (i, tok) in tokens.iter().enumerate() {
                c.push(arg_canonical(
                    tok,
                    if i == 2 { TX_LENGTH } else { LP },
                    false,
                )?);
            }
            let mut keep = n;
            if keep == 3 && c[2] == b"0px" {
                keep = 2;
            }
            if keep == 2 && c[1] == b"0px" {
                keep = 1;
            }
            Some(keep_join(&c, keep))
        }
        Individual::Scale if (1..=3).contains(&n) => {
            let mut c = Vec::new();
            for tok in &tokens {
                c.push(arg_canonical(tok, TX_NUMBER, true)?);
            }
            let mut keep = n;
            if keep == 3 && c[2] == b"1" {
                keep = 2;
            }
            if keep == 2 && c[0] == c[1] {
                keep = 1;
            }
            Some(keep_join(&c, keep))
        }
        Individual::Rotate if n == 1 || n == 2 || n == 4 => rotate_canonical(&tokens),
        _ => None,
    }
}

fn rotate_canonical(tokens: &[&[u8]]) -> Option<Vec<u8>> {
    let n = tokens.len();
    let mut angle: Option<Vec<u8>> = None;
    let mut axis_kw: Option<&[u8]> = None;
    let mut vec = [0.0, 0.0, 1.0];
    let mut n_vec = 0;
    let mut have_vec = false;
    for (i, &tok) in tokens.iter().enumerate() {
        if n == 2
            && (tok.eq_ignore_ascii_case(b"x")
                || tok.eq_ignore_ascii_case(b"y")
                || tok.eq_ignore_ascii_case(b"z"))
        {
            if axis_kw.is_some() {
                return None;
            }
            axis_kw = Some(tok);
            continue;
        }
        if n == 4 && n_vec < 3 && (i == 0 || n_vec > 0 || angle.is_some()) {
            let text = c_text(tok);
            let (num, end) = ffi::strtod(&text, 0);
            if end != 0 && end == text.to_bytes().len() {
                vec[n_vec] = num;
                n_vec += 1;
                if n_vec == 3 {
                    have_vec = true;
                }
                continue;
            }
        }
        if angle.is_some() {
            return None;
        }
        angle = Some(arg_canonical(tok, TX_ANGLE, false)?);
    }
    let angle = angle?;
    if (n == 2 && axis_kw.is_none()) || (n == 4 && !have_vec) {
        return None;
    }
    let mut axis: Option<&[u8]> = None;
    let mut flip = false;
    let mut numeric = false;
    if let Some(kw) = axis_kw {
        axis = match kw[0].to_ascii_lowercase() {
            b'x' => Some(b"x"),
            b'y' => Some(b"y"),
            _ => None,
        };
    } else if have_vec {
        if vec[1] == 0.0 && vec[2] == 0.0 && vec[0] != 0.0 {
            axis = Some(b"x");
            flip = vec[0] < 0.0;
        } else if vec[0] == 0.0 && vec[2] == 0.0 && vec[1] != 0.0 {
            axis = Some(b"y");
            flip = vec[1] < 0.0;
        } else if vec[0] == 0.0 && vec[1] == 0.0 && vec[2] != 0.0 {
            flip = vec[2] < 0.0;
        } else {
            numeric = true;
        }
    }
    let mut out = Vec::new();
    if numeric {
        for v in vec {
            out.extend_from_slice(&units::number_text(v));
            out.push(b' ');
        }
    } else if let Some(axis) = axis {
        out.extend_from_slice(axis);
        out.push(b' ');
    }
    if flip && angle != b"0deg" {
        if angle.first() == Some(&b'-') {
            out.extend_from_slice(&angle[1..]);
        } else {
            out.push(b'-');
            out.extend_from_slice(&angle);
        }
    } else {
        out.extend_from_slice(&angle);
    }
    Some(out)
}

pub(crate) fn origin_canonical(value: &[u8], two_only: bool) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(value, 5);
    let n = tokens.len();
    if two_only && (n == 3 || n == 4) {
        return position::canonical_ex(value, true, false);
    }
    if n < 1 || n > if two_only { 2 } else { 3 } {
        return None;
    }
    let a_h = position::is_h_edge(tokens[0]);
    let a_v = position::is_v_edge(tokens[0]);
    let a_c = tokens[0].eq_ignore_ascii_case(b"center");
    let (x, y): (&[u8], &[u8]) = if n == 1 {
        if a_v {
            (b"center", tokens[0])
        } else {
            (tokens[0], b"center")
        }
    } else {
        let b_h = position::is_h_edge(tokens[1]);
        let b_v = position::is_v_edge(tokens[1]);
        let b_c = tokens[1].eq_ignore_ascii_case(b"center");
        let a_kw = a_h || a_v || a_c;
        let b_kw = b_h || b_v || b_c;
        if a_kw && b_kw {
            if (a_h && b_h) || (a_v && b_v) {
                return None;
            }
            if a_v || (a_c && b_h) {
                (tokens[1], tokens[0])
            } else {
                (tokens[0], tokens[1])
            }
        } else if a_kw {
            if a_v {
                return None;
            }
            (tokens[0], tokens[1])
        } else if b_kw {
            if b_h {
                return None;
            }
            (tokens[0], tokens[1])
        } else {
            (tokens[0], tokens[1])
        }
    };
    let xc = if position::is_h_edge(x) || x.eq_ignore_ascii_case(b"center") {
        Some(x.to_ascii_lowercase())
    } else {
        arg_canonical(x, LP, false)
    };
    let yc = if position::is_v_edge(y) || y.eq_ignore_ascii_case(b"center") {
        Some(y.to_ascii_lowercase())
    } else {
        arg_canonical(y, LP, false)
    };
    let (xc, yc) = (xc?, yc?);
    let mut out = xc;
    out.push(b' ');
    out.extend_from_slice(&yc);
    if n == 3 {
        let zc = arg_canonical(tokens[2], TX_LENGTH, false)?;
        out.push(b' ');
        out.extend_from_slice(&zc);
    }
    Some(out)
}
