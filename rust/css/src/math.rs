//! Southstar — the canonical spelling of CSS math values: sums of numbers, percentages and dimensions folded to one calc() with their units converted and sorted, and other math functions reduced to their computed calc() when they resolve without a percentage.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use std::ffi::{CStr, CString};

use crate::calc::{self, MAX_DEPTH, Parsed};
use crate::ffi;
use crate::scan::{is_ws, skip_ws, starts_with_ci};
use crate::units;

const MAX_TERMS: usize = 16;
const MAX_UNIT: usize = 7;

#[derive(Clone, Default)]
struct MathSum {
    number: f64,
    has_number: bool,
    percent: f64,
    has_percent: bool,
    dims: Vec<(Vec<u8>, f64)>,
}

const CONVERTIBLE: &[(&[u8], &[u8], f64)] = &[
    (b"px", b"px", 1.0),
    (b"cm", b"px", 96.0 / 2.54),
    (b"mm", b"px", 96.0 / 25.4),
    (b"q", b"px", 96.0 / 101.6),
    (b"in", b"px", 96.0),
    (b"pt", b"px", 96.0 / 72.0),
    (b"pc", b"px", 16.0),
    (b"deg", b"deg", 1.0),
    (b"grad", b"deg", 0.9),
    (b"rad", b"deg", 180.0 / PI),
    (b"turn", b"deg", 360.0),
    (b"s", b"s", 1.0),
    (b"ms", b"s", 0.001),
    (b"hz", b"hz", 1.0),
    (b"khz", b"hz", 1000.0),
    (b"dppx", b"dppx", 1.0),
    (b"x", b"dppx", 1.0),
    (b"dpi", b"dppx", 1.0 / 96.0),
    (b"dpcm", b"dppx", 2.54 / 96.0),
];

const RELATIVE: &[&[u8]] = &[
    b"em", b"rem", b"ex", b"rex", b"ch", b"rch", b"cap", b"rcap", b"ic", b"ric", b"lh", b"rlh",
    b"vw", b"vh", b"vi", b"vb", b"vmin", b"vmax", b"svw", b"svh", b"svi", b"svb", b"svmin",
    b"svmax", b"lvw", b"lvh", b"lvi", b"lvb", b"lvmin", b"lvmax", b"dvw", b"dvh", b"dvi", b"dvb",
    b"dvmin", b"dvmax", b"cqw", b"cqh", b"cqi", b"cqb", b"cqmin", b"cqmax", b"fr",
];

fn unit_canonical(unit: &[u8]) -> Option<(&'static [u8], f64)> {
    if unit.is_empty() || unit.len() > MAX_UNIT {
        return None;
    }
    if let Some(&(_, canonical, factor)) = CONVERTIBLE
        .iter()
        .find(|(name, _, _)| unit.eq_ignore_ascii_case(name))
    {
        return Some((canonical, factor));
    }
    RELATIVE
        .iter()
        .find(|name| unit.eq_ignore_ascii_case(name))
        .map(|&name| (name, 1.0))
}

impl MathSum {
    fn add_dim(&mut self, unit: &[u8], v: f64) -> bool {
        if let Some(dim) = self.dims.iter_mut().find(|(known, _)| known == unit) {
            dim.1 += v;
            return true;
        }
        if self.dims.len() >= MAX_TERMS {
            return false;
        }
        self.dims
            .push((unit[..unit.len().min(MAX_UNIT)].to_vec(), v));
        true
    }

    fn add(&mut self, other: &MathSum, sign: f64) -> bool {
        if other.has_number {
            self.number += sign * other.number;
            self.has_number = true;
        }
        if other.has_percent {
            self.percent += sign * other.percent;
            self.has_percent = true;
        }
        other
            .dims
            .iter()
            .all(|(unit, coeff)| self.add_dim(unit, sign * coeff))
    }

    fn scale(&mut self, m: f64) {
        self.number *= m;
        self.percent *= m;
        for dim in &mut self.dims {
            dim.1 *= m;
        }
    }

    fn is_number(&self) -> bool {
        self.has_number && !self.has_percent && self.dims.is_empty()
    }
}

