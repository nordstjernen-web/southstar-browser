//! Southstar — calc() and the other CSS math functions: parsing min(), max(), clamp(), the stepped functions, trigonometry, exponents, sign() and progress() to a length, number or deferred calc value, resolving lengths to pixels and percentages, and evaluating deferred functions against a basis.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::f64::consts::{E, PI};
use std::ffi::{CStr, CString};

use crate::ffi;
use crate::scan::{is_ws, match_close_paren, skip_ws, split_args, starts_with_ci, strip};
use crate::units::{
    self, CAP, CH, CQH, CQMAX, CQMIN, CQW, EM, EX, IC, NUMBER, PERCENT, PX, REM, Unit, VH, VMAX,
    VMIN, VW,
};

pub(crate) const MAX_DEPTH: i32 = 64;

pub(crate) const FN_MIN: u8 = 1;
pub(crate) const FN_MAX: u8 = 2;
pub(crate) const FN_CLAMP: u8 = 3;
pub(crate) const FN_ROUND: u8 = 4;
pub(crate) const FN_MOD: u8 = 8;
pub(crate) const FN_REM: u8 = 9;
pub(crate) const FN_ABS: u8 = 10;

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct CalcArg {
    pub px: f64,
    pub pct: f64,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Calc {
    pub pct: f64,
    pub px: f64,
    pub em: f64,
    pub rem: f64,
    pub vw: f64,
    pub vh: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub parsed_vw: f64,
    pub parsed_vh: f64,
    pub func: u8,
    pub n_args: u8,
    pub arg_none: u8,
    pub args: [CalcArg; 4],
}

#[derive(Clone, Copy)]
pub(crate) enum Parsed {
    Length(f64, Unit),
    Calc(Calc),
}

#[derive(Clone, Copy, Default)]
struct Term {
    px: f64,
    pct: f64,
    em: f64,
    rem: f64,
    vw: f64,
    vh: f64,
    vmin: f64,
    vmax: f64,
    num: f64,
    is_number: bool,
    func: u8,
    n_args: u8,
    arg_none: u8,
    args: [CalcArg; 4],
}

fn viewport_w() -> f64 {
    ffi::viewport().0
}

fn number(n: f64) -> Parsed {
    Parsed::Length(n, NUMBER)
}

fn pixels(px: f64) -> Parsed {
    Parsed::Length(px, PX)
}

fn c_text(bytes: Vec<u8>) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    let mut bytes = bytes;
    bytes.truncate(len);
    CString::new(bytes).unwrap_or_default()
}

fn wrapped_in_calc(text: &[u8]) -> CString {
    let mut wrapped = Vec::with_capacity(text.len() + 6);
    wrapped.extend_from_slice(b"calc(");
    wrapped.extend_from_slice(text);
    wrapped.push(b')');
    c_text(wrapped)
}

