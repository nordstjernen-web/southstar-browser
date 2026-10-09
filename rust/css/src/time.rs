//! Southstar — CSS time values: checking that an expression of numbers, s and ms and the math functions is a time, its value in seconds, and the specified and computed spelling of a list of them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};

use crate::calc::{self, Parsed};
use crate::ffi;
use crate::scan::{is_ws, match_close_paren};
use crate::units::number_text;

const MAX_DEPTH: i32 = 64;

#[derive(Clone, Copy, PartialEq)]
enum TVal {
    Invalid,
    Number,
    Time,
}

struct Expr<'a> {
    text: &'a CStr,
    s: &'a [u8],
}

fn name_is(name: &[u8], lit: &[u8]) -> bool {
    name.eq_ignore_ascii_case(lit)
}

impl Expr<'_> {
    fn func(&self, name: &[u8], s: usize, e: usize, nest: i32) -> TVal {
        let mut args: Vec<(usize, usize)> = Vec::new();
        let mut depth = 0i32;
        let mut seg = s;
        for c in s..=e {
            let ch = if c < e { self.s[c] } else { 0 };
            if c < e && ch == b'(' {
                depth += 1;
            } else if c < e && ch == b')' {
                depth -= 1;
            } else if c == e || (ch == b',' && depth == 0) {
                if args.len() >= 8 {
                    return TVal::Invalid;
                }
                args.push((seg, c));
                seg = c + 1;
            }
        }
        let at: Vec<TVal> = args
            .iter()
            .map(|&(a, b)| self.sum_depth(a, b, nest + 1))
            .collect();
        let n = at.len();
        let all_same =
            |from: usize| at[from] != TVal::Invalid && at[from..].iter().all(|&t| t == at[from]);
        if name_is(name, b"calc") || name_is(name, b"abs") {
            return if n == 1 { at[0] } else { TVal::Invalid };
        }
        if name_is(name, b"min") || name_is(name, b"max") || name_is(name, b"hypot") {
            return if n >= 1 && all_same(0) {
                at[0]
            } else {
                TVal::Invalid
            };
        }
        if name_is(name, b"clamp") {
            return if n == 3 && all_same(0) {
                at[0]
            } else {
                TVal::Invalid
            };
        }
        if name_is(name, b"sign") {
            return if n == 1 && at[0] != TVal::Invalid {
                TVal::Number
            } else {
                TVal::Invalid
            };
        }
        if name_is(name, b"mod") || name_is(name, b"rem") {
            return if n == 2 && at[0] != TVal::Invalid && at[0] == at[1] {
                at[0]
            } else {
                TVal::Invalid
            };
        }
        if name_is(name, b"round") {
            let first = &self.s[args[0].0..args[0].1];
            let base = usize::from(
                [&b"nearest"[..], b"up", b"down", b"to-zero"]
                    .iter()
                    .any(|strategy| name_is(first, strategy)),
            );
            let count = n - base;
            if !(1..=2).contains(&count) {
                return TVal::Invalid;
            }
            return if all_same(base) {
                at[base]
            } else {
                TVal::Invalid
            };
        }
        if [
            &b"sqrt"[..],
            b"exp",
            b"log",
            b"pow",
            b"sin",
            b"cos",
            b"tan",
            b"asin",
            b"acos",
            b"atan",
            b"atan2",
        ]
        .iter()
        .any(|f| name_is(name, f))
        {
            return if at.iter().all(|&t| t == TVal::Number) {
                TVal::Number
            } else {
                TVal::Invalid
            };
        }
        TVal::Invalid
    }

    fn factor(&self, pos: &mut usize, e: usize, depth: i32) -> TVal {
        let s = self.s;
        let mut p = *pos;
        while p < e && is_ws(s[p]) {
            p += 1;
        }
        if p >= e {
            *pos = p;
            return TVal::Invalid;
        }
        if s[p] == b'(' {
            let Some(close) = match_close_paren(s, p + 1, e) else {
                *pos = e;
                return TVal::Invalid;
            };
            let t = self.sum_depth(p + 1, close, depth + 1);
            *pos = close + 1;
            return t;
        }
        if s[p].is_ascii_alphabetic() {
            let id = p;
            while p < e && (s[p].is_ascii_alphabetic() || s[p] == b'-') {
                p += 1;
            }
            let name = &s[id..p];
            if p < e && s[p] == b'(' {
                let Some(close) = match_close_paren(s, p + 1, e) else {
                    *pos = e;
                    return TVal::Invalid;
                };
                let t = self.func(name, p + 1, close, depth);
                *pos = close + 1;
                return t;
            }
            *pos = p;
            return if [&b"pi"[..], b"e", b"infinity", b"nan"]
                .iter()
                .any(|c| name_is(name, c))
            {
                TVal::Number
            } else {
                TVal::Invalid
            };
        }
        let (_, num_end) = ffi::strtod(self.text, p);
        if num_end == p {
            *pos = p;
            return TVal::Invalid;
        }
        let mut u = num_end;
        if u < e && s[u] == b'%' {
            *pos = u + 1;
            return TVal::Invalid;
        }
        let unit_start = u;
        while u < e && s[u].is_ascii_alphabetic() {
            u += 1;
        }
        *pos = u;
        let unit = &s[unit_start.min(u)..u];
        if unit.is_empty() {
            TVal::Number
        } else if name_is(unit, b"s") || name_is(unit, b"ms") {
            TVal::Time
        } else {
            TVal::Invalid
        }
    }

    fn product(&self, pos: &mut usize, e: usize, depth: i32) -> TVal {
        let s = self.s;
        let mut p = *pos;
        let mut acc = self.factor(&mut p, e, depth);
        if acc == TVal::Invalid {
            *pos = p;
            return TVal::Invalid;
        }
        loop {
            let mut q = p;
            while q < e && is_ws(s[q]) {
                q += 1;
            }
            if q >= e || (s[q] != b'*' && s[q] != b'/') {
                p = q;
                break;
            }
            let op = s[q];
            let mut r = q + 1;
            let rhs = self.factor(&mut r, e, depth);
            if rhs == TVal::Invalid {
                *pos = r;
                return TVal::Invalid;
            }
            acc = match (op, acc, rhs) {
                (b'*', TVal::Number, TVal::Number) => TVal::Number,
                (b'*', TVal::Number, TVal::Time) | (b'*', TVal::Time, TVal::Number) => TVal::Time,
                (b'*', _, _) => {
                    *pos = r;
                    return TVal::Invalid;
                }
                (_, TVal::Time, TVal::Time) => TVal::Number,
                (_, _, TVal::Number) => acc,
                _ => {
                    *pos = r;
                    return TVal::Invalid;
                }
            };
            p = r;
        }
        *pos = p;
        acc
    }

    fn sum_depth(&self, start: usize, e: usize, depth: i32) -> TVal {
        if depth > MAX_DEPTH {
            return TVal::Invalid;
        }
        let s = self.s;
        let mut p = start;
        while p < e && is_ws(s[p]) {
            p += 1;
        }
        if p >= e {
            return TVal::Invalid;
        }
        let acc = self.product(&mut p, e, depth);
        if acc == TVal::Invalid {
            return TVal::Invalid;
        }
        loop {
            while p < e && is_ws(s[p]) {
                p += 1;
            }
            if p >= e {
                break;
            }
            if s[p] != b'+' && s[p] != b'-' {
                return TVal::Invalid;
            }
            p += 1;
            let rhs = self.product(&mut p, e, depth);
            if rhs == TVal::Invalid || rhs != acc {
                return TVal::Invalid;
            }
        }
        acc
    }
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn is_time_in(text: &CStr, s: usize, e: usize) -> bool {
    let expr = Expr {
        text,
        s: text.to_bytes(),
    };
    expr.sum_depth(s, e, 0) == TVal::Time
}

