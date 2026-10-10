//! Southstar — computed values as text, interpolated between two values for transitions and animations, and compared by their text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::animation::{self, Entry};
use crate::calc::Calc;
use crate::color::{color_text, parse_color};
use crate::display;
use crate::ffi;
use crate::gradient;
use crate::grid::{
    Areas, LineName, TRACK_AUTO, TRACK_FR, TRACK_MAX_CONTENT, TRACK_MIN_CONTENT, TRACK_PERCENT,
    TRACK_PX, Tracks,
};
use crate::math::math_canonical;
use crate::prop::Prop;
use crate::property::{self, text_is_ident};
use crate::property::{Body, Rect, Size, Value};
use crate::shadow::{self, ShadowList};
use crate::shorthand::border_radius_canonical;
use crate::time;
use crate::transform::{
    self, Individual, MATRIX, MATRIX3D, Op, ROTATE, ROTATE3D, SCALE, SKEW, TRANSLATE, Transform,
};
use crate::units::{self, EM, NUMBER, PERCENT, PX};

fn g(v: f64) -> Vec<u8> {
    ffi::format_double(c"%g", v)
}

fn g_unit(v: f64, unit: u32) -> Vec<u8> {
    let mut out = g(v);
    out.extend_from_slice(units::unit_suffix(unit).to_bytes());
    out
}

fn size_text(s: &Size) -> Vec<u8> {
    if s.w_unit == NUMBER && s.h_unit == NUMBER && !s.h_auto {
        let mut out = if s.w_auto {
            b"auto ".to_vec()
        } else {
            Vec::new()
        };
        out.extend_from_slice(&g(s.w));
        out.extend_from_slice(b" / ");
        out.extend_from_slice(&g(s.h));
        return out;
    }
    let mut out = if s.w_auto {
        b"auto".to_vec()
    } else {
        g_unit(s.w, s.w_unit)
    };
    out.push(b' ');
    out.extend_from_slice(&if s.h_auto {
        b"auto".to_vec()
    } else {
        g_unit(s.h, s.h_unit)
    });
    out
}

fn calc_text(c: &Calc) -> Vec<u8> {
    let parts = [(c.pct, "%"), (c.em, "em"), (c.px, "px"), (c.rem, "rem")];
    let used: Vec<&(f64, &str)> = parts.iter().filter(|(v, _)| *v != 0.0).collect();
    match used.as_slice() {
        [] => b"0px".to_vec(),
        [(v, unit)] => [g(*v), unit.as_bytes().to_vec()].concat(),
        _ => {
            let mut out = b"calc(".to_vec();
            for (i, (v, unit)) in used.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(if *v < 0.0 { b" - " } else { b" + " });
                }
                out.extend_from_slice(&g(if i == 0 { *v } else { v.abs() }));
                out.extend_from_slice(unit.as_bytes());
            }
            out.push(b')');
            out
        }
    }
}

fn line_name(name: &LineName) -> &[u8] {
    let len = name
        .name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(name.name.len());
    &name.name[..len]
}

fn tracks_text(tracks: &Tracks, specified: Option<&[u8]>) -> Vec<u8> {
    if tracks.subgrid != 0 {
        return specified.unwrap_or(b"subgrid").to_vec();
    }
    let n = usize::try_from(tracks.n)
        .unwrap_or(0)
        .min(tracks.tracks.len());
    let n_names = usize::try_from(tracks.n_line_names).unwrap_or(0);
    let names = &tracks.line_names[..n_names.min(tracks.line_names.len())];
    let mut out = Vec::new();
    for i in 0..=n {
        let mut open = false;
        for name in names.iter().filter(|name| name.line == i as i32 + 1) {
            out.extend_from_slice(if open {
                b" "
            } else if out.is_empty() {
                b"["
            } else {
                b" ["
            });
            out.extend_from_slice(line_name(name));
            open = true;
        }
        if open {
            out.push(b']');
        }
        if i == n {
            break;
        }
        if !out.is_empty() {
            out.push(b' ');
        }
        let track = &tracks.tracks[i];
        match track.kind {
            TRACK_PX => out.extend_from_slice(&[g(track.v), b"px".to_vec()].concat()),
            TRACK_PERCENT => out.extend_from_slice(&[g(track.v), b"%".to_vec()].concat()),
            TRACK_FR => out.extend_from_slice(&[g(track.v), b"fr".to_vec()].concat()),
            TRACK_AUTO => out.extend_from_slice(b"auto"),
            TRACK_MIN_CONTENT => out.extend_from_slice(b"min-content"),
            TRACK_MAX_CONTENT => out.extend_from_slice(b"max-content"),
            _ => {}
        }
    }
    out
}

