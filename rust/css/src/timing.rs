//! Southstar — CSS easing functions: the keywords, linear(), steps() and cubic-bezier() parsed into ns_css_timing, and their canonical text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use crate::ffi;
use crate::scan::{starts_with_ci, strip, strtol10};
use crate::text::split_top_level_commas;
use crate::units::number_text;

pub(crate) const LINEAR: i32 = 0;
pub(crate) const EASE: i32 = 1;
pub(crate) const EASE_IN: i32 = 2;
pub(crate) const EASE_OUT: i32 = 3;
pub(crate) const EASE_IN_OUT: i32 = 4;
pub(crate) const STEPS: i32 = 5;
pub(crate) const CUBIC: i32 = 6;

pub(crate) const JUMP_END: i32 = 0;
pub(crate) const JUMP_START: i32 = 1;
pub(crate) const JUMP_NONE: i32 = 2;
pub(crate) const JUMP_BOTH: i32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Timing {
    pub kind: i32,
    pub steps: i32,
    pub step_pos: i32,
    pub jump_keyword: i32,
    pub cb: [f64; 4],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<Timing>() == 48);

impl Timing {
    pub(crate) fn of(kind: i32) -> Timing {
        Timing {
            kind,
            ..Timing::default()
        }
    }
}

const KEYWORDS: [(&[u8], i32); 5] = [
    (b"linear", LINEAR),
    (b"ease", EASE),
    (b"ease-in", EASE_IN),
    (b"ease-out", EASE_OUT),
    (b"ease-in-out", EASE_IN_OUT),
];

pub(crate) fn keyword_matches(kw: &[u8]) -> bool {
    KEYWORDS
        .iter()
        .any(|(name, _)| kw.eq_ignore_ascii_case(name))
        || kw.eq_ignore_ascii_case(b"step-start")
        || kw.eq_ignore_ascii_case(b"step-end")
}

