//! Southstar — SMIL animation values: number templates, colours, interpolation between key values and motion along a path.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_style::parse_color;

use super::{Calc, Element, Kind};

const SLOT: u8 = 1;

pub struct PathPoint {
    pub x: f64,
    pub y: f64,
    pub starts_subpath: bool,
}

struct Numbers {
    values: Vec<f64>,
    template: Vec<u8>,
}

pub fn number_span(text: &[u8]) -> usize {
    let at = |i: usize| text.get(i).copied().unwrap_or(0);
    let mut q = usize::from(matches!(at(0), b'+' | b'-'));
    let digits = q;
    while at(q).is_ascii_digit() {
        q += 1;
    }
    let int_len = q - digits;
    let mut frac_len = 0;
    if at(q) == b'.' {
        let mut f = q + 1;
        while at(f).is_ascii_digit() {
            f += 1;
        }
        frac_len = f - (q + 1);
        if int_len != 0 || frac_len != 0 {
            q = f;
        }
    }
    if int_len == 0 && frac_len == 0 {
        return 0;
    }
    if matches!(at(q), b'e' | b'E') {
        let mut e = q + 1;
        if matches!(at(e), b'+' | b'-') {
            e += 1;
        }
        if at(e).is_ascii_digit() {
            while at(e).is_ascii_digit() {
                e += 1;
            }
            q = e;
        }
    }
    q
}