fn areas_text(areas: &Areas) -> Vec<u8> {
    let mut out = Vec::new();
    for r in 0..areas.rows.max(0) {
        if r > 0 {
            out.push(b' ');
        }
        out.push(b'"');
        for c in 0..areas.cols.max(0) {
            let name = areas
                .rects
                .iter()
                .find(|rect| r >= rect.r0 && r <= rect.r1 && c >= rect.c0 && c <= rect.c1)
                .map_or(&b"."[..], |rect| &rect.name);
            if c > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(name);
        }
        out.push(b'"');
    }
    out
}

fn anim_text(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        if let Some(name) = &e.name {
            out.extend_from_slice(name);
            out.push(b' ');
        }
        out.extend_from_slice(&g(e.duration_ms));
        out.extend_from_slice(b"ms");
        if e.delay_ms != 0.0 {
            out.push(b' ');
            out.extend_from_slice(&g(e.delay_ms));
            out.extend_from_slice(b"ms");
        }
    }
    out
}

fn rect_text(r: &Rect) -> Vec<u8> {
    let mut out = b"rect(".to_vec();
    for i in 0..4 {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        if r.is_auto[i] {
            out.extend_from_slice(b"auto");
        } else {
            out.extend_from_slice(&g(r.v[i]));
            out.extend_from_slice(b"px");
        }
    }
    out.push(b')');
    out
}

pub(crate) fn serialize_one(v: &Value) -> Vec<u8> {
    match &v.body {
        Body::Keyword(text) => text.clone(),
        Body::Color(rgba) => color_text(*rgba),
        Body::Length(n, unit) => g_unit(*n, *unit),
        Body::Size(s) => size_text(s),
        Body::Calc(c) => calc_text(c),
        Body::Shadow(list) => shadow::serialize(list),
        Body::Gradient(gr) => gradient::serialize_computed(gr),
        Body::Tracks(tracks) => tracks_text(tracks, v.specified.as_deref()),
        Body::Url(url) => match &v.image_set_text {
            Some(text) => text.clone(),
            None => [b"url(\"".as_slice(), url, b"\")"].concat(),
        },
        Body::Areas(areas) => areas_text(areas),
        Body::Anim(entries) => anim_text(entries),
        Body::Transform(tf) => transform::serialize(tf),
        Body::Rect(r) => rect_text(r),
    }
}

pub(crate) fn serialize(v: &Value, specified: bool) -> Vec<u8> {
    if v.next_layer.is_none() && !(specified && v.specified.is_some()) {
        return serialize_one(v);
    }
    let mut out = Vec::new();
    let mut layer = Some(v);
    while let Some(l) = layer {
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        match (&l.specified, specified) {
            (Some(text), true) => out.extend_from_slice(text),
            _ => out.extend_from_slice(&serialize_one(l)),
        }
        layer = l.next_layer.as_deref();
    }
    out
}

enum Linear {
    Length(f64, u32),
    Mixed { px: f64, pct: f64, em: f64 },
}

fn linear_parts(v: &Body) -> Option<(f64, f64, f64)> {
    match v {
        Body::Calc(c) if c.func != 0 => None,
        Body::Calc(c) => Some((c.px, c.pct, c.em)),
        Body::Length(n, PX) => Some((*n, 0.0, 0.0)),
        Body::Length(n, PERCENT) => Some((0.0, *n, 0.0)),
        Body::Length(n, EM) => Some((0.0, 0.0, *n)),
        _ => None,
    }
}