fn add_viewport_coeff(unit: Unit, v: f64, term: &mut Term) {
    match unit {
        VW => term.vw += v,
        VH => term.vh += v,
        VMIN => term.vmin += v,
        VMAX => term.vmax += v,
        _ => {}
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Resolved {
    pub px: f64,
    pub pct: f64,
    pub em: f64,
    pub rem: f64,
}

fn font_unit_set(out: &mut Resolved, font: bool, unit: Unit, v: f64) {
    if font {
        if unit == EM {
            out.em = v;
        } else {
            out.rem = v;
        }
    } else {
        out.px = v * 16.0;
    }
}

pub(crate) fn resolve_to_px_pct(text: &[u8], font: bool) -> (bool, Resolved) {
    let stripped = c_text(strip(text).to_vec());
    let mut out = Resolved::default();
    let parsed =
        parse_calc(&stripped).or_else(|| parse_calc(&wrapped_in_calc(stripped.to_bytes())));
    match parsed {
        Some(Parsed::Calc(calc)) => {
            if font {
                out.em = calc.em;
                out.rem = calc.rem;
                out.px = calc.px;
            } else {
                let relative = (calc.em + calc.rem) * 16.0;
                out.px = if relative == 0.0 {
                    calc.px
                } else {
                    calc.px + relative
                };
            }
            out.pct = calc.pct;
            return (true, out);
        }
        Some(Parsed::Length(v, unit)) => {
            match unit {
                PERCENT => out.pct = v,
                EM | REM => font_unit_set(&mut out, font, unit, v),
                _ => out.px = v,
            }
            return (true, out);
        }
        None => {}
    }
    let Some((num, unit)) = units::parse_length(&stripped) else {
        return (false, out);
    };
    let (w, h) = ffi::viewport();
    match unit {
        PERCENT => out.pct = num,
        EM | REM => font_unit_set(&mut out, font, unit, num),
        VW => out.px = num * w / 100.0,
        VH => out.px = num * h / 100.0,
        VMIN => out.px = num * (if w < h { w } else { h }) / 100.0,
        VMAX => out.px = num * (if w > h { w } else { h }) / 100.0,
        CQW | CQH | CQMIN | CQMAX => out.px = crate::container::unit_resolve(num, unit),
        _ => out.px = num,
    }
    (true, out)
}

fn resolve_px_pct(text: &[u8]) -> Option<(f64, f64)> {
    let (ok, out) = resolve_to_px_pct(text, false);
    ok.then_some((out.px, out.pct))
}

fn resolve_px_pct_or_zero(text: &[u8]) -> (bool, f64, f64) {
    let (ok, out) = resolve_to_px_pct(text, false);
    (ok, out.px, out.pct)
}

fn fn_scale(term: &mut Term, m: f64) {
    if term.func == 0 {
        return;
    }
    if !m.is_finite() {
        term.func = 0;
        return;
    }
    for arg in term.args.iter_mut().take(usize::from(term.n_args)) {
        arg.px *= m;
        arg.pct *= m;
    }
    if m >= 0.0 {
        return;
    }
    if term.func == FN_MIN || term.func == FN_MAX {
        term.func = if term.func == FN_MIN { FN_MAX } else { FN_MIN };
        return;
    }
    term.args.swap(0, 2);
    let none = term.arg_none;
    term.arg_none = (none & 2) | ((none & 1) << 2) | ((none & 4) >> 2);
}

fn fn_add(out: &mut Term, rhs: &Term, sign: f64) {
    if out.func == 0 && rhs.func == 0 {
        return;
    }
    if (out.func != 0 && rhs.func != 0)
        || out.em != 0.0
        || out.rem != 0.0
        || rhs.em != 0.0
        || rhs.rem != 0.0
    {
        out.func = 0;
        return;
    }
    if out.func != 0 {
        for arg in out.args.iter_mut().take(usize::from(out.n_args)) {
            arg.px += sign * rhs.px;
            arg.pct += sign * rhs.pct;
        }
        return;
    }
    let mut r = *rhs;
    fn_scale(&mut r, sign);
    if r.func == 0 {
        return;
    }
    for arg in r.args.iter_mut().take(usize::from(r.n_args)) {
        arg.px += out.px;
        arg.pct += out.pct;
    }
    out.func = r.func;
    out.n_args = r.n_args;
    out.arg_none = r.arg_none;
    out.args = r.args;
}

fn term_scale(term: &mut Term, m: f64) {
    fn_scale(term, m);
    if term.is_number {
        term.num *= m;
    } else if m.is_finite() {
        for field in [
            &mut term.px,
            &mut term.pct,
            &mut term.em,
            &mut term.rem,
            &mut term.vw,
            &mut term.vh,
            &mut term.vmin,
            &mut term.vmax,
        ] {
            *field *= m;
        }
    } else {
        for field in [&mut term.px, &mut term.pct, &mut term.em, &mut term.rem] {
            if *field != 0.0 {
                *field *= m;
            }
        }
        term.vw = 0.0;
        term.vh = 0.0;
        term.vmin = 0.0;
        term.vmax = 0.0;
    }
}

fn term_from_length(v: f64, unit: Unit) -> Term {
    let mut out = Term::default();
    match unit {
        NUMBER => {
            out.num = v;
            out.is_number = true;
        }
        PERCENT => out.pct = v,
        EM | IC => out.em = v,
        EX | CH => out.em = v * 0.5,
        CAP => out.em = v * 0.7,
        REM => out.rem = v,
        VW | VH | VMIN | VMAX => {
            out.px = units::viewport_resolve(v, unit);
            add_viewport_coeff(unit, v, &mut out);
        }
        CQW | CQH | CQMIN | CQMAX => out.px = crate::container::unit_resolve(v, unit),
        _ => out.px = v,
    }
    out
}

fn unit_value(unit: &[u8], num: f64) -> Option<Term> {
    if !num.is_finite() && unit.is_empty() {
        return Some(Term {
            num,
            is_number: true,
            ..Term::default()
        });
    }
    let mut text = ffi::dtostr(num);
    text.extend_from_slice(unit);
    let (v, u) = units::parse_length(&c_text(text))?;
    Some(term_from_length(v, u))
}

const FUNCTIONS: &[&[u8]] = &[
    b"calc",
    b"min",
    b"max",
    b"clamp",
    b"round",
    b"mod",
    b"rem",
    b"abs",
    b"hypot",
    b"pow",
    b"sqrt",
    b"atan2",
    b"atan",
    b"asin",
    b"acos",
    b"sign",
    b"sin",
    b"cos",
    b"tan",
    b"exp",
    b"log",
    b"progress",
];

const CONSTANTS: &[(&[u8], f64)] = &[
    (b"infinity", f64::INFINITY),
    (b"pi", PI),
    (b"e", E),
    (b"nan", f64::NAN),
];

fn primary(text: &CStr, pos: &mut usize, end: usize, depth: i32) -> Option<Term> {
    if depth > MAX_DEPTH {
        return None;
    }
    let s = text.to_bytes();
    let mut p = skip_ws(s, *pos, end);
    if p >= end {
        return None;
    }
    if end - p > 4 && starts_with_ci(&s[p..], b"env(") {
        let args = p + 4;
        let close = match_close_paren(s, args, end)?;
        let parts = split_args(s, args, close, 2);
        let mut out = Term::default();
        if parts.len() >= 2 {
            let (_, px, pct) = resolve_px_pct_or_zero(parts[1]);
            out.px = px;
            out.pct = pct;
        }
        *pos = close + 1;
        return Some(out);
    }
    if s[p] == b'(' {
        p += 1;
        let out = sum(text, &mut p, end, depth + 1)?;
        p = skip_ws(s, p, end);
        if p >= end || s[p] != b')' {
            return None;
        }
        *pos = p + 1;
        return Some(out);
    }
    for name in FUNCTIONS {
        let len = name.len();
        if end - p <= len + 1 || !starts_with_ci(&s[p..], name) || s[p + len] != b'(' {
            continue;
        }
        let close = match_close_paren(s, p + len + 1, end)?;
        let fragment = c_text(s[p..=close].to_vec());
        let out = match parse_calc(&fragment)? {
            Parsed::Calc(calc) => {
                let mut out = Term {
                    px: calc.px,
                    pct: calc.pct,
                    em: calc.em,
                    rem: calc.rem,
                    vw: calc.vw,
                    vh: calc.vh,
                    vmin: calc.vmin,
                    vmax: calc.vmax,
                    ..Term::default()
                };
                if (FN_MIN..=FN_CLAMP).contains(&calc.func) && (1..=4).contains(&calc.n_args) {
                    out.func = calc.func;
                    out.n_args = calc.n_args;
                    out.arg_none = calc.arg_none;
                    out.args = calc.args;
                    for arg in out.args.iter_mut().skip(usize::from(calc.n_args)) {
                        *arg = CalcArg::default();
                    }
                }
                out
            }
            Parsed::Length(v, unit) => term_from_length(v, unit),
        };
        *pos = close + 1;
        return Some(out);
    }
    for &(name, value) in CONSTANTS {
        let len = name.len();
        if end - p < len || !starts_with_ci(&s[p..], name) {
            continue;
        }
        let after = p + len;
        if after < end {
            let c = s[after];
            if c.is_ascii_alphanumeric() || c == b'.' || c == b'%' || c == b'(' {
                continue;
            }
        }
        *pos = after;
        return Some(Term {
            num: value,
            is_number: true,
            ..Term::default()
        });
    }
    let (num, number_end) = ffi::strtod(text, p);
    if number_end == p {
        return None;
    }
    let mut u = number_end;
    while u < end && (s[u].is_ascii_alphabetic() || s[u] == b'%') {
        u += 1;
    }
    let out = unit_value(&s[number_end..u], num)?;
    *pos = u;
    Some(out)
}

fn product(text: &CStr, pos: &mut usize, end: usize, depth: i32) -> Option<Term> {
    let s = text.to_bytes();
    let mut out = primary(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'*' && s[p] != b'/') {
            *pos = p;
            return Some(out);
        }
        let op = s[p];
        p += 1;
        let rhs = primary(text, &mut p, end, depth)?;
        if op == b'*' {
            if out.is_number && rhs.is_number {
                out.num *= rhs.num;
            } else if out.is_number {
                let m = out.num;
                out = rhs;
                term_scale(&mut out, m);
            } else if rhs.is_number {
                term_scale(&mut out, rhs.num);
            } else {
                return None;
            }
        } else {
            if !rhs.is_number {
                return None;
            }
            term_scale(&mut out, 1.0 / rhs.num);
        }
        *pos = p;
    }
}

fn sum(text: &CStr, pos: &mut usize, end: usize, depth: i32) -> Option<Term> {
    let s = text.to_bytes();
    let mut out = product(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'+' && s[p] != b'-') {
            *pos = p;
            return Some(out);
        }
        let op = s[p];
        p += 1;
        let rhs = product(text, &mut p, end, depth)?;
        if out.is_number != rhs.is_number {
            return None;
        }
        if out.is_number {
            if op == b'+' {
                out.num += rhs.num;
            } else {
                out.num -= rhs.num;
            }
            *pos = p;
            continue;
        }
        fn_add(&mut out, &rhs, if op == b'+' { 1.0 } else { -1.0 });
        let pairs = [
            (&mut out.px, rhs.px),
            (&mut out.pct, rhs.pct),
            (&mut out.em, rhs.em),
            (&mut out.rem, rhs.rem),
            (&mut out.vw, rhs.vw),
            (&mut out.vh, rhs.vh),
            (&mut out.vmin, rhs.vmin),
            (&mut out.vmax, rhs.vmax),
        ];
        for (field, value) in pairs {
            if op == b'+' {
                *field += value;
            } else {
                *field -= value;
            }
        }
        *pos = p;
    }
}