fn number_token(text: &[u8], pos: &mut usize, end: usize) -> Option<MathSum> {
    let mut p = *pos;
    let start = p;
    if p < end && (text[p] == b'+' || text[p] == b'-') {
        p += 1;
    }
    let digits = p;
    while p < end && (text[p].is_ascii_digit() || text[p] == b'.') {
        p += 1;
    }
    if p == digits {
        return None;
    }
    if p < end && (text[p] == b'e' || text[p] == b'E') {
        let mut exponent = p + 1;
        if exponent < end && (text[exponent] == b'+' || text[exponent] == b'-') {
            exponent += 1;
        }
        let exponent_digits = exponent;
        while exponent < end && text[exponent].is_ascii_digit() {
            exponent += 1;
        }
        if exponent != exponent_digits {
            p = exponent;
        }
    }
    let literal = CString::new(&text[start..p]).ok()?;
    let (v, tail) = ffi::strtod(&literal, 0);
    if tail != literal.to_bytes().len() || !v.is_finite() {
        return None;
    }
    let mut out = MathSum::default();
    if p < end && text[p] == b'%' {
        out.has_percent = true;
        out.percent = v;
        p += 1;
    } else {
        let unit = p;
        while p < end && text[p].is_ascii_alphabetic() {
            p += 1;
        }
        if p == unit {
            out.has_number = true;
            out.number = v;
        } else {
            let (canonical, factor) = unit_canonical(&text[unit..p])?;
            out.add_dim(canonical, v * factor);
        }
    }
    *pos = p;
    Some(out)
}

fn primary(text: &[u8], pos: &mut usize, end: usize, depth: i32) -> Option<MathSum> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut p = skip_ws(text, *pos, end);
    if p >= end {
        return None;
    }
    if end - p > 5 && starts_with_ci(&text[p..], b"calc(") {
        p += 5;
    } else if text[p] == b'(' {
        p += 1;
    } else {
        let out = number_token(text, &mut p, end)?;
        *pos = p;
        return Some(out);
    }
    let out = sum(text, &mut p, end, depth + 1)?;
    p = skip_ws(text, p, end);
    if p >= end || text[p] != b')' {
        return None;
    }
    *pos = p + 1;
    Some(out)
}

fn product(text: &[u8], pos: &mut usize, end: usize, depth: i32) -> Option<MathSum> {
    let mut out = primary(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(text, *pos, end);
        if p >= end || (text[p] != b'*' && text[p] != b'/') {
            return Some(out);
        }
        let op = text[p];
        p += 1;
        let rhs = primary(text, &mut p, end, depth)?;
        if op == b'/' {
            if !rhs.is_number() || rhs.number == 0.0 {
                return None;
            }
            out.scale(1.0 / rhs.number);
        } else if rhs.is_number() {
            out.scale(rhs.number);
        } else if out.is_number() {
            let scale = out.number;
            out = rhs;
            out.scale(scale);
        } else {
            return None;
        }
        *pos = p;
    }
}

fn sum(text: &[u8], pos: &mut usize, end: usize, depth: i32) -> Option<MathSum> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut out = product(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(text, *pos, end);
        if p == *pos || p >= end || (text[p] != b'+' && text[p] != b'-') {
            return Some(out);
        }
        let op = text[p];
        p += 1;
        if p >= end || !is_ws(text[p]) {
            return None;
        }
        let rhs = product(text, &mut p, end, depth)?;
        if !out.add(&rhs, if op == b'+' { 1.0 } else { -1.0 }) {
            return None;
        }
        *pos = p;
    }
}

fn ascii_case_cmp(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    a.iter()
        .map(u8::to_ascii_lowercase)
        .cmp(b.iter().map(u8::to_ascii_lowercase))
}

fn serialize(sum: &MathSum) -> Option<Vec<u8>> {
    let mut terms: Vec<(&[u8], f64)> = Vec::new();
    if sum.has_number {
        terms.push((b"", sum.number));
    }
    if sum.has_percent {
        terms.push((b"%", sum.percent));
    }
    let first_dim = terms.len();
    terms.extend(
        sum.dims
            .iter()
            .map(|(unit, coeff)| (unit.as_slice(), *coeff)),
    );
    terms[first_dim..].sort_by(|a, b| ascii_case_cmp(a.0, b.0));
    if terms.is_empty() {
        return None;
    }
    let mut out = b"calc(".to_vec();
    for (i, &(unit, v)) in terms.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(if v < 0.0 { b" - " } else { b" + " });
        }
        out.extend_from_slice(&units::number_text(if i > 0 { v.abs() } else { v }));
        out.extend_from_slice(unit);
    }
    out.push(b')');
    Some(out)
}