fn lerp_lengths(a: &Body, b: &Body, t: f64) -> Option<Linear> {
    if let (Body::Length(av, au), Body::Length(bv, bu)) = (a, b) {
        if au == bu {
            return Some(Linear::Length(av + (bv - av) * t, *au));
        }
        if *av == 0.0 && *au != PERCENT && *bu != NUMBER && *bu != PERCENT {
            return Some(Linear::Length(bv * t, *bu));
        }
        if *bv == 0.0 && *bu != PERCENT && *au != NUMBER && *au != PERCENT {
            return Some(Linear::Length(av * (1.0 - t), *au));
        }
        return None;
    }
    if !matches!(a, Body::Length(..) | Body::Calc(_))
        || !matches!(b, Body::Length(..) | Body::Calc(_))
    {
        return None;
    }
    let (apx, apct, aem) = linear_parts(a)?;
    let (bpx, bpct, bem) = linear_parts(b)?;
    Some(Linear::Mixed {
        px: apx + (bpx - apx) * t,
        pct: apct + (bpct - apct) * t,
        em: aem + (bem - aem) * t,
    })
}

fn lerp_channel(a: u8, b: u8, t: f64) -> u8 {
    let v = (f64::from(a) + (f64::from(b) - f64::from(a)) * t).clamp(0.0, 255.0);
    (v + 0.5) as u8
}

fn c_text(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
}

fn keyword_number(kw: &[u8]) -> Option<(f64, bool)> {
    match kw {
        b"" => None,
        b"bold" => Some((700.0, true)),
        b"normal" => Some((400.0, true)),
        _ => {
            let text = c_text(kw);
            let (v, end) = ffi::strtod(&text, 0);
            (end != 0 && end == kw.len()).then(|| (v, !kw.contains(&b'.') && !kw.contains(&b'e')))
        }
    }
}

fn keyword_lerp_list(ka: &[u8], kb: &[u8], t: f64) -> Option<Vec<u8>> {
    let pa: Vec<&[u8]> = ka.split(|&c| c == b' ').collect();
    let pb: Vec<&[u8]> = kb.split(|&c| c == b' ').collect();
    if pa.len() != pb.len() || pa.len() <= 1 {
        return None;
    }
    let mut out = Vec::new();
    for (a, b) in pa.into_iter().zip(pb) {
        if a.is_empty() || b.is_empty() {
            if a.is_empty() && b.is_empty() {
                continue;
            }
            return None;
        }
        let part = keyword_lerp(a, b, t)?;
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(&part);
    }
    Some(out)
}

fn keyword_lerp(ka: &[u8], kb: &[u8], t: f64) -> Option<Vec<u8>> {
    if ka.contains(&b' ') || kb.contains(&b' ') {
        return keyword_lerp_list(ka, kb, t);
    }
    if let (Some((na, ia)), Some((nb, ib))) = (keyword_number(ka), keyword_number(kb)) {
        let mut r = na + (nb - na) * t;
        if ia && ib {
            r = r.round();
        }
        return Some(g(r));
    }
    let (Some((va, ua)), Some((vb, ub))) = (
        units::parse_length(&c_text(ka)),
        units::parse_length(&c_text(kb)),
    ) else {
        return None;
    };
    if ua != ub || ua == NUMBER {
        return None;
    }
    let unit_start = ka
        .iter()
        .rposition(|&c| c.is_ascii_digit() || c == b'.')
        .map_or(0, |i| i + 1);
    Some([g(va + (vb - va) * t), ka[unit_start..].to_vec()].concat())
}