fn arg_key(text: &[u8]) -> Option<f64> {
    let (px, pct) = resolve_px_pct(text).or_else(|| {
        let wrapped = wrapped_in_calc(text);
        resolve_px_pct(wrapped.to_bytes())
    })?;
    let add = pct * 0.01 * viewport_w();
    Some(if add == 0.0 { px } else { px + add })
}

fn sign_of(n: f64) -> f64 {
    if n.is_nan() {
        f64::NAN
    } else if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        n
    }
}

const SIGN_UNITS: &[&[u8]] = &[
    b"s", b"ms", b"deg", b"grad", b"rad", b"turn", b"hz", b"khz", b"dpi", b"dpcm", b"dppx", b"x",
    b"fr",
];

fn token_sign(text: &[u8]) -> Option<f64> {
    let stripped = c_text(strip(text).to_vec());
    let s = stripped.to_bytes();
    let (num, end) = ffi::strtod(&stripped, 0);
    if end == 0 {
        return None;
    }
    let mut u = end;
    while u < s.len() && s[u].is_ascii_alphabetic() {
        u += 1;
    }
    let unit = &s[end..u];
    let rest = skip_ws(s, u, s.len());
    if rest != s.len() || unit.is_empty() {
        return None;
    }
    SIGN_UNITS
        .iter()
        .any(|known| unit.eq_ignore_ascii_case(known))
        .then(|| sign_of(num))
}

