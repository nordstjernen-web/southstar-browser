//! Southstar — CSS dimensions: the length units css.c knows, parsing a number with its unit, viewport units against the current viewport, and the unit and number spellings used when values serialize.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use std::ffi::CStr;

use crate::ffi;

pub(crate) type Unit = u32;

pub(crate) const PX: Unit = 0;
pub(crate) const EM: Unit = 1;
pub(crate) const REM: Unit = 2;
pub(crate) const PERCENT: Unit = 3;
pub(crate) const NUMBER: Unit = 4;
pub(crate) const VW: Unit = 5;
pub(crate) const VH: Unit = 6;
pub(crate) const VMIN: Unit = 7;
pub(crate) const VMAX: Unit = 8;
pub(crate) const CQW: Unit = 9;
pub(crate) const CQH: Unit = 10;
pub(crate) const CQMIN: Unit = 11;
pub(crate) const CQMAX: Unit = 12;
pub(crate) const EX: Unit = 13;
pub(crate) const CH: Unit = 14;
pub(crate) const CAP: Unit = 15;
pub(crate) const IC: Unit = 16;
pub(crate) const LH: Unit = 17;
pub(crate) const RLH: Unit = 18;
pub(crate) const REX: Unit = 19;
pub(crate) const RCH: Unit = 20;
pub(crate) const RCAP: Unit = 21;
pub(crate) const RIC: Unit = 22;

const SUFFIXES: &[(&[u8], Unit)] = &[
    (b"px", PX),
    (b"em", EM),
    (b"rem", REM),
    (b"%", PERCENT),
    (b"vw", VW),
    (b"vh", VH),
    (b"dvw", VW),
    (b"svw", VW),
    (b"lvw", VW),
    (b"dvh", VH),
    (b"svh", VH),
    (b"lvh", VH),
    (b"cqi", CQW),
    (b"cqw", CQW),
    (b"cqb", CQH),
    (b"cqh", CQH),
    (b"vi", VW),
    (b"dvi", VW),
    (b"svi", VW),
    (b"lvi", VW),
    (b"vb", VH),
    (b"dvb", VH),
    (b"svb", VH),
    (b"lvb", VH),
    (b"vmin", VMIN),
    (b"vmax", VMAX),
    (b"dvmin", VMIN),
    (b"svmin", VMIN),
    (b"lvmin", VMIN),
    (b"dvmax", VMAX),
    (b"svmax", VMAX),
    (b"lvmax", VMAX),
    (b"cqmin", CQMIN),
    (b"cqmax", CQMAX),
    (b"ex", EX),
    (b"ch", CH),
    (b"cap", CAP),
    (b"ic", IC),
    (b"lh", LH),
    (b"rlh", RLH),
    (b"rex", REX),
    (b"rch", RCH),
    (b"rcap", RCAP),
    (b"ric", RIC),
];

const ABSOLUTE: &[(&[u8], f64)] = &[
    (b"pt", 96.0 / 72.0),
    (b"pc", 16.0),
    (b"cm", 96.0 / 2.54),
    (b"mm", 96.0 / 25.4),
    (b"q", 96.0 / 101.6),
    (b"in", 96.0),
];

pub(crate) fn parse_length(text: &CStr) -> Option<(f64, Unit)> {
    let s = text.to_bytes();
    if s.is_empty() {
        return None;
    }
    let mut p = usize::from(s[0] == b'-' || s[0] == b'+');
    let digits = p;
    while p < s.len() && (s[p].is_ascii_digit() || s[p] == b'.') {
        p += 1;
    }
    if p == digits {
        return None;
    }
    let (v, end) = ffi::strtod(text, 0);
    if end == 0 {
        return None;
    }
    let rest = &s[end..];
    if rest.is_empty() {
        return Some((v, NUMBER));
    }
    if let Some(&(_, unit)) = SUFFIXES
        .iter()
        .find(|(name, _)| rest.eq_ignore_ascii_case(name))
    {
        return Some((v, unit));
    }
    ABSOLUTE
        .iter()
        .find(|(name, _)| rest.eq_ignore_ascii_case(name))
        .map(|&(_, factor)| (v * factor, PX))
}