pub(crate) fn is_time(item: &[u8]) -> bool {
    let text = c_text(item);
    let len = text.to_bytes().len();
    is_time_in(&text, 0, len)
}

fn top_level_segments(s: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut seg = 0;
    for c in 0..=s.len() {
        let ch = s.get(c).copied().unwrap_or(0);
        if c < s.len() && ch == b'(' {
            depth += 1;
        } else if c < s.len() && ch == b')' {
            depth -= 1;
        } else if c == s.len() || (ch == b',' && depth == 0) {
            out.push((seg, c));
            seg = c + 1;
        }
    }
    out
}

pub(crate) fn property_valid(t: &[u8]) -> bool {
    let text = c_text(t);
    top_level_segments(text.to_bytes())
        .iter()
        .all(|&(a, b)| is_time_in(&text, a, b))
}

fn strip_units(text: &CStr, s: usize, e: usize) -> Vec<u8> {
    let b = text.to_bytes();
    let mut out = Vec::new();
    let mut p = s;
    while p < e {
        if b[p].is_ascii_alphabetic() {
            while p < e && (b[p].is_ascii_alphabetic() || b[p] == b'-') {
                out.push(b[p]);
                p += 1;
            }
            continue;
        }
        let next = b.get(p + 1).copied().unwrap_or(0);
        let num_start = b[p].is_ascii_digit()
            || b[p] == b'.'
            || ((b[p] == b'+' || b[p] == b'-')
                && p + 1 < e
                && (next.is_ascii_digit() || next == b'.'));
        if num_start {
            let (num, num_end) = ffi::strtod(text, p);
            if num_end == p {
                out.push(b[p]);
                p += 1;
                continue;
            }
            let mut u = num_end;
            while u < e && b[u].is_ascii_alphabetic() {
                u += 1;
            }
            let unit = &b[num_end.min(u)..u];
            if unit.eq_ignore_ascii_case(b"ms") {
                out.push(b'(');
                out.extend_from_slice(&ffi::format_double(c"%.17g", num));
                out.extend_from_slice(b"*0.001)");
            } else if unit.eq_ignore_ascii_case(b"s") {
                out.extend_from_slice(&b[p..num_end]);
            } else {
                out.extend_from_slice(&b[p..num_end]);
                out.extend_from_slice(unit);
            }
            p = u.max(num_end);
            continue;
        }
        out.push(b[p]);
        p += 1;
    }
    out
}