fn keyword(kw: &[u8]) -> Timing {
    if let Some((_, kind)) = KEYWORDS
        .iter()
        .find(|(name, _)| kw.eq_ignore_ascii_case(name))
    {
        return Timing::of(*kind);
    }
    let step_pos = if kw.eq_ignore_ascii_case(b"step-start") {
        JUMP_START
    } else if kw.eq_ignore_ascii_case(b"step-end") {
        JUMP_END
    } else {
        return Timing::of(EASE);
    };
    Timing {
        kind: STEPS,
        steps: 1,
        step_pos,
        ..Timing::default()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LinearPoint {
    pub output: f64,
    pub input: f64,
}

static LINEAR_FUNCTIONS: Mutex<Vec<Arc<[LinearPoint]>>> = Mutex::new(Vec::new());

fn intern_linear(points: Vec<LinearPoint>) -> i32 {
    let mut functions = LINEAR_FUNCTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let index = match functions.iter().position(|f| **f == *points) {
        Some(index) => index,
        None => {
            functions.push(points.into());
            functions.len() - 1
        }
    };
    i32::try_from(index + 1).unwrap_or(0)
}

pub fn linear_points(t: &Timing) -> Option<Arc<[LinearPoint]>> {
    if t.kind != LINEAR || t.steps <= 0 {
        return None;
    }
    let functions = LINEAR_FUNCTIONS.lock().unwrap_or_else(|e| e.into_inner());
    functions.get(usize::try_from(t.steps - 1).ok()?).cloned()
}

pub fn linear_eval(t: &Timing, progress: f64) -> Option<f64> {
    let points = linear_points(t)?;
    Some(linear_output(&points, progress))
}

pub(crate) fn linear_output(points: &[LinearPoint], progress: f64) -> f64 {
    if points.len() < 2 {
        return progress;
    }
    let mut a = points
        .iter()
        .rposition(|p| p.input <= progress)
        .unwrap_or(0);
    if a == points.len() - 1 {
        a -= 1;
    }
    let (from, to) = (points[a], points[a + 1]);
    if from.input == to.input {
        return to.output;
    }
    let fraction = (progress - from.input) / (to.input - from.input);
    from.output + fraction * (to.output - from.output)
}

struct LinearStop {
    output: f64,
    inputs: Vec<f64>,
    text: Vec<u8>,
}

fn split_spaces(text: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, &c) in text.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && c.is_ascii_whitespace() {
            if let Some(s) = start.take() {
                out.push(&text[s..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

fn plain_number(token: &[u8]) -> Option<f64> {
    let first = *token.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'-' | b'+' | b'.')) {
        return None;
    }
    let text = CString::new(token).ok()?;
    let (v, end) = ffi::strtod(&text, 0);
    (end == token.len() && v.is_finite()).then_some(v)
}

fn percentage(token: &[u8]) -> Option<(f64, Vec<u8>)> {
    if let Some(number) = token.strip_suffix(b"%") {
        let v = plain_number(number)?;
        let mut text = number_text(v);
        text.push(b'%');
        return Some((v, text));
    }
    let math = [&b"calc("[..], b"min(", b"max(", b"clamp("];
    if !math.iter().any(|f| starts_with_ci(token, f)) || token.last() != Some(&b')') {
        return None;
    }
    let (ok, resolved) = crate::calc::resolve_to_px_pct(token, false);
    if !ok || resolved.px != 0.0 || resolved.em != 0.0 || resolved.rem != 0.0 {
        return None;
    }
    let v = resolved.pct;
    if !v.is_finite() {
        return None;
    }
    let mut text = b"calc(".to_vec();
    text.extend_from_slice(&number_text(v));
    text.extend_from_slice(b"%)");
    Some((v, text))
}

fn linear_stop(entry: &[u8]) -> Option<LinearStop> {
    let tokens = split_spaces(strip(entry));
    let (number_at, output) = tokens
        .iter()
        .enumerate()
        .find_map(|(i, tok)| plain_number(tok).map(|v| (i, v)))?;
    let rest: Vec<&[u8]> = tokens
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != number_at)
        .map(|(_, tok)| *tok)
        .collect();
    if rest.len() > 2 || (number_at != 0 && number_at != tokens.len() - 1) {
        return None;
    }
    let mut text = number_text(output);
    let mut inputs = Vec::new();
    for tok in rest {
        let (v, written) = percentage(tok)?;
        inputs.push(v);
        text.push(b' ');
        text.extend_from_slice(&written);
    }
    Some(LinearStop {
        output,
        inputs,
        text,
    })
}

fn linear_stops(item: &[u8]) -> Option<Vec<LinearStop>> {
    let body = &item[b"linear(".len()..item.len() - 1];
    let entries = split_top_level_commas(body);
    if entries.len() < 2 {
        return None;
    }
    entries.iter().map(|e| linear_stop(e)).collect()
}

fn linear_points_of(stops: &[LinearStop]) -> Vec<LinearPoint> {
    let mut points: Vec<(f64, Option<f64>)> = Vec::new();
    for stop in stops {
        if stop.inputs.is_empty() {
            points.push((stop.output, None));
        }
        for &input in &stop.inputs {
            points.push((stop.output, Some(input)));
        }
    }
    if let Some(first) = points.first_mut()
        && first.1.is_none()
    {
        first.1 = Some(0.0);
    }
    if let Some(last) = points.last_mut()
        && last.1.is_none()
    {
        last.1 = Some(100.0);
    }
    let mut largest = f64::NEG_INFINITY;
    for point in &mut points {
        if let Some(input) = point.1.as_mut() {
            if *input < largest {
                *input = largest;
            } else {
                largest = *input;
            }
        }
    }
    let mut i = 0;
    while i < points.len() {
        if points[i].1.is_some() {
            i += 1;
            continue;
        }
        let start = i - 1;
        let end = (i..points.len())
            .find(|&j| points[j].1.is_some())
            .unwrap_or(points.len() - 1);
        let (from, to) = (
            points[start].1.unwrap_or(0.0),
            points[end].1.unwrap_or(100.0),
        );
        let gaps = (end - start) as f64;
        for (k, point) in points[start + 1..end].iter_mut().enumerate() {
            point.1 = Some(from + (to - from) * (k + 1) as f64 / gaps);
        }
        i = end;
    }
    points
        .into_iter()
        .map(|(output, input)| LinearPoint {
            output,
            input: input.unwrap_or(0.0) / 100.0,
        })
        .collect()
}

fn linear(item: &[u8]) -> Option<Timing> {
    let stops = linear_stops(item)?;
    Some(Timing {
        steps: intern_linear(linear_points_of(&stops)),
        ..Timing::of(LINEAR)
    })
}

fn linear_specified(item: &[u8]) -> Option<Vec<u8>> {
    let stops = linear_stops(item)?;
    let parts: Vec<Vec<u8>> = stops.into_iter().map(|s| s.text).collect();
    let mut out = b"linear(".to_vec();
    out.extend_from_slice(&parts.join(&b", "[..]));
    out.push(b')');
    Some(out)
}

fn fixed6(v: f64) -> String {
    let text = southstar_numfmt::fixed(v, 6);
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if text == "-0" {
        String::from("0")
    } else {
        text
    }
}

fn linear_computed(points: &[LinearPoint]) -> Vec<u8> {
    let parts: Vec<String> = points
        .iter()
        .map(|p| format!("{} {}%", fixed6(p.output), fixed6(p.input * 100.0)))
        .collect();
    format!("linear({})", parts.join(", ")).into_bytes()
}

fn split_commas(body: &[u8]) -> Vec<&[u8]> {
    if body.is_empty() {
        return Vec::new();
    }
    body.split(|&c| c == b',').collect()
}

fn steps(parts: &[&[u8]]) -> Option<Timing> {
    let count = strip(parts[0]);
    let (n, end) = strtol10(count);
    if end == 0 || end != count.len() || n < 1 {
        return None;
    }
    let mut pos = JUMP_END;
    let mut jump_keyword = false;
    if let Some(k) = parts.get(1) {
        let k = strip(k);
        jump_keyword = starts_with_ci(k, b"jump-");
        pos = if k.eq_ignore_ascii_case(b"jump-start") || k.eq_ignore_ascii_case(b"start") {
            JUMP_START
        } else if k.eq_ignore_ascii_case(b"jump-end") || k.eq_ignore_ascii_case(b"end") {
            JUMP_END
        } else if k.eq_ignore_ascii_case(b"jump-none") {
            JUMP_NONE
        } else if k.eq_ignore_ascii_case(b"jump-both") {
            JUMP_BOTH
        } else {
            return None;
        };
    }
    if pos == JUMP_NONE && n < 2 {
        return None;
    }
    Some(Timing {
        kind: STEPS,
        steps: n as i32,
        step_pos: pos,
        jump_keyword: i32::from(jump_keyword),
        cb: [0.0; 4],
    })
}

fn cubic(parts: &[&[u8]]) -> Option<Timing> {
    let mut cb = [0.0f64; 4];
    for (slot, part) in cb.iter_mut().zip(parts) {
        let text = CString::new(strip(part)).ok()?;
        let (v, end) = ffi::strtod(&text, 0);
        if end == 0 || end != text.to_bytes().len() {
            return None;
        }
        *slot = v;
    }
    if cb[0] < 0.0 || cb[0] > 1.0 || cb[2] < 0.0 || cb[2] > 1.0 {
        return None;
    }
    Some(Timing {
        cb,
        ..Timing::of(CUBIC)
    })
}

pub(crate) fn item_parse(item: &[u8]) -> Option<Timing> {
    let item = &item[..item.iter().position(|&c| c == 0).unwrap_or(item.len())];
    let last = *item.last()?;
    if keyword_matches(item) {
        return Some(keyword(item));
    }
    if starts_with_ci(item, b"linear(") && last == b')' {
        return linear(item);
    }
    let is_steps = starts_with_ci(item, b"steps(");
    let is_cubic = starts_with_ci(item, b"cubic-bezier(");
    if (!is_steps && !is_cubic) || last != b')' {
        return None;
    }
    let body = &item[if is_steps { 6 } else { 13 }..item.len() - 1];
    let parts = split_commas(body);
    match parts.len() {
        1 | 2 if is_steps => steps(&parts),
        4 if is_cubic => cubic(&parts),
        _ => None,
    }
}

pub fn parse(text: &[u8]) -> Option<Timing> {
    item_parse(strip(text))
}

pub fn easing_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let item = strip(text);
    let timing = item_parse(item)?;
    if starts_with_ci(item, b"linear(") {
        return linear_specified(item);
    }
    Some(serialize(Some(&timing)))
}

pub(crate) fn item_specified(item: &[u8]) -> Vec<u8> {
    if starts_with_ci(item, b"linear(") && item.last() == Some(&b')') {
        return linear_specified(item).unwrap_or_else(|| item.to_ascii_lowercase());
    }
    item.to_ascii_lowercase()
}

fn canonical(text: &[u8]) -> Vec<u8> {
    serialize(Some(&parse(text).unwrap_or(Timing::of(EASE))))
}

pub fn list_canonical(text: &[u8]) -> Vec<u8> {
    let items: Vec<Vec<u8>> = split_top_level_commas(text)
        .iter()
        .map(|item| canonical(item))
        .collect();
    items.join(&b", "[..])
}

pub fn serialize(t: Option<&Timing>) -> Vec<u8> {
    let Some(t) = t else {
        return b"ease".to_vec();
    };
    match t.kind {
        LINEAR => linear_points(t).map_or_else(|| b"linear".to_vec(), |p| linear_computed(&p)),
        EASE_IN => b"ease-in".to_vec(),
        EASE_OUT => b"ease-out".to_vec(),
        EASE_IN_OUT => b"ease-in-out".to_vec(),
        STEPS => {
            let suffix: &str = match t.step_pos {
                JUMP_START if t.jump_keyword != 0 => ", jump-start",
                JUMP_START => ", start",
                JUMP_NONE => ", jump-none",
                JUMP_BOTH => ", jump-both",
                _ => "",
            };
            format!("steps({}{suffix})", t.steps).into_bytes()
        }
        CUBIC => {
            let mut out = b"cubic-bezier(".to_vec();
            for (i, v) in t.cb.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b", ");
                }
                out.extend_from_slice(&number_text(*v));
            }
            out.push(b')');
            out
        }
        _ => b"ease".to_vec(),
    }
}