#[derive(Clone, Copy, PartialEq)]
enum Progress {
    Number,
    LengthPercentage,
    Angle,
}

const ANGLES: &[(&[u8], f64)] = &[
    (b"deg", 1.0),
    (b"grad", 0.9),
    (b"rad", 180.0 / PI),
    (b"turn", 360.0),
];

fn progress_operand(text: &[u8]) -> Option<(Progress, f64)> {
    if let Some(parsed) = parse_calc(&wrapped_in_calc(text)) {
        let vw = viewport_w();
        return Some(match parsed {
            Parsed::Length(v, NUMBER) => (Progress::Number, v),
            Parsed::Length(v, PERCENT) => (Progress::LengthPercentage, v * 0.01 * vw),
            Parsed::Length(v, _) => (Progress::LengthPercentage, v),
            Parsed::Calc(calc) => (
                Progress::LengthPercentage,
                calc.px + (calc.em + calc.rem) * 16.0 + calc.pct * 0.01 * vw,
            ),
        });
    }
    let stripped = c_text(strip(text).to_vec());
    let s = stripped.to_bytes();
    let (num, end) = ffi::strtod(&stripped, 0);
    if end == 0 {
        return None;
    }
    let mut u = end;
    while u < s.len() && s[u].is_ascii_alphabetic() {
        u += 1;
    }
    if u != s.len() || u == end {
        return None;
    }
    let unit = &s[end..u];
    ANGLES
        .iter()
        .find(|(name, _)| unit.eq_ignore_ascii_case(name))
        .map(|&(_, to_deg)| (Progress::Angle, num * to_deg))
}

