//! Southstar — CSS gradients: linear-, radial- and conic-gradient() with their repeating forms parsed into the ns_css_gradient css.c stores, serialized in specified and computed form, and resolved to an angle or radii for painting.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::{PI, SQRT_2};
use std::ffi::{CStr, CString};

use crate::calc;
use crate::color::{self, color_text};
use crate::ffi;
use crate::position;
use crate::scan::{skip_ws, split_ws_paren, starts_with_ci, strip};
use crate::units::{self, NUMBER, PERCENT};

pub(crate) const STOPS_MAX: usize = 32;
pub(crate) const INTERP_MAX: usize = 32;

pub(crate) const TO_TOP: i32 = 1;
pub(crate) const TO_BOTTOM: i32 = 2;
pub(crate) const TO_LEFT: i32 = 4;
pub(crate) const TO_RIGHT: i32 = 8;

pub(crate) const SIZE_FARTHEST_CORNER: u32 = 0;
pub(crate) const SIZE_CLOSEST_SIDE: u32 = 1;
pub(crate) const SIZE_FARTHEST_SIDE: u32 = 2;
pub(crate) const SIZE_CLOSEST_CORNER: u32 = 3;
pub(crate) const SIZE_EXPLICIT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stop {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub pos: f64,
    pub pos_px: f64,
    pub has_pos: i32,
    pub pos_is_angle: i32,
    pub is_hint: i32,
    pub pair_with_prev: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Gradient {
    pub angle_deg: f64,
    pub to_side: i32,
    pub has_angle: i32,
    pub n_stops: i32,
    pub radial: i32,
    pub conic: i32,
    pub repeating: i32,
    pub circle: i32,
    pub shape_explicit: i32,
    pub size: u32,
    pub size_x: f64,
    pub size_y: f64,
    pub size_x_pct: f64,
    pub size_y_pct: f64,
    pub from_deg: f64,
    pub has_from: i32,
    pub center_x: f64,
    pub center_y: f64,
    pub center_x_px: f64,
    pub center_y_px: f64,
    pub has_center: i32,
    pub interp: [u8; INTERP_MAX],
    pub stops: [Stop; STOPS_MAX],
}

const _: () = assert!(
    core::mem::size_of::<Stop>() == 40
        && core::mem::size_of::<Gradient>() == 1448
        && core::mem::offset_of!(Gradient, size_x) == 48
        && core::mem::offset_of!(Gradient, has_from) == 88
        && core::mem::offset_of!(Gradient, interp) == 132
        && core::mem::offset_of!(Gradient, stops) == 168
);

impl Default for Gradient {
    fn default() -> Gradient {
        Gradient {
            angle_deg: 0.0,
            to_side: 0,
            has_angle: 0,
            n_stops: 0,
            radial: 0,
            conic: 0,
            repeating: 0,
            circle: 0,
            shape_explicit: 0,
            size: SIZE_FARTHEST_CORNER,
            size_x: 0.0,
            size_y: 0.0,
            size_x_pct: 0.0,
            size_y_pct: 0.0,
            from_deg: 0.0,
            has_from: 0,
            center_x: 0.0,
            center_y: 0.0,
            center_x_px: 0.0,
            center_y_px: 0.0,
            has_center: 0,
            interp: [0; INTERP_MAX],
            stops: [Stop::default(); STOPS_MAX],
        }
    }
}

impl Gradient {
    fn interp(&self) -> &[u8] {
        let len = self
            .interp
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(INTERP_MAX);
        &self.interp[..len]
    }

    fn set_interp(&mut self, text: &[u8]) {
        self.interp = [0; INTERP_MAX];
        let len = text.len().min(INTERP_MAX - 1);
        self.interp[..len].copy_from_slice(&text[..len]);
    }

    fn push_stop(&mut self, stop: Stop) {
        if (self.n_stops as usize) < STOPS_MAX {
            self.stops[self.n_stops as usize] = stop;
            self.n_stops += 1;
        }
    }
}

#[derive(Default)]
pub(crate) struct GradientParse {
    pub gr: Gradient,
    angle_text: Option<Vec<u8>>,
    size_text: Option<Vec<u8>>,
    position_text: Option<Vec<u8>>,
    interp_explicit: bool,
    all_legacy: bool,
    stop_texts: Option<Vec<Vec<u8>>>,
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn flag(value: bool) -> i32 {
    i32::from(value)
}

fn token_eq(t: &[u8], kw: &[u8]) -> bool {
    t.eq_ignore_ascii_case(kw)
}

pub(crate) fn token_is_math_fn(t: &[u8]) -> bool {
    [&b"calc("[..], b"min(", b"max(", b"clamp("]
        .iter()
        .any(|name| starts_with_ci(t, name))
}

pub(crate) fn token_is_length_pct(t: &[u8]) -> bool {
    if token_is_math_fn(t) {
        return calc::parse_calc(&c_text(t)).is_some();
    }
    match units::parse_length(&c_text(t)) {
        Some((v, unit)) => unit != NUMBER || v == 0.0,
        None => false,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn token_is_angle(t: &[u8]) -> bool {
    if token_is_math_fn(t) {
        let lower = t.to_ascii_lowercase();
        return [&b"deg"[..], b"rad", b"turn", b"grad"]
            .iter()
            .any(|unit| contains(&lower, unit));
    }
    if t == b"0" {
        return true;
    }
    let text = c_text(t);
    let (_, end) = ffi::strtod(&text, 0);
    if end == 0 {
        return false;
    }
    let rest = &text.to_bytes()[end..];
    [&b"deg"[..], b"grad", b"rad", b"turn"]
        .iter()
        .any(|unit| token_eq(rest, unit))
}

pub(crate) fn parse_angle_deg(s: &[u8]) -> f64 {
    let text = c_text(s);
    let bytes = text.to_bytes();
    let (v, end) = ffi::strtod(&text, 0);
    if end == 0 {
        return 0.0;
    }
    let rest = &bytes[skip_ws(bytes, end, bytes.len())..];
    if starts_with_ci(rest, b"rad") {
        v * 180.0 / PI
    } else if starts_with_ci(rest, b"turn") {
        v * 360.0
    } else if starts_with_ci(rest, b"grad") {
        v * 0.9
    } else {
        v
    }
}

fn stop_color_specified(tok: &[u8]) -> Vec<u8> {
    let legacy = tok.first() == Some(&b'#')
        || [&b"rgb("[..], b"rgba(", b"hsl(", b"hsla("]
            .iter()
            .any(|name| starts_with_ci(tok, name));
    if legacy {
        if let Some(rgba) = color::parse_color(&c_text(tok)) {
            return color_text(rgba);
        }
    }
    if tok.contains(&b'(') {
        tok.to_vec()
    } else {
        tok.to_ascii_lowercase()
    }
}

fn color_text_is_legacy(t: &[u8]) -> bool {
    t.first() == Some(&b'#')
        || !t.contains(&b'(')
        || [&b"rgb("[..], b"rgba(", b"hsl(", b"hsla("]
            .iter()
            .any(|name| starts_with_ci(t, name))
}

const RECT_SPACES: &[&[u8]] = &[
    b"srgb",
    b"srgb-linear",
    b"display-p3",
    b"a98-rgb",
    b"prophoto-rgb",
    b"rec2020",
    b"lab",
    b"oklab",
    b"xyz",
    b"xyz-d50",
    b"xyz-d65",
];
const POLAR_SPACES: &[&[u8]] = &[b"hsl", b"hwb", b"lch", b"oklch"];

fn interp_parse(tok: &[&[u8]], gr: &mut Gradient) -> usize {
    let Some(first) = tok.first() else {
        return 0;
    };
    let mut space = first.to_ascii_lowercase();
    let is_rect = RECT_SPACES.contains(&space.as_slice());
    let is_polar = POLAR_SPACES.contains(&space.as_slice());
    if !is_rect && !is_polar {
        return 0;
    }
    if space == b"xyz" {
        space = b"xyz-d65".to_vec();
    }
    let n = tok.len();
    let mut used = 1;
    let mut hue: Option<&[u8]> = None;
    if is_polar && n >= 3 && token_eq(tok[2], b"hue") {
        if token_eq(tok[1], b"shorter") {
            hue = None;
        } else if token_eq(tok[1], b"longer") {
            hue = Some(b"longer");
        } else if token_eq(tok[1], b"increasing") {
            hue = Some(b"increasing");
        } else if token_eq(tok[1], b"decreasing") {
            hue = Some(b"decreasing");
        } else {
            return 0;
        }
        used = 3;
    } else if n >= 2
        && [
            &b"hue"[..],
            b"shorter",
            b"longer",
            b"increasing",
            b"decreasing",
        ]
        .iter()
        .any(|kw| token_eq(tok[1], kw))
    {
        return 0;
    }
    match hue {
        Some(hue) => {
            let mut text = space;
            text.push(b' ');
            text.extend_from_slice(hue);
            text.extend_from_slice(b" hue");
            gr.set_interp(&text);
        }
        None => gr.set_interp(&space),
    }
    used
}

fn position_parse(tok: &[&[u8]], gp: &mut GradientParse) -> usize {
    let mut m = 0;
    while m < tok.len() && m < 4 && !token_eq(tok[m], b"in") {
        m += 1;
    }
    if m == 0 {
        return 0;
    }
    let joined = tok[..m].join(&b' ');
    let Some(spec) = position::canonical_ex(&joined, false, false) else {
        return 0;
    };
    let (xs, ys) = position::split(&spec);
    let (ok, x) = calc::resolve_to_px_pct(&xs, false);
    if ok {
        gp.gr.center_x = x.pct / 100.0;
        gp.gr.center_x_px = x.px;
    }
    let (ok, y) = calc::resolve_to_px_pct(&ys, false);
    if ok {
        gp.gr.center_y = y.pct / 100.0;
        gp.gr.center_y_px = y.px;
    }
    gp.gr.has_center = flag(
        !(gp.gr.center_x == 0.5
            && gp.gr.center_x_px == 0.0
            && gp.gr.center_y == 0.5
            && gp.gr.center_y_px == 0.0),
    );
    gp.position_text = Some(spec);
    m
}

const SIZE_KEYWORDS: &[(&[u8], u32)] = &[
    (b"closest-side", SIZE_CLOSEST_SIDE),
    (b"farthest-side", SIZE_FARTHEST_SIDE),
    (b"closest-corner", SIZE_CLOSEST_CORNER),
    (b"farthest-corner", SIZE_FARTHEST_CORNER),
];

fn parse_prelude(gp: &mut GradientParse, tok: &[&[u8]]) -> bool {
    let n = tok.len();
    let (mut seen_dir, mut seen_shape, mut seen_size) = (false, false, false);
    let (mut seen_at, mut seen_in, mut seen_from) = (false, false, false);
    let mut explicit_lengths = 0;
    let mut i = 0;
    while i < n {
        let t = tok[i];
        if token_eq(t, b"in") {
            if seen_in {
                return false;
            }
            let used = interp_parse(&tok[i + 1..], &mut gp.gr);
            if used == 0 {
                return false;
            }
            seen_in = true;
            gp.interp_explicit = true;
            i += 1 + used;
            continue;
        }
        if gp.gr.radial == 0 && gp.gr.conic == 0 {
            if token_eq(t, b"to") {
                if seen_dir {
                    return false;
                }
                let mut sides = 0;
                let mut k = i + 1;
                while k < n && k <= i + 2 {
                    let bit = if token_eq(tok[k], b"top") {
                        TO_TOP
                    } else if token_eq(tok[k], b"bottom") {
                        TO_BOTTOM
                    } else if token_eq(tok[k], b"left") {
                        TO_LEFT
                    } else if token_eq(tok[k], b"right") {
                        TO_RIGHT
                    } else {
                        0
                    };
                    if bit == 0 {
                        break;
                    }
                    if gp.gr.to_side & bit != 0 {
                        return false;
                    }
                    gp.gr.to_side |= bit;
                    sides += 1;
                    k += 1;
                }
                if sides == 0 {
                    return false;
                }
                if gp.gr.to_side & (TO_TOP | TO_BOTTOM) == TO_TOP | TO_BOTTOM
                    || gp.gr.to_side & (TO_LEFT | TO_RIGHT) == TO_LEFT | TO_RIGHT
                {
                    return false;
                }
                seen_dir = true;
                i += 1 + sides;
                continue;
            }
            if token_is_angle(t) {
                if seen_dir {
                    return false;
                }
                gp.gr.has_angle = 1;
                gp.gr.angle_deg = parse_angle_deg(t);
                gp.angle_text = Some(t.to_ascii_lowercase());
                seen_dir = true;
                i += 1;
                continue;
            }
            return false;
        }
        if token_eq(t, b"at") {
            if seen_at {
                return false;
            }
            let used = position_parse(&tok[i + 1..], gp);
            if used == 0 {
                return false;
            }
            seen_at = true;
            i += 1 + used;
            continue;
        }
        if gp.gr.conic != 0 {
            if token_eq(t, b"from") {
                if seen_from
                    || i + 1 >= n
                    || !token_is_angle(tok[i + 1])
                    || tok[i + 1].contains(&b'%')
                    || math_text_mixes_angle_and_length(tok[i + 1])
                {
                    return false;
                }
                gp.gr.has_from = 1;
                gp.gr.from_deg = parse_angle_deg(tok[i + 1]);
                gp.angle_text = Some(tok[i + 1].to_ascii_lowercase());
                seen_from = true;
                i += 2;
                continue;
            }
            return false;
        }
        if token_eq(t, b"circle") || token_eq(t, b"ellipse") {
            if seen_shape {
                return false;
            }
            gp.gr.circle = flag(token_eq(t, b"circle"));
            gp.gr.shape_explicit = 1;
            seen_shape = true;
            i += 1;
            continue;
        }
        let mut size_keyword = false;
        for &(kw, size) in SIZE_KEYWORDS {
            if !token_eq(t, kw) {
                continue;
            }
            if seen_size {
                return false;
            }
            gp.gr.size = size;
            seen_size = true;
            size_keyword = true;
        }
        if size_keyword {
            i += 1;
            continue;
        }
        if token_is_length_pct(t) {
            if seen_size {
                return false;
            }
            let mut j = 0;
            let mut px = [0.0f64; 2];
            let mut pct = [0.0f64; 2];
            while i + j < n && j < 2 && token_is_length_pct(tok[i + j]) {
                let (_, resolved) = calc::resolve_to_px_pct(tok[i + j], false);
                px[j] = resolved.px;
                pct[j] = resolved.pct;
                if px[j] < 0.0 || pct[j] < 0.0 {
                    return false;
                }
                j += 1;
            }
            if i + j < n && token_is_length_pct(tok[i + j]) {
                return false;
            }
            gp.gr.size = SIZE_EXPLICIT;
            gp.gr.size_x = px[0];
            gp.gr.size_x_pct = pct[0];
            gp.gr.size_y = if j == 2 { px[1] } else { px[0] };
            gp.gr.size_y_pct = if j == 2 { pct[1] } else { pct[0] };
            explicit_lengths = j;
            if j == 1 && tok[i].contains(&b'%') {
                return false;
            }
            gp.size_text = Some(if j == 2 {
                [tok[i], tok[i + 1]].join(&b' ')
            } else {
                tok[i].to_vec()
            });
            seen_size = true;
            i += j;
            continue;
        }
        return false;
    }
    if explicit_lengths > 0 {
        if seen_shape && (gp.gr.circle != 0) != (explicit_lengths == 1) {
            return false;
        }
        gp.gr.circle = flag(explicit_lengths == 1);
    }
    true
}

pub(crate) fn math_text_has_unit(t: &[u8], units: &[&[u8]]) -> bool {
    let lower = t.to_ascii_lowercase();
    let mut p = 0;
    while p < lower.len() {
        if !(lower[p].is_ascii_digit() || lower[p] == b'.') {
            p += 1;
            continue;
        }
        while p < lower.len() && (lower[p].is_ascii_digit() || lower[p] == b'.') {
            p += 1;
        }
        let rest = &lower[p..];
        let found = units.iter().any(|unit| {
            starts_with_ci(rest, unit)
                && !rest
                    .get(unit.len())
                    .is_some_and(|&c| c.is_ascii_alphabetic())
        });
        if found {
            return true;
        }
        if p >= lower.len() {
            break;
        }
        p += 1;
    }
    false
}

const ANGLE_UNITS: &[&[u8]] = &[b"deg", b"grad", b"rad", b"turn"];
const LENGTH_UNITS: &[&[u8]] = &[
    b"px", b"em", b"rem", b"vw", b"vh", b"vmin", b"vmax", b"ch", b"ex", b"cm", b"mm", b"in", b"pt",
    b"pc", b"lh",
];

fn math_text_mixes_angle_and_length(t: &[u8]) -> bool {
    math_text_has_unit(t, ANGLE_UNITS) && math_text_has_unit(t, LENGTH_UNITS)
}

fn conic_calc_canonical(t: &[u8]) -> Vec<u8> {
    if !starts_with_ci(t, b"calc(") || t.len() < 7 || t[t.len() - 1] != b')' {
        return t.to_vec();
    }
    let inner = &t[5..t.len() - 1];
    let Some(plus) = inner.windows(3).position(|window| window == b" + ") else {
        return t.to_vec();
    };
    if inner.contains(&b'(') {
        return t.to_vec();
    }
    let a = strip(&inner[..plus]);
    let b = strip(&inner[plus + 3..]);
    if a.contains(&b'%') || !b.contains(&b'%') {
        return t.to_vec();
    }
    let mut out = b"calc(".to_vec();
    out.extend_from_slice(b);
    out.extend_from_slice(b" + ");
    out.extend_from_slice(a);
    out.push(b')');
    out
}

fn stop_pos_parse(t: &[u8], conic: bool, st: &mut Stop) -> bool {
    st.pos = 0.0;
    st.pos_px = 0.0;
    st.pos_is_angle = 0;
    if token_is_math_fn(t) {
        let has_angle = math_text_has_unit(t, ANGLE_UNITS);
        let has_len = math_text_has_unit(t, LENGTH_UNITS);
        if if conic { has_len } else { has_angle } {
            return false;
        }
        if conic && !has_angle && !t.contains(&b'%') {
            return false;
        }
    }
    if conic {
        if token_is_angle(t) {
            st.pos = parse_angle_deg(t) / 360.0;
            st.pos_is_angle = 1;
            st.has_pos = 1;
            return true;
        }
        if token_is_math_fn(t) {
            let (ok, resolved) = calc::resolve_to_px_pct(t, false);
            if !ok {
                return false;
            }
            st.pos = resolved.pct / 100.0;
            st.has_pos = 1;
            return true;
        }
        if let Some((v, PERCENT)) = units::parse_length(&c_text(t)) {
            st.pos = v / 100.0;
            st.has_pos = 1;
            return true;
        }
        return false;
    }
    if !token_is_length_pct(t) {
        return false;
    }
    let (_, resolved) = calc::resolve_to_px_pct(t, false);
    st.pos = resolved.pct / 100.0;
    st.pos_px = resolved.px;
    st.has_pos = 1;
    true
}

fn stop_parse(gp: &mut GradientParse, seg: &[u8]) -> Option<bool> {
    let tok = split_ws_paren(seg, 8);
    let n = tok.len();
    let conic = gp.gr.conic != 0;
    if n < 1 {
        return None;
    }
    if let Some(rgba) = color::parse_color(&c_text(tok[0])) {
        if n > 3 {
            return None;
        }
        let [r, g, b, a] = rgba;
        let mut first = Stop {
            r,
            g,
            b,
            a,
            ..Stop::default()
        };
        let mut second = first;
        if n >= 2 && !stop_pos_parse(tok[1], conic, &mut first) {
            return None;
        }
        if n == 3 && !stop_pos_parse(tok[2], conic, &mut second) {
            return None;
        }
        if !color_text_is_legacy(tok[0]) {
            gp.all_legacy = false;
        }
        gp.gr.push_stop(first);
        if n == 3 {
            second.pair_with_prev = 1;
            gp.gr.push_stop(second);
        }
        if let Some(texts) = &mut gp.stop_texts {
            let mut text = stop_color_specified(tok[0]);
            for part in &tok[1..] {
                text.push(b' ');
                if conic {
                    text.extend_from_slice(&conic_calc_canonical(part));
                } else {
                    text.extend_from_slice(part);
                }
            }
            texts.push(text);
        }
        return Some(false);
    }
    if n != 1 {
        return None;
    }
    let mut hint = Stop {
        is_hint: 1,
        ..Stop::default()
    };
    if !stop_pos_parse(tok[0], conic, &mut hint) {
        return None;
    }
    gp.gr.push_stop(hint);
    if let Some(texts) = &mut gp.stop_texts {
        texts.push(if conic {
            conic_calc_canonical(tok[0])
        } else {
            tok[0].to_vec()
        });
    }
    Some(true)
}

fn finish_stops(gr: &mut Gradient) {
    let n = (gr.n_stops as usize).min(STOPS_MAX);
    if n == 0 {
        return;
    }
    let st = &mut gr.stops;
    let mut fixed = [false; STOPS_MAX];
    for i in 0..n {
        fixed[i] = st[i].has_pos != 0;
    }
    if !fixed[0] {
        st[0].pos = 0.0;
        st[0].pos_px = 0.0;
        fixed[0] = true;
    }
    if !fixed[n - 1] {
        st[n - 1].pos = 1.0;
        st[n - 1].pos_px = 0.0;
        fixed[n - 1] = true;
    }
    for i in 1..n {
        if fixed[i]
            && fixed[i - 1]
            && st[i].pos_px == 0.0
            && st[i - 1].pos_px == 0.0
            && st[i].pos < st[i - 1].pos
        {
            st[i].pos = st[i - 1].pos;
        }
    }
    let mut i = 0;
    while i < n {
        if fixed[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && !fixed[j] {
            j += 1;
        }
        let lo = st[i - 1].pos;
        let hi = st[j].pos;
        for (offset, stop) in st[i..j].iter_mut().enumerate() {
            stop.pos = lo + (hi - lo) * (offset + 1) as f64 / (j - i + 1) as f64;
            stop.pos_px = 0.0;
        }
        i = j;
    }
    for i in 1..n.saturating_sub(1) {
        if st[i].is_hint == 0 {
            continue;
        }
        let average = |a: u8, b: u8| ((u32::from(a) + u32::from(b)) / 2) as u8;
        st[i].r = average(st[i - 1].r, st[i + 1].r);
        st[i].g = average(st[i - 1].g, st[i + 1].g);
        st[i].b = average(st[i - 1].b, st[i + 1].b);
        st[i].a = average(st[i - 1].a, st[i + 1].a);
    }
}

pub(crate) fn parse(text: &[u8], keep_texts: bool) -> Option<(GradientParse, usize)> {
    let mut gp = GradientParse::default();
    let mut p = skip_ws(text, 0, text.len());
    if starts_with_ci(&text[p..], b"repeating-") {
        gp.gr.repeating = 1;
        p += 10;
    }
    if starts_with_ci(&text[p..], b"linear-gradient") {
        p += 15;
    } else if starts_with_ci(&text[p..], b"radial-gradient") {
        gp.gr.radial = 1;
        p += 15;
    } else if starts_with_ci(&text[p..], b"conic-gradient") {
        gp.gr.conic = 1;
        p += 14;
    } else {
        return None;
    }
    p = skip_ws(text, p, text.len());
    if p >= text.len() || text[p] != b'(' {
        return None;
    }
    let body_start = p + 1;
    let mut depth = 0i32;
    let mut end = None;
    for (q, &c) in text.iter().enumerate().skip(body_start) {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            if depth == 0 {
                end = Some(q);
                break;
            }
            depth -= 1;
        }
    }
    let end = end?;
    gp.gr.center_x = 0.5;
    gp.gr.center_y = 0.5;
    gp.all_legacy = true;
    if keep_texts {
        gp.stop_texts = Some(Vec::new());
    }
    let mut parts: Vec<&[u8]> = Vec::new();
    let mut seg = body_start;
    depth = 0;
    let mut q = body_start;
    loop {
        let c = text[q];
        if c == b'(' {
            depth += 1;
        } else if c == b')' && depth > 0 {
            depth -= 1;
        }
        if (c == b',' && depth == 0) || q == end {
            parts.push(strip(&text[seg..q]));
            if q == end {
                break;
            }
            seg = q + 1;
        }
        q += 1;
    }
    let mut ok = parts.iter().all(|part| !part.is_empty());
    let mut start = 0;
    if ok && !parts.is_empty() {
        let tok = split_ws_paren(parts[0], 24);
        let saved = gp.gr;
        let saved_explicit = gp.interp_explicit;
        if !tok.is_empty() && parse_prelude(&mut gp, &tok) {
            start = 1;
        } else {
            gp.gr = saved;
            gp.interp_explicit = saved_explicit;
            gp.angle_text = None;
            gp.size_text = None;
            gp.position_text = None;
        }
    }
    if ok && start >= parts.len() {
        ok = false;
    }
    let mut prev_hint = true;
    let mut color_stops = 0;
    if ok {
        for part in &parts[start..] {
            let Some(hint) = stop_parse(&mut gp, part) else {
                ok = false;
                break;
            };
            if hint && prev_hint {
                ok = false;
                break;
            }
            if !hint {
                color_stops += 1;
            }
            prev_hint = hint;
        }
    }
    if !ok || prev_hint || color_stops < 1 {
        return None;
    }
    if gp.interp_explicit {
        let default: &[u8] = if gp.all_legacy { b"srgb" } else { b"oklab" };
        if gp.gr.interp() == default {
            gp.gr.interp[0] = 0;
        }
    }
    finish_stops(&mut gp.gr);
    Some((gp, end + 1))
}

fn append_number(out: &mut Vec<u8>, v: f64, suffix: &[u8]) {
    out.extend_from_slice(&ffi::format_double(c"%g", v));
    out.extend_from_slice(suffix);
}

fn append_mixed(out: &mut Vec<u8>, pct: f64, px: f64) {
    out.extend_from_slice(b"calc(");
    append_number(out, pct, b"% ");
    out.push(if px < 0.0 { b'-' } else { b'+' });
    out.push(b' ');
    append_number(out, px.abs(), b"px)");
}

fn append_stop_pos(out: &mut Vec<u8>, st: &Stop) {
    if st.pos_is_angle != 0 {
        append_number(out, st.pos * 360.0, b"deg");
    } else if st.pos_px == 0.0 {
        append_number(out, st.pos * 100.0, b"%");
    } else if st.pos == 0.0 {
        append_number(out, st.pos_px, b"px");
    } else {
        append_mixed(out, st.pos * 100.0, st.pos_px);
    }
}

fn append_center_coord(out: &mut Vec<u8>, frac: f64, px: f64) {
    if px == 0.0 {
        append_number(out, frac * 100.0, b"%");
    } else if frac == 0.0 {
        append_number(out, px, b"px");
    } else {
        append_mixed(out, frac * 100.0, px);
    }
}

fn to_side_text(to_side: i32) -> Vec<u8> {
    let mut out = b"to".to_vec();
    for (bit, name) in [
        (TO_LEFT, &b" left"[..]),
        (TO_RIGHT, b" right"),
        (TO_TOP, b" top"),
        (TO_BOTTOM, b" bottom"),
    ] {
        if to_side & bit != 0 {
            out.extend_from_slice(name);
        }
    }
    out
}

fn append_name(out: &mut Vec<u8>, gr: &Gradient) {
    if gr.repeating != 0 {
        out.extend_from_slice(b"repeating-");
    }
    out.extend_from_slice(if gr.conic != 0 {
        b"conic-gradient("
    } else if gr.radial != 0 {
        b"radial-gradient("
    } else {
        b"linear-gradient("
    });
}

fn append_part(prelude: &mut Vec<u8>, part: &[u8]) {
    if !prelude.is_empty() {
        prelude.push(b' ');
    }
    prelude.extend_from_slice(part);
}

fn size_keyword(size: u32) -> Option<&'static [u8]> {
    match size {
        SIZE_CLOSEST_SIDE => Some(b"closest-side"),
        SIZE_FARTHEST_SIDE => Some(b"farthest-side"),
        SIZE_CLOSEST_CORNER => Some(b"closest-corner"),
        _ => None,
    }
}

fn finish_prelude(out: &mut Vec<u8>, prelude: Vec<u8>) {
    if !prelude.is_empty() {
        out.extend_from_slice(&prelude);
        out.extend_from_slice(b", ");
    }
}

pub(crate) fn serialize_computed(gr: &Gradient) -> Vec<u8> {
    let mut out = Vec::new();
    append_name(&mut out, gr);
    let mut pre = Vec::new();
    if gr.conic != 0 {
        if gr.has_from != 0 {
            let mut t = b"from ".to_vec();
            append_number(&mut t, gr.from_deg, b"deg");
            append_part(&mut pre, &t);
        }
    } else if gr.radial != 0 {
        if gr.size == SIZE_EXPLICIT {
            let mut size = Vec::new();
            append_center_coord(&mut size, gr.size_x_pct / 100.0, gr.size_x);
            if gr.circle == 0 {
                size.push(b' ');
                append_center_coord(&mut size, gr.size_y_pct / 100.0, gr.size_y);
            }
            append_part(&mut pre, &size);
        } else {
            if gr.circle != 0 {
                append_part(&mut pre, b"circle");
            }
            if let Some(kw) = size_keyword(gr.size) {
                append_part(&mut pre, kw);
            }
        }
    } else if gr.has_angle != 0 {
        let mut t = Vec::new();
        append_number(&mut t, gr.angle_deg, b"deg");
        append_part(&mut pre, &t);
    } else if gr.to_side != 0 && gr.to_side != TO_BOTTOM {
        append_part(&mut pre, &to_side_text(gr.to_side));
    }
    if (gr.radial != 0 || gr.conic != 0) && gr.has_center != 0 {
        let mut at = b"at ".to_vec();
        append_center_coord(&mut at, gr.center_x, gr.center_x_px);
        at.push(b' ');
        append_center_coord(&mut at, gr.center_y, gr.center_y_px);
        append_part(&mut pre, &at);
    }
    if !gr.interp().is_empty() {
        let mut t = b"in ".to_vec();
        t.extend_from_slice(gr.interp());
        append_part(&mut pre, &t);
    }
    finish_prelude(&mut out, pre);
    for i in 0..(gr.n_stops.max(0) as usize).min(STOPS_MAX) {
        let st = &gr.stops[i];
        if st.pair_with_prev != 0 {
            out.push(b' ');
            append_stop_pos(&mut out, st);
            continue;
        }
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        if st.is_hint != 0 {
            append_stop_pos(&mut out, st);
            continue;
        }
        out.extend_from_slice(&color_text([st.r, st.g, st.b, st.a]));
        if st.has_pos != 0 {
            out.push(b' ');
            append_stop_pos(&mut out, st);
        }
    }
    out.push(b')');
    out
}

pub(crate) fn serialize_specified(gp: &GradientParse) -> Vec<u8> {
    let gr = &gp.gr;
    let mut out = Vec::new();
    append_name(&mut out, gr);
    let mut pre = Vec::new();
    if gr.conic != 0 {
        if gr.has_from != 0 {
            let mut t = b"from ".to_vec();
            t.extend_from_slice(gp.angle_text.as_deref().unwrap_or_default());
            append_part(&mut pre, &t);
        }
    } else if gr.radial != 0 {
        if gr.size == SIZE_EXPLICIT {
            append_part(&mut pre, gp.size_text.as_deref().unwrap_or_default());
        } else {
            if gr.circle != 0 {
                append_part(&mut pre, b"circle");
            }
            if let Some(kw) = size_keyword(gr.size) {
                append_part(&mut pre, kw);
            }
        }
    } else if gr.has_angle != 0 {
        append_part(&mut pre, gp.angle_text.as_deref().unwrap_or_default());
    } else if gr.to_side != 0 && gr.to_side != TO_BOTTOM {
        append_part(&mut pre, &to_side_text(gr.to_side));
    }
    if let Some(position) = &gp.position_text {
        if (gr.radial != 0 || gr.conic != 0)
            && !position.eq_ignore_ascii_case(b"center")
            && !position.eq_ignore_ascii_case(b"center center")
        {
            let mut t = b"at ".to_vec();
            t.extend_from_slice(position);
            append_part(&mut pre, &t);
        }
    }
    if !gr.interp().is_empty() {
        let mut t = b"in ".to_vec();
        t.extend_from_slice(gr.interp());
        append_part(&mut pre, &t);
    }
    finish_prelude(&mut out, pre);
    for (i, text) in gp.stop_texts.iter().flatten().enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(text);
    }
    out.push(b')');
    out
}

pub(crate) fn text_starts_gradient(text: &[u8]) -> bool {
    let mut p = skip_ws(text, 0, text.len());
    if starts_with_ci(&text[p..], b"repeating-") {
        p += 10;
    }
    let rest = &text[p..];
    starts_with_ci(rest, b"linear-gradient")
        || starts_with_ci(rest, b"radial-gradient")
        || starts_with_ci(rest, b"conic-gradient")
}

pub(crate) fn parse_value(text: &CStr) -> Option<Gradient> {
    let s = text.to_bytes();
    let (gp, end) = parse(s, false)?;
    if skip_ws(s, end, s.len()) < s.len() {
        return None;
    }
    Some(gp.gr)
}

pub(crate) fn specified_with_end(layer: &[u8]) -> Option<(Vec<u8>, usize)> {
    let (gp, end) = parse(layer, true)?;
    Some((serialize_specified(&gp), end))
}

pub(crate) fn angle(gr: &Gradient, w: f64, h: f64) -> f64 {
    if gr.has_angle != 0 {
        return gr.angle_deg;
    }
    let side = if gr.to_side != 0 {
        gr.to_side
    } else {
        TO_BOTTOM
    };
    let horiz = side & (TO_LEFT | TO_RIGHT) != 0;
    let vert = side & (TO_TOP | TO_BOTTOM) != 0;
    if !horiz {
        return if side & TO_TOP != 0 { 0.0 } else { 180.0 };
    }
    if !vert {
        return if side & TO_LEFT != 0 { 270.0 } else { 90.0 };
    }
    let corner = if w > 0.0 && h > 0.0 {
        w.atan2(h) * 180.0 / PI
    } else {
        45.0
    };
    let right = side & TO_RIGHT != 0;
    let top = side & TO_TOP != 0;
    if right && top {
        corner
    } else if right {
        180.0 - corner
    } else if top {
        360.0 - corner
    } else {
        180.0 + corner
    }
}

fn gmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn gmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub(crate) fn radii(gr: &Gradient, w: f64, h: f64, cx: f64, cy: f64) -> (f64, f64) {
    let dl = cx.abs();
    let dr = (w - cx).abs();
    let dt = cy.abs();
    let db = (h - cy).abs();
    let (mut rx, mut ry);
    match gr.size {
        SIZE_EXPLICIT => {
            rx = gr.size_x + gr.size_x_pct / 100.0 * w;
            ry = if gr.circle != 0 {
                rx
            } else {
                gr.size_y + gr.size_y_pct / 100.0 * h
            };
        }
        SIZE_CLOSEST_SIDE => {
            rx = gmin(dl, dr);
            ry = gmin(dt, db);
            if gr.circle != 0 {
                rx = gmin(rx, ry);
                ry = rx;
            }
        }
        SIZE_FARTHEST_SIDE => {
            rx = gmax(dl, dr);
            ry = gmax(dt, db);
            if gr.circle != 0 {
                rx = gmax(rx, ry);
                ry = rx;
            }
        }
        SIZE_CLOSEST_CORNER => {
            if gr.circle != 0 {
                rx = (gmin(dl, dr) * gmin(dl, dr) + gmin(dt, db) * gmin(dt, db)).sqrt();
                ry = rx;
            } else {
                rx = gmin(dl, dr) * SQRT_2;
                ry = gmin(dt, db) * SQRT_2;
            }
        }
        _ => {
            if gr.circle != 0 {
                rx = (gmax(dl, dr) * gmax(dl, dr) + gmax(dt, db) * gmax(dt, db)).sqrt();
                ry = rx;
            } else {
                rx = gmax(dl, dr) * SQRT_2;
                ry = gmax(dt, db) * SQRT_2;
            }
        }
    }
    if rx <= 0.0 {
        rx = 1.0;
    }
    if ry <= 0.0 {
        ry = 1.0;
    }
    (rx, ry)
}