fn translate_axis_lerp(x: &Op, y: &Op, t: f64, axis: usize, o: &mut Op) {
    let (xp, yp) = if axis == 0 {
        (x.a_is_percent != 0, y.a_is_percent != 0)
    } else {
        (x.b_is_percent != 0, y.b_is_percent != 0)
    };
    let (xv, yv) = if axis == 0 { (x.a, y.a) } else { (x.b, y.b) };
    let (x_extra, y_extra) = if axis == 0 {
        (x.a_pct, y.a_pct)
    } else {
        (x.b_pct, y.b_pct)
    };
    let xpct = (if xp { xv } else { 0.0 }) + x_extra;
    let ypct = (if yp { yv } else { 0.0 }) + y_extra;
    let xpx = if xp { 0.0 } else { xv };
    let ypx = if yp { 0.0 } else { yv };
    let pure_percent = xp && yp;
    let v = if pure_percent {
        xv + (yv - xv) * t
    } else {
        xpx + (ypx - xpx) * t
    };
    let pct = if pure_percent {
        0.0
    } else {
        xpct + (ypct - xpct) * t
    };
    let flag = i32::from(pure_percent);
    if axis == 0 {
        o.a = v;
        o.a_is_percent = flag;
        o.a_pct = pct;
    } else {
        o.b = v;
        o.b_is_percent = flag;
        o.b_pct = pct;
    }
}