fn arg_is_number(text: &[u8]) -> bool {
    let stripped = c_text(strip(text).to_vec());
    let (_, end) = ffi::strtod(&stripped, 0);
    if end != 0 && end == stripped.to_bytes().len() {
        return true;
    }
    let parsed =
        parse_calc(&stripped).or_else(|| parse_calc(&wrapped_in_calc(stripped.to_bytes())));
    matches!(parsed, Some(Parsed::Length(_, NUMBER)))
}

pub(crate) fn round_step(strategy: u8, a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() || b == 0.0 {
        return f64::NAN;
    }
    if a.is_infinite() {
        return if b.is_infinite() { f64::NAN } else { a };
    }
    if b.is_infinite() {
        return match strategy {
            1 if a > 0.0 => f64::INFINITY,
            2 if a < 0.0 => f64::NEG_INFINITY,
            _ => 0.0,
        };
    }
    let q = a / b.abs();
    let rounded = match strategy {
        1 => q.ceil(),
        2 => q.floor(),
        3 => q.trunc(),
        _ => q.round(),
    };
    rounded * b.abs()
}

pub(crate) fn mod_rem(is_mod: bool, a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() || b == 0.0 || a.is_infinite() {
        return f64::NAN;
    }
    if b.is_infinite() {
        if !is_mod {
            return a;
        }
        return if a.is_sign_negative() == b.is_sign_negative() {
            a
        } else {
            f64::NAN
        };
    }
    let q = a / b;
    if is_mod {
        a - b * q.floor()
    } else {
        a - b * q.trunc()
    }
}

pub(crate) fn stepped_eval(func: u8, k: &[f64; 4]) -> f64 {
    match func {
        FN_ABS => k[0].abs(),
        FN_MOD | FN_REM => mod_rem(func == FN_MOD, k[0], k[1]),
        _ => round_step(func.wrapping_sub(FN_ROUND), k[0], k[1]),
    }
}

pub(crate) fn math_fn_px(calc: &Calc, basis: f64) -> f64 {
    let n = usize::from(calc.n_args.min(4));
    let mut k = [0.0; 4];
    for (slot, arg) in k.iter_mut().zip(&calc.args).take(n) {
        *slot = arg.px + arg.pct * 0.01 * basis;
    }
    if calc.func >= FN_ROUND {
        return stepped_eval(calc.func, &k);
    }
    if calc.func == FN_CLAMP {
        let low = if calc.arg_none & 1 != 0 {
            f64::NEG_INFINITY
        } else {
            k[0]
        };
        let high = if calc.arg_none & 4 != 0 {
            f64::INFINITY
        } else {
            k[2]
        };
        let mut out = k[1];
        if out > high {
            out = high;
        }
        if out < low {
            out = low;
        }
        return out;
    }
    let mut out = k[0];
    for &key in k.iter().take(n).skip(1) {
        if calc.func == FN_MIN && key < out {
            out = key;
        }
        if calc.func == FN_MAX && key > out {
            out = key;
        }
    }
    out
}