fn sum_canonical(value: &[u8]) -> Option<Vec<u8>> {
    let p = skip_ws(value, 0, value.len());
    let rest = &value[p..];
    let body = if starts_with_ci(rest, b"calc(") {
        p + 5
    } else if starts_with_ci(rest, b"min(") || starts_with_ci(rest, b"max(") {
        p + 4
    } else {
        return None;
    };
    let mut end = value.len();
    while end > body && is_ws(value[end - 1]) {
        end -= 1;
    }
    if end <= body || value[end - 1] != b')' {
        return None;
    }
    end -= 1;
    let mut cursor = body;
    let total = sum(value, &mut cursor, end, 0)?;
    if skip_ws(value, cursor, end) != end {
        return None;
    }
    serialize(&total)
}

fn nonfinite_length(v: f64, unit: &[u8]) -> Vec<u8> {
    let word: &[u8] = if v.is_nan() {
        b"NaN"
    } else if v < 0.0 {
        b"-infinity"
    } else {
        b"infinity"
    };
    let mut out = b"calc(".to_vec();
    out.extend_from_slice(word);
    if !unit.is_empty() {
        out.extend_from_slice(b" * 1");
        out.extend_from_slice(unit);
    }
    out.push(b')');
    out
}

fn finite_length(v: f64, unit: &[u8]) -> Vec<u8> {
    let mut out = b"calc(".to_vec();
    out.extend_from_slice(&units::number_text(v));
    out.extend_from_slice(unit);
    out.push(b')');
    out
}

const MATH_FUNCTIONS: &[&[u8]] = &[
    b"calc(",
    b"min(",
    b"max(",
    b"clamp(",
    b"round(",
    b"mod(",
    b"rem(",
    b"abs(",
    b"hypot(",
    b"pow(",
    b"sqrt(",
    b"sin(",
    b"cos(",
    b"tan(",
    b"sign(",
    b"exp(",
    b"log(",
    b"progress(",
];

pub(crate) fn math_canonical(text: &CStr) -> Option<Vec<u8>> {
    let all = text.to_bytes();
    let value = &all[skip_ws(all, 0, all.len())..];
    if let Some(sum) = sum_canonical(value) {
        return Some(sum);
    }
    if units::has_relative_unit(value)
        || !MATH_FUNCTIONS
            .iter()
            .any(|name| starts_with_ci(value, name))
    {
        return None;
    }
    let has_pct = value.contains(&b'%');
    let parsed = calc::parse_calc(&CString::new(value).ok()?)?;
    let mut nonfinite = false;
    let number_result =
        matches!(parsed, Parsed::Length(_, units::NUMBER)) && starts_with_ci(value, b"progress(");
    let out = match parsed {
        Parsed::Length(v, unit) => {
            let suffix = units::unit_suffix(unit).to_bytes();
            if v.is_finite() {
                Some(finite_length(v, suffix))
            } else {
                nonfinite = true;
                Some(nonfinite_length(v, suffix))
            }
        }
        Parsed::Calc(calc) => {
            let mut nonzero = 0;
            let mut val = 0.0;
            let mut unit: &[u8] = b"px";
            for (v, name) in [
                (calc.px, &b"px"[..]),
                (calc.pct, b"%"),
                (calc.em, b"em"),
                (calc.rem, b"rem"),
            ] {
                if v != 0.0 {
                    nonzero += 1;
                    val = v;
                    unit = name;
                }
            }
            match nonzero {
                0 => Some(b"calc(0px)".to_vec()),
                1 if val.is_finite() => Some(finite_length(val, unit)),
                1 => {
                    nonfinite = true;
                    Some(nonfinite_length(val, unit))
                }
                _ => None,
            }
        }
    };
    if has_pct && !nonfinite && !number_result {
        return None;
    }
    out
}