pub(crate) fn viewport_resolve(v: f64, unit: Unit) -> f64 {
    let (w, h) = ffi::viewport();
    match unit {
        VW => v * w / 100.0,
        VH => v * h / 100.0,
        VMIN => v * (if w < h { w } else { h }) / 100.0,
        VMAX => v * (if w > h { w } else { h }) / 100.0,
        _ => 0.0,
    }
}

pub(crate) fn unit_suffix(unit: Unit) -> &'static CStr {
    match unit {
        PX => c"px",
        EM => c"em",
        REM => c"rem",
        PERCENT => c"%",
        NUMBER => c"",
        VW => c"vw",
        VH => c"vh",
        VMIN => c"vmin",
        VMAX => c"vmax",
        EX => c"ex",
        CH => c"ch",
        CAP => c"cap",
        IC => c"ic",
        LH => c"lh",
        RLH => c"rlh",
        REX => c"rex",
        RCH => c"rch",
        RCAP => c"rcap",
        RIC => c"ric",
        _ => c"px",
    }
}

pub(crate) fn number_text(n: f64) -> Vec<u8> {
    if n.is_nan() {
        b"NaN".to_vec()
    } else if n.is_infinite() {
        if n < 0.0 {
            b"-infinity".to_vec()
        } else {
            b"infinity".to_vec()
        }
    } else {
        ffi::format_double(c"%g", n)
    }
}

const RELATIVE_UNITS: &[&[u8]] = &[
    b"em", b"rem", b"ex", b"rex", b"ch", b"rch", b"cap", b"rcap", b"ic", b"ric", b"lh", b"rlh",
    b"vw", b"vh", b"vi", b"vb", b"vmin", b"vmax", b"svw", b"svh", b"svmin", b"svmax", b"lvw",
    b"lvh", b"lvmin", b"lvmax", b"dvw", b"dvh", b"dvmin", b"dvmax", b"cqw", b"cqh", b"cqi", b"cqb",
    b"cqmin", b"cqmax",
];

pub(crate) fn has_relative_unit(s: &[u8]) -> bool {
    let mut p = 0;
    while p < s.len() {
        if s[p].is_ascii_alphabetic() {
            let start = p;
            while p < s.len() && (s[p].is_ascii_alphabetic() || s[p] == b'-') {
                p += 1;
            }
            if p < s.len() && s[p] == b'(' {
                continue;
            }
            let word = &s[start..p];
            if RELATIVE_UNITS
                .iter()
                .any(|unit| word.eq_ignore_ascii_case(unit))
            {
                return true;
            }
        } else {
            p += 1;
        }
    }
    false
}

pub(crate) fn angle_expr_rewrite(text: &CStr, to_radians: bool) -> Vec<u8> {
    let s = text.to_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut p = 0;
    while p < s.len() {
        let number_start =
            s[p].is_ascii_digit() || (s[p] == b'.' && s.get(p + 1).is_some_and(u8::is_ascii_digit));
        if !number_start {
            out.push(s[p]);
            p += 1;
            continue;
        }
        let (v, end) = ffi::strtod(text, p);
        let mut u = end;
        while u < s.len() && s[u].is_ascii_alphabetic() {
            u += 1;
        }
        let unit = &s[end..u];
        let degrees = if unit.eq_ignore_ascii_case(b"deg") {
            Some(v)
        } else if unit.eq_ignore_ascii_case(b"grad") {
            Some(v * 0.9)
        } else if unit.eq_ignore_ascii_case(b"rad") {
            Some(v * 180.0 / PI)
        } else if unit.eq_ignore_ascii_case(b"turn") {
            Some(v * 360.0)
        } else {
            None
        };
        match degrees {
            Some(deg) => {
                let value = if to_radians { deg * PI / 180.0 } else { deg };
                out.extend_from_slice(&ffi::format_double(c"%.17g", value));
            }
            None => out.extend_from_slice(&s[p..u]),
        }
        p = u;
    }
    out
}