#[derive(Clone, Copy)]
struct SteppedArg {
    px: f64,
    pct: f64,
    number: bool,
}

fn stepped_arg(text: &[u8]) -> Option<SteppedArg> {
    let (px, pct) = resolve_px_pct(text).or_else(|| {
        let wrapped = wrapped_in_calc(text);
        resolve_px_pct(wrapped.to_bytes())
    })?;
    Some(SteppedArg {
        px,
        pct,
        number: arg_is_number(text),
    })
}

fn stepped_result(func: u8, args: &[SteppedArg; 2], r: f64) -> Option<Parsed> {
    let n = if func == FN_ABS { 1 } else { 2 };
    let numbers = args.iter().take(n).filter(|arg| arg.number).count();
    let basis = args.iter().take(n).any(|arg| arg.pct != 0.0);
    if numbers == n {
        return Some(number(r));
    }
    if numbers > 0 {
        return None;
    }
    if !basis {
        return Some(pixels(r));
    }
    let mut calc = Calc {
        px: r,
        func,
        n_args: n as u8,
        ..Calc::default()
    };
    for (slot, arg) in calc.args.iter_mut().zip(args).take(n) {
        *slot = CalcArg {
            px: arg.px,
            pct: arg.pct,
        };
    }
    Some(Parsed::Calc(calc))
}

fn stepped_value(func: u8, parts: &[&[u8]]) -> Option<Parsed> {
    let mut args = [
        SteppedArg {
            px: 0.0,
            pct: 0.0,
            number: false,
        },
        SteppedArg {
            px: 1.0,
            pct: 0.0,
            number: true,
        },
    ];
    for (slot, part) in args.iter_mut().zip(parts) {
        *slot = stepped_arg(part)?;
    }
    let vw = viewport_w();
    let mut k = [0.0; 4];
    for (slot, arg) in k.iter_mut().zip(&args) {
        *slot = arg.px + arg.pct * 0.01 * vw;
    }
    stepped_result(func, &args, stepped_eval(func, &k))
}

fn stepped_function(fn_index: i32, parts: &[&[u8]]) -> Option<Parsed> {
    match fn_index {
        4 => {
            const STRATEGIES: [&[u8]; 4] = [b"nearest", b"up", b"down", b"to-zero"];
            let mut strategy = 0u8;
            let mut skip = 0;
            if let Some(first) = parts.first() {
                for (i, name) in STRATEGIES.iter().enumerate() {
                    if first.eq_ignore_ascii_case(name) {
                        strategy = i as u8;
                        skip = 1;
                    }
                }
            }
            let rest = &parts[skip..];
            if rest.is_empty() || rest.len() > 2 {
                return None;
            }
            stepped_value(FN_ROUND + strategy, rest)
        }
        7 => {
            if parts.len() == 1 {
                stepped_value(FN_ABS, parts)
            } else {
                None
            }
        }
        _ => {
            if parts.len() != 2 {
                return None;
            }
            stepped_value(if fn_index == 5 { FN_MOD } else { FN_REM }, parts)
        }
    }
}

fn numeric_function(fn_index: i32, parts: &[&[u8]]) -> Option<Parsed> {
    let n = parts.len();
    match fn_index {
        8 if n >= 1 => {
            let mut total = 0.0;
            for part in parts {
                let x = arg_key(part);
                let value = x.unwrap_or(0.0);
                total += value * value;
                x?;
            }
            Some(number(total.sqrt()))
        }
        9 if n == 2 => Some(number(arg_key(parts[0])?.powf(arg_key(parts[1])?))),
        10 if n == 1 => Some(number(arg_key(parts[0])?.sqrt())),
        11..=13 if n == 1 => {
            let radians = units::angle_expr_rewrite(&c_text(parts[0].to_vec()), true);
            let x = arg_key(&radians)?;
            Some(number(match fn_index {
                11 => x.sin(),
                12 => x.cos(),
                _ => x.tan(),
            }))
        }
        14 if n == 2 => {
            let y = arg_key(parts[0])?;
            let x = arg_key(parts[1])?;
            Some(number(y.atan2(x)))
        }
        15..=17 if n == 1 => {
            let x = arg_key(parts[0])?;
            Some(number(match fn_index {
                15 => x.atan(),
                16 => x.asin(),
                _ => x.acos(),
            }))
        }
        18 if n == 1 => match arg_key(parts[0]) {
            Some(x) => Some(number(sign_of(x))),
            None => token_sign(parts[0]).map(number),
        },
        19 if n == 1 => Some(number(arg_key(parts[0])?.exp())),
        20 if n >= 1 => {
            let x = arg_key(parts[0])?;
            if n >= 2 {
                arg_key(parts[1]).map(|base| number(x.ln() / base.ln()))
            } else {
                Some(number(x.ln()))
            }
        }
        21 if n == 3 => progress_function(parts),
        _ => None,
    }
}