fn identity_like(tf: &Transform) -> Option<Transform> {
    let mut out = *tf;
    let n = usize::try_from(out.n_ops).unwrap_or(0);
    for op in &mut out.ops[..n] {
        match op.kind {
            TRANSLATE => {
                op.a = 0.0;
                op.b = 0.0;
                op.c = 0.0;
                op.a_pct = 0.0;
                op.b_pct = 0.0;
                op.em = [0.0; 3];
                op.rem = [0.0; 3];
            }
            ROTATE => op.a = 0.0,
            ROTATE3D => op.d = 0.0,
            SCALE => {
                op.a = 1.0;
                op.b = 1.0;
                op.c = 1.0;
            }
            SKEW => {
                op.a = 0.0;
                op.b = 0.0;
            }
            MATRIX => {
                op.a = 1.0;
                op.b = 0.0;
                op.c = 0.0;
                op.d = 1.0;
                op.e = 0.0;
                op.f = 0.0;
            }
            MATRIX3D => {
                for (k, m) in op.m3d.iter_mut().enumerate() {
                    *m = if k % 5 == 0 { 1.0 } else { 0.0 };
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn transform_lerp(ta: &Transform, tb: &Transform, t: f64) -> Option<Transform> {
    if ta.n_ops != tb.n_ops {
        return None;
    }
    let n = usize::try_from(ta.n_ops).unwrap_or(0);
    if ta.ops[..n]
        .iter()
        .zip(&tb.ops[..n])
        .any(|(x, y)| x.kind != y.kind)
    {
        return None;
    }
    let mut out = Transform {
        n_ops: ta.n_ops,
        ..Transform::default()
    };
    for i in 0..n {
        let (x, y) = (&ta.ops[i], &tb.ops[i]);
        let mut o = *x;
        o.a = lerp(x.a, y.a, t);
        o.b = lerp(x.b, y.b, t);
        o.c = lerp(x.c, y.c, t);
        o.d = lerp(x.d, y.d, t);
        o.e = lerp(x.e, y.e, t);
        o.f = lerp(x.f, y.f, t);
        for k in 0..16 {
            o.m3d[k] = lerp(x.m3d[k], y.m3d[k], t);
        }
        if x.kind == TRANSLATE {
            translate_axis_lerp(x, y, t, 0, &mut o);
            translate_axis_lerp(x, y, t, 1, &mut o);
            for k in 0..3 {
                o.em[k] = lerp(x.em[k], y.em[k], t);
                o.rem[k] = lerp(x.rem[k], y.rem[k], t);
            }
        }
        out.ops[i] = o;
    }
    Some(out)
}

fn shadow_lerp(sa: &ShadowList, sb: &ShadowList, t: f64) -> Option<ShadowList> {
    if sa.n != sb.n || sa.is_text != sb.is_text {
        return None;
    }
    let n = usize::try_from(sa.n).unwrap_or(0);
    if sa.s[..n]
        .iter()
        .zip(&sb.s[..n])
        .any(|(x, y)| x.inset != y.inset)
    {
        return None;
    }
    let mut out = ShadowList {
        n: sa.n,
        is_text: sa.is_text,
        ..ShadowList::default()
    };
    for i in 0..n {
        let (x, y) = (&sa.s[i], &sb.s[i]);
        let o = &mut out.s[i];
        o.x = lerp(x.x, y.x, t);
        o.y = lerp(x.y, y.y, t);
        o.blur = lerp(x.blur, y.blur, t);
        o.spread = lerp(x.spread, y.spread, t);
        o.r = lerp_channel(x.r, y.r, t);
        o.g = lerp_channel(x.g, y.g, t);
        o.b = lerp_channel(x.b, y.b, t);
        o.a = lerp_channel(x.a, y.a, t);
        o.inset = x.inset;
    }
    Some(out)
}

fn rect_lerp(ra: &Rect, rb: &Rect, t: f64) -> Option<Rect> {
    let mut out = Rect::default();
    for i in 0..4 {
        if ra.is_auto[i] != rb.is_auto[i] || (!ra.is_auto[i] && ra.unit[i] != rb.unit[i]) {
            return None;
        }
        out.is_auto[i] = ra.is_auto[i];
        out.unit[i] = ra.unit[i];
        out.v[i] = lerp(ra.v[i], rb.v[i], t);
    }
    Some(out)
}

fn is_none_keyword(v: &Value) -> bool {
    matches!(&v.body, Body::Keyword(k) if k == b"none")
}

pub(crate) fn interpolate(a: &Value, b: &Value, t: f64) -> Option<Value> {
    if let (true, Body::Transform(tb)) = (is_none_keyword(a), &b.body) {
        let identity = Value::of(Body::Transform(Box::new(identity_like(tb)?)));
        return interpolate(&identity, b, t);
    }
    if let (Body::Transform(ta), true) = (&a.body, is_none_keyword(b)) {
        let identity = Value::of(Body::Transform(Box::new(identity_like(ta)?)));
        return interpolate(a, &identity, t);
    }
    if let Some(linear) = lerp_lengths(&a.body, &b.body, t) {
        return Some(Value::of(match linear {
            Linear::Length(v, unit) => Body::Length(v, unit),
            Linear::Mixed { px, pct, em } => Body::Calc(Calc {
                px,
                pct,
                em,
                ..Calc::default()
            }),
        }));
    }
    let body = match (&a.body, &b.body) {
        (Body::Keyword(ka), Body::Keyword(kb)) => Body::Keyword(keyword_lerp(ka, kb, t)?),
        (Body::Rect(ra), Body::Rect(rb)) => Body::Rect(rect_lerp(ra, rb, t)?),
        (Body::Color(ca), Body::Color(cb)) => {
            Body::Color(core::array::from_fn(|i| lerp_channel(ca[i], cb[i], t)))
        }
        (Body::Shadow(sa), Body::Shadow(sb)) => Body::Shadow(Box::new(shadow_lerp(sa, sb, t)?)),
        (Body::Transform(ta), Body::Transform(tb)) => {
            Body::Transform(Box::new(transform_lerp(ta, tb, t)?))
        }
        _ => return None,
    };
    Some(Value::of(body))
}

const COLOR_PROPS: [&[u8]; 22] = [
    b"color",
    b"background-color",
    b"border-top-color",
    b"border-right-color",
    b"border-bottom-color",
    b"border-left-color",
    b"border-color",
    b"outline-color",
    b"text-decoration-color",
    b"column-rule-color",
    b"caret-color",
    b"accent-color",
    b"fill",
    b"stroke",
    b"stop-color",
    b"flood-color",
    b"lighting-color",
    b"text-emphasis-color",
    b"border-block-start-color",
    b"border-block-end-color",
    b"border-inline-start-color",
    b"border-inline-end-color",
];

const KEYWORD_CANONICAL_PROPS: [&[u8]; 12] = [
    b"animation-range-start",
    b"animation-range-end",
    b"animation-timeline",
    b"animation-name",
    b"transition-property",
    b"animation-timing-function",
    b"transition-timing-function",
    b"counter-reset",
    b"counter-increment",
    b"counter-set",
    b"list-style-type",
    b"overflow-clip-margin",
];

fn starts_ci(text: &[u8], prefix: &[u8]) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn has(text: &[u8], needle: &[u8]) -> bool {
    text.windows(needle.len()).any(|w| w == needle)
}

fn parsed_text(prop: Option<Prop>, value: &[u8]) -> Option<Vec<u8>> {
    property::parse_for(Some(prop?), value).map(|v| serialize(&v, false))
}

fn color_canonical(value: &[u8]) -> Option<Vec<u8>> {
    let text = c_text(value);
    if text_is_ident(value) {
        let known = parse_color(&text).is_some() || value.eq_ignore_ascii_case(b"currentcolor");
        return known.then(|| value.to_ascii_lowercase());
    }
    let functional = value.first() == Some(&b'#')
        || starts_ci(value, b"rgb")
        || starts_ci(value, b"hsl")
        || starts_ci(value, b"hwb(");
    if !functional || has(value, b"var(") || has(value, b"calc(") || has(value, b"none") {
        return None;
    }
    parse_color(&text).map(color_text)
}

pub(crate) fn specified_canonical(prop: Option<&[u8]>, value: &[u8]) -> Option<Vec<u8>> {
    let Some(prop) = prop else {
        return math_canonical(&c_text(value));
    };
    if prop == b"display"
        && let Some(text) = display::canonical(value)
    {
        return Some(text);
    }
    if prop == b"transform"
        && let Some(text) =
            transform::list_canonical(value).or_else(|| transform::transform_canonical(value))
    {
        return Some(text);
    }
    let individual = match prop {
        b"scale" => Some(Individual::Scale),
        b"rotate" => Some(Individual::Rotate),
        b"translate" => Some(Individual::Translate),
        _ => None,
    };
    if let Some(text) = individual.and_then(|which| transform::individual_canonical(value, which)) {
        return Some(text);
    }
    if (prop == b"transform-origin" || prop == b"perspective-origin")
        && let Some(text) = transform::origin_canonical(value, prop[0] == b'p')
    {
        return Some(text);
    }
    if (prop == b"border-radius" || prop == b"-webkit-border-radius")
        && let Some(text) = border_radius_canonical(value)
    {
        return Some(text);
    }
    if (prop == b"animation" || prop == b"transition")
        && let Some(text) = animation::shorthand_canonical(value, prop[0] == b'a')
    {
        return Some(text);
    }
    if KEYWORD_CANONICAL_PROPS.contains(&prop) {
        let parsed = ffi::prop_named(prop).and_then(|id| property::parse(id, value));
        if let Some(Value {
            body: Body::Keyword(text),
            ..
        }) = parsed
        {
            return Some(text);
        }
    }
    if COLOR_PROPS.contains(&prop)
        && let Some(text) = color_canonical(value)
    {
        return Some(text);
    }
    if matches!(
        prop,
        b"background-clip" | b"background-origin" | b"background-attachment" | b"background-repeat"
    ) && let Some(text) = parsed_text(ffi::prop_named(prop), value)
    {
        return Some(text);
    }
    if (prop == b"box-shadow" || prop == b"text-shadow")
        && let Some(text) = shadow::specified_canonical(value, prop[0] == b't')
    {
        return Some(text);
    }
    if prop == b"aspect-ratio" {
        return parsed_text(Some(Prop::AspectRatio), value);
    }
    if prop == b"animation-range" {
        let (start, end) = animation::range_shorthand_expand(value)?;
        return Some(animation::range_serialize(&start, &end));
    }
    if matches!(
        prop,
        b"transition-delay" | b"transition-duration" | b"animation-delay" | b"animation-duration"
    ) {
        return time::list_serialize(value, false);
    }
    math_canonical(&c_text(value))
}