pub fn span_value(span: &[u8]) -> f64 {
    core::str::from_utf8(span)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn parse_numbers(text: &str) -> Option<Numbers> {
    let bytes = text.as_bytes();
    if bytes.contains(&b'#') || contains(bytes, b"url(") {
        return None;
    }
    let mut out = Numbers {
        values: Vec::new(),
        template: Vec::new(),
    };
    let mut p = 0;
    while p < bytes.len() {
        let len = number_span(&bytes[p..]);
        if len == 0 {
            out.template.push(bytes[p]);
            p += 1;
            continue;
        }
        let digits = p + usize::from(matches!(bytes[p], b'+' | b'-'));
        if bytes[digits] == b'0' && bytes.get(digits + 1).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        out.values.push(span_value(&bytes[p..p + len]));
        out.template.push(SLOT);
        p += len;
    }
    (!out.values.is_empty()).then_some(out)
}

fn is_separator(c: u8) -> bool {
    matches!(c, b' ' | b',' | b'\t' | b'\n' | b'\r')
}

fn compatible(a: &Numbers, b: &Numbers) -> bool {
    a.values.len() == b.values.len()
        && a.template
            .iter()
            .filter(|&&c| !is_separator(c))
            .eq(b.template.iter().filter(|&&c| !is_separator(c)))
}

fn trim_fraction(text: &mut String) {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
}

pub fn format_number(value: f64) -> String {
    let value = if value.is_finite() { value } else { 0.0 };
    if value == 0.0 {
        return String::from("0");
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-5..6).contains(&exponent) {
        let text = format!("{value:.5e}");
        let (mantissa, power) = text.split_once('e').unwrap_or((&text, "0"));
        let mut mantissa = mantissa.to_owned();
        trim_fraction(&mut mantissa);
        return format!("{mantissa}e{power}");
    }
    let mut text = format!("{value:.*}", (5 - exponent).max(0) as usize);
    trim_fraction(&mut text);
    text
}

fn format_numbers(template: &[u8], values: &[f64]) -> String {
    let mut out = Vec::with_capacity(template.len() + values.len() * 8);
    let mut next = values.iter();
    for &c in template {
        if c == SLOT {
            out.extend_from_slice(format_number(next.next().copied().unwrap_or(0.0)).as_bytes());
        } else {
            out.push(c);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn format_color(c: [f64; 4]) -> String {
    format!(
        "rgba({},{},{},{})",
        format_number(c[0].clamp(0.0, 255.0).round()),
        format_number(c[1].clamp(0.0, 255.0).round()),
        format_number(c[2].clamp(0.0, 255.0).round()),
        format_number(c[3].clamp(0.0, 255.0) / 255.0)
    )
}

fn color_of(text: &str) -> Option<[f64; 4]> {
    let text = CString::new(text).ok()?;
    parse_color(&text).map(|c| c.map(f64::from))
}

pub fn combine(a: &str, b: &str, wa: f64, wb: f64, is_color: bool) -> Option<String> {
    if is_color {
        let (ca, cb) = (color_of(a)?, color_of(b)?);
        return Some(format_color(core::array::from_fn(|i| {
            ca[i] * wa + cb[i] * wb
        })));
    }
    let (na, nb) = (parse_numbers(a)?, parse_numbers(b)?);
    if !compatible(&na, &nb) {
        return None;
    }
    let mixed: Vec<f64> = na
        .values
        .iter()
        .zip(&nb.values)
        .map(|(x, y)| x * wa + y * wb)
        .collect();
    Some(format_numbers(&na.template, &mixed))
}

pub fn zero_like(text: &str) -> Option<String> {
    let numbers = parse_numbers(text)?;
    Some(format_numbers(
        &numbers.template,
        &vec![0.0; numbers.values.len()],
    ))
}

fn distance(a: &str, b: &str, is_color: bool) -> Option<f64> {
    if is_color {
        let (ca, cb) = (color_of(a)?, color_of(b)?);
        let sum: f64 = ca.iter().zip(&cb).map(|(x, y)| (x - y) * (x - y)).sum();
        return Some(sum.sqrt());
    }
    let (na, nb) = (parse_numbers(a)?, parse_numbers(b)?);
    if !compatible(&na, &nb) {
        return None;
    }
    let sum: f64 = na
        .values
        .iter()
        .zip(&nb.values)
        .map(|(x, y)| (x - y) * (x - y))
        .sum();
    Some(sum.sqrt())
}

fn bezier_axis(a: f64, b: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

fn spline_ease(k: &[f64], x: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if bezier_axis(k[0], k[2], mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier_axis(k[1], k[3], (lo + hi) / 2.0)
}

fn key_times_usable(el: &Element, n: usize) -> bool {
    n >= 2
        && el.key_times.len() == n
        && el.key_times[0] == 0.0
        && el.key_times.windows(2).all(|w| w[1] >= w[0])
}

fn key_segment(times: &[f64], p: f64) -> (usize, f64) {
    let last = times.len() - 1;
    let mut i = 0;
    while i + 1 < last && p >= times[i + 1] {
        i += 1;
    }
    let (t0, t1) = (times[i], times[i + 1]);
    let local = if t1 > t0 {
        ((p - t0) / (t1 - t0)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    (i, local)
}

fn paced_segment(el: &Element, values: &[&str], p: f64) -> Option<(usize, f64)> {
    let n = values.len();
    let mut dist = Vec::with_capacity(n - 1);
    for pair in values.windows(2) {
        dist.push(distance(pair[0], pair[1], el.is_color)?);
    }
    let total: f64 = dist.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut at = p * total;
    let mut i = 0;
    while i + 2 < n && at > dist[i] {
        at -= dist[i];
        i += 1;
    }
    let local = if dist[i] > 0.0 {
        (at / dist[i]).clamp(0.0, 1.0)
    } else {
        1.0
    };
    Some((i, local))
}

fn discrete_index(el: &Element, n: usize, p: f64) -> usize {
    if key_times_usable(el, n) {
        let mut i = 0;
        while i + 1 < n && p >= el.key_times[i + 1] {
            i += 1;
        }
        return i;
    }
    ((p * n as f64).floor() as usize).min(n - 1)
}

fn sample(el: &Element, values: &[&str], p: f64) -> Option<String> {
    let n = values.len();
    match n {
        0 => return None,
        1 => return Some(values[0].to_owned()),
        _ => {}
    }
    if el.calc != Calc::Discrete {
        let paced = if el.calc == Calc::Paced {
            paced_segment(el, values, p)
        } else {
            None
        };
        let (seg, mut local) = match paced {
            Some(found) => found,
            None if key_times_usable(el, n) => key_segment(&el.key_times, p),
            None => {
                let at = p * (n - 1) as f64;
                let seg = (at.floor() as usize).min(n - 2);
                (seg, at - seg as f64)
            }
        };
        if el.calc == Calc::Spline && el.key_splines.len() >= 4 * (seg + 1) {
            local = spline_ease(&el.key_splines[4 * seg..4 * seg + 4], local);
        }
        if let Some(mixed) = combine(
            values[seg],
            values[seg + 1],
            1.0 - local,
            local,
            el.is_color,
        ) {
            return Some(mixed);
        }
    }
    Some(values[discrete_index(el, n, p)].to_owned())
}

fn default_for(attr: &[u8]) -> Option<&'static str> {
    const UNIT_PROPS: [&[u8]; 5] = [
        b"opacity",
        b"fill-opacity",
        b"stroke-opacity",
        b"stop-opacity",
        b"flood-opacity",
    ];
    UNIT_PROPS.contains(&attr).then_some("1")
}

fn path_length(points: &[PathPoint]) -> f64 {
    points
        .windows(2)
        .filter(|w| !w[1].starts_subpath)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum()
}

fn path_point(points: &[PathPoint], at: f64) -> (f64, f64, f64) {
    let Some(first) = points.first() else {
        return (0.0, 0.0, 0.0);
    };
    let (mut x, mut y, mut angle) = (first.x, first.y, 0.0);
    let mut walked = 0.0;
    for w in points.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if b.starts_subpath {
            continue;
        }
        let seg = (b.x - a.x).hypot(b.y - a.y);
        if seg <= 0.0 {
            continue;
        }
        angle = (b.y - a.y).atan2(b.x - a.x);
        if at <= walked + seg {
            let t = (at - walked) / seg;
            return (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, angle);
        }
        walked += seg;
        (x, y) = (b.x, b.y);
    }
    (x, y, angle)
}

fn motion_transform(el: &Element, x: f64, y: f64, angle: f64) -> String {
    let mut out = format!("translate({} {})", format_number(x), format_number(y));
    let degrees = if el.rotate_auto {
        angle.to_degrees() + if el.rotate_reverse { 180.0 } else { 0.0 }
    } else {
        el.rotate
    };
    if degrees != 0.0 {
        out.push_str(&format!(" rotate({})", format_number(degrees)));
    }
    out
}

fn motion_fraction(el: &Element, p: f64) -> f64 {
    if el.key_points.len() < 2 || el.key_times.len() != el.key_points.len() {
        return p;
    }
    let (i, local) = key_segment(&el.key_times, p);
    let (a, b) = (el.key_points[i], el.key_points[i + 1]);
    if el.calc == Calc::Discrete {
        return if local >= 1.0 { b } else { a };
    }
    a + (b - a) * local
}

fn motion_on_path(el: &Element, p: f64) -> String {
    let fraction = motion_fraction(el, p).clamp(0.0, 1.0);
    let (x, y, angle) = path_point(&el.path, fraction * path_length(&el.path));
    motion_transform(el, x, y, angle)
}

fn motion_from_pair(el: &Element, raw: &str) -> Option<String> {
    let numbers = parse_numbers(raw)?;
    (numbers.values.len() >= 2)
        .then(|| motion_transform(el, numbers.values[0], numbers.values[1], 0.0))
}

fn sample_to(el: &Element, under: Option<&str>, p: f64) -> Option<String> {
    let to = el.values[0].as_str();
    let attr = el.attr.as_ref().map_or(&[][..], |a| a.to_bytes());
    let zero;
    let from = if el.kind == Kind::Animate {
        under.or_else(|| default_for(attr))
    } else {
        None
    };
    let from = match from {
        Some(from) => Some(from),
        None => {
            zero = zero_like(to);
            zero.as_deref()
        }
    };
    match from {
        Some(from) => sample(el, &[from, to], p),
        None => Some(to.to_owned()),
    }
}

pub fn eval(el: &Element, active_time: f64, ended: bool, under: Option<&str>) -> Option<String> {
    let (mut p, mut iteration) = (0.0, 0.0);
    if el.dur > 0.0 {
        let q = active_time / el.dur;
        iteration = q.floor();
        p = q - iteration;
        if ended && iteration > 0.0 && p < 1e-9 {
            iteration -= 1.0;
            p = 1.0;
        }
    }
    if el.kind == Kind::Motion && !el.path.is_empty() {
        return Some(motion_on_path(el, p));
    }
    let values: Vec<&str> = el.values.iter().map(String::as_str).collect();
    let last = *values.last()?;
    let mut raw = if el.kind == Kind::Set {
        values[0].to_owned()
    } else if el.to_anim {
        sample_to(el, under, p)?
    } else {
        sample(el, &values, p)?
    };

    if el.accumulate
        && iteration > 0.0
        && !el.to_anim
        && el.kind != Kind::Set
        && let Some(sum) = combine(&raw, last, 1.0, iteration, el.is_color)
    {
        raw = sum;
    }

    match el.kind {
        Kind::Motion => motion_from_pair(el, &raw),
        Kind::Transform => Some(match under.filter(|_| el.additive) {
            Some(under) => format!("{under} {}({raw})", el.transform_type),
            None => format!("{}({raw})", el.transform_type),
        }),
        Kind::Animate if el.additive && !el.to_anim => {
            let attr = el.attr.as_ref().map_or(&[][..], |a| a.to_bytes());
            let base = under.or_else(|| default_for(attr));
            Some(
                base.and_then(|base| combine(base, &raw, 1.0, 1.0, el.is_color))
                    .unwrap_or(raw),
            )
        }
        _ => Some(raw),
    }
}