fn progress_function(parts: &[&[u8]]) -> Option<Parsed> {
    let mut a = skip_ws(parts[0], 0, parts[0].len());
    let first = parts[0];
    let mut no_clamp = false;
    if starts_with_ci(&first[a..], b"no-clamp") && (a + 8 == first.len() || is_ws(first[a + 8])) {
        no_clamp = true;
        a = skip_ws(first, a + 8, first.len());
    }
    let ta = progress_operand(&first[a..]);
    let tb = progress_operand(parts[1]);
    let tc = progress_operand(parts[2]);
    let (Some((ta, va)), Some((tb, vb)), Some((tc, vc))) = (ta, tb, tc) else {
        return None;
    };
    if ta != tb || tb != tc {
        return None;
    }
    let den = vc - vb;
    let num = va - vb;
    let p = if den == 0.0 {
        if !no_clamp {
            0.0
        } else if num > 0.0 {
            f64::INFINITY
        } else if num < 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::NAN
        }
    } else {
        let p = num / den;
        if no_clamp {
            p
        } else if p.is_nan() || p < 0.0 {
            0.0
        } else if p > 1.0 {
            1.0
        } else {
            p
        }
    };
    Some(number(p))
}

fn comparison_function(func: u8, s: &[u8], args: usize, body_end: usize) -> Option<Parsed> {
    let mut values_px = [0.0f64; 8];
    let mut values_pct = [0.0f64; 8];
    let mut is_none = [false; 8];
    let mut num_count = 0;
    let mut none_count = 0;
    let mut ok = true;
    let mut n = 0usize;
    let mut seg = args;
    let mut depth = 0i32;
    for q in args..=body_end {
        if q < body_end && s[q] == b'(' {
            depth += 1;
        } else if q < body_end && s[q] == b')' {
            depth -= 1;
        }
        if q == body_end || (s[q] == b',' && depth == 0) {
            let slot = n.min(7);
            let part = strip(&s[seg..q]);
            if part.eq_ignore_ascii_case(b"none") {
                is_none[slot] = true;
                none_count += 1;
                if func != FN_CLAMP {
                    ok = false;
                }
            } else {
                let (resolved, px, pct) = resolve_px_pct_or_zero(part);
                values_px[slot] = px;
                values_pct[slot] = pct;
                if !resolved {
                    ok = false;
                } else if arg_is_number(part) {
                    num_count += 1;
                }
            }
            n += 1;
            seg = q + 1;
        }
    }
    if n == 0 || !ok {
        return None;
    }
    let non_none = n - none_count;
    if num_count != 0 && num_count != non_none {
        return None;
    }
    if func == FN_CLAMP && (n != 3 || is_none[1]) {
        return None;
    }
    let all_numbers = non_none > 0 && num_count == non_none;
    let n = n.min(8);
    let vw = viewport_w();
    let mut keys = [0.0f64; 8];
    for i in 0..n {
        keys[i] = values_px[i] + values_pct[i] * 0.01 * vw;
    }
    let out_px = if func == FN_CLAMP {
        let low = if is_none[0] {
            f64::NEG_INFINITY
        } else {
            keys[0]
        };
        let value = keys[1];
        let high = if is_none[2] { f64::INFINITY } else { keys[2] };
        if low.is_nan() || value.is_nan() || high.is_nan() {
            f64::NAN
        } else {
            let mut out = value;
            if out > high {
                out = high;
            }
            if out < low {
                out = low;
            }
            out
        }
    } else {
        let mut out = keys[0];
        let mut any_nan = keys[0].is_nan();
        for &key in keys.iter().take(n).skip(1) {
            if key.is_nan() {
                any_nan = true;
            }
            if func == FN_MIN && key < out {
                out = key;
            }
            if func == FN_MAX && key > out {
                out = key;
            }
        }
        if any_nan { f64::NAN } else { out }
    };
    if all_numbers {
        return Some(number(out_px));
    }
    let basis_dependent = n <= 4 && values_pct.iter().take(n).any(|&pct| pct != 0.0);
    if !basis_dependent {
        return Some(pixels(out_px));
    }
    let mut calc = Calc {
        px: out_px,
        func,
        n_args: n as u8,
        ..Calc::default()
    };
    for i in 0..n {
        calc.args[i] = CalcArg {
            px: values_px[i],
            pct: values_pct[i],
        };
        if is_none[i] {
            calc.arg_none |= 1 << i;
        }
    }
    Some(Parsed::Calc(calc))
}