fn seconds_in(text: &CStr, s: usize, e: usize) -> Option<f64> {
    if !is_time_in(text, s, e) {
        return None;
    }
    let stripped = strip_units(text, s, e);
    let parsed = calc::parse_calc(&c_text(&stripped)).or_else(|| {
        let mut wrapped = b"calc(".to_vec();
        wrapped.extend_from_slice(&stripped);
        wrapped.push(b')');
        calc::parse_calc(&c_text(&wrapped))
    })?;
    match parsed {
        Parsed::Length(v, _) => Some(v),
        Parsed::Calc(_) => None,
    }
}

pub(crate) fn seconds(item: &[u8]) -> Option<f64> {
    let text = c_text(item);
    let len = text.to_bytes().len();
    seconds_in(&text, 0, len)
}

pub(crate) fn starts_math_fn(s: &[u8]) -> bool {
    let start = s.iter().position(|&c| !is_ws(c)).unwrap_or(s.len());
    let s = &s[start..];
    [
        &b"calc("[..],
        b"min(",
        b"max(",
        b"clamp(",
        b"round(",
        b"mod(",
        b"rem(",
        b"abs(",
        b"hypot(",
        b"sign(",
    ]
    .iter()
    .any(|f| s.len() >= f.len() && s[..f.len()].eq_ignore_ascii_case(f))
}

pub(crate) fn list_serialize(value: &[u8], computed: bool) -> Option<Vec<u8>> {
    let text = c_text(value);
    let b = text.to_bytes();
    let mut out = Vec::new();
    let mut changed = false;
    for (i, &(seg, c)) in top_level_segments(b).iter().enumerate() {
        let mut is = seg;
        let mut ie = c;
        while is < ie && is_ws(b[is]) {
            is += 1;
        }
        while ie > is && is_ws(b[ie - 1]) {
            ie -= 1;
        }
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        let mut did = false;
        if computed || starts_math_fn(&b[is..ie]) {
            if let Some(sec) = seconds_in(&text, is, ie) {
                if computed {
                    if sec.is_finite() {
                        out.extend_from_slice(&number_text(sec));
                        out.push(b's');
                        did = true;
                    }
                } else if sec.is_finite() {
                    out.extend_from_slice(b"calc(");
                    out.extend_from_slice(&number_text(sec));
                    out.extend_from_slice(b"s)");
                    did = true;
                } else if sec.is_nan() {
                    out.extend_from_slice(b"calc(NaN * 1s)");
                    did = true;
                } else if sec < 0.0 {
                    out.extend_from_slice(b"calc(-infinity * 1s)");
                    did = true;
                } else {
                    out.extend_from_slice(b"calc(infinity * 1s)");
                    did = true;
                }
            }
        }
        changed |= did;
        if !did {
            out.extend_from_slice(&b[is..ie]);
        }
    }
    changed.then_some(out)
}

pub(crate) fn time_text(ms: f64) -> Vec<u8> {
    let mut out = number_text(ms / 1000.0);
    out.push(b's');
    out
}

pub(crate) fn parse_ms(tok: &[u8]) -> Option<f64> {
    let text = c_text(tok);
    let (v, end) = ffi::strtod(&text, 0);
    if end == 0 {
        return None;
    }
    let unit = &text.to_bytes()[end..];
    if unit.eq_ignore_ascii_case(b"ms") {
        Some(v)
    } else if unit.eq_ignore_ascii_case(b"s") {
        Some(v * 1000.0)
    } else {
        None
    }
}