const PARSERS: &[(&[u8], i32)] = &[
    (b"calc(", 0),
    (b"clamp(", 3),
    (b"min(", 1),
    (b"max(", 2),
    (b"round(", 4),
    (b"mod(", 5),
    (b"rem(", 6),
    (b"abs(", 7),
    (b"hypot(", 8),
    (b"pow(", 9),
    (b"sqrt(", 10),
    (b"sin(", 11),
    (b"cos(", 12),
    (b"tan(", 13),
    (b"atan2(", 14),
    (b"atan(", 15),
    (b"asin(", 16),
    (b"acos(", 17),
    (b"sign(", 18),
    (b"exp(", 19),
    (b"log(", 20),
    (b"progress(", 21),
];

fn parse_calc_inner(text: &CStr) -> Option<Parsed> {
    let s = text.to_bytes();
    let start = skip_ws(s, 0, s.len());
    let &(name, fn_index) = PARSERS
        .iter()
        .find(|(name, _)| starts_with_ci(&s[start..], name))?;
    let args = start + name.len();
    let body_end = match_close_paren(s, args, s.len())?;
    if s[body_end + 1..].iter().any(|&c| !is_ws(c)) {
        return None;
    }
    if fn_index >= 4 {
        let parts = split_args(s, args, body_end, 4);
        return if fn_index <= 7 {
            stepped_function(fn_index, &parts)
        } else {
            numeric_function(fn_index, &parts)
        };
    }
    if fn_index != 0 {
        return comparison_function(fn_index as u8, s, args, body_end);
    }
    let mut p = args;
    let term = sum(text, &mut p, body_end, 0)?;
    if skip_ws(s, p, body_end) != body_end {
        return None;
    }
    if term.is_number {
        return Some(number(term.num));
    }
    let mut calc = Calc::default();
    if term.func != 0 {
        calc.px = term.px + (term.em + term.rem) * 16.0 + term.pct * 0.01 * viewport_w();
        calc.func = term.func;
        calc.n_args = term.n_args;
        calc.arg_none = term.arg_none;
        let n = usize::from(term.n_args);
        calc.args[..n].copy_from_slice(&term.args[..n]);
        return Some(Parsed::Calc(calc));
    }
    let (w, h) = ffi::viewport();
    calc.pct = term.pct;
    calc.px = term.px;
    calc.em = term.em;
    calc.rem = term.rem;
    calc.vw = term.vw;
    calc.vh = term.vh;
    calc.vmin = term.vmin;
    calc.vmax = term.vmax;
    calc.parsed_vw = w;
    calc.parsed_vh = h;
    Some(Parsed::Calc(calc))
}

thread_local! {
    static DEPTH: Cell<i32> = const { Cell::new(0) };
}

pub(crate) fn parse_calc(text: &CStr) -> Option<Parsed> {
    let depth = DEPTH.with(Cell::get);
    if depth > MAX_DEPTH {
        return None;
    }
    DEPTH.with(|d| d.set(depth + 1));
    let parsed = parse_calc_inner(text);
    DEPTH.with(|d| d.set(d.get() - 1));
    parsed
}
