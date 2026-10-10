//! Southstar — the C string and number primitives grid placement parses its keywords with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) const ROWS_MAX: i32 = 4096;

pub(crate) fn fmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub(crate) fn fmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

pub(crate) fn skip_spaces(s: &[u8]) -> &[u8] {
    &s[s.iter().take_while(|&&c| c == b' ').count()..]
}

pub(crate) fn trim_spaces_end(s: &[u8]) -> &[u8] {
    let len = s.len() - s.iter().rev().take_while(|&&c| c == b' ').count();
    &s[..len]
}

pub(crate) fn strip(s: &[u8]) -> &[u8] {
    let start = s.iter().take_while(|&&c| is_space(c)).count();
    let s = &s[start..];
    let len = s.len() - s.iter().rev().take_while(|&&c| is_space(c)).count();
    &s[..len]
}

pub(crate) fn strtol(s: &[u8]) -> Option<(i64, &[u8])> {
    let mut i = s.iter().take_while(|&&c| is_space(c)).count();
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let digits = s[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let mut v: i64 = 0;
    for &c in &s[i..i + digits] {
        v = v.saturating_mul(10).saturating_add(i64::from(c - b'0'));
    }
    Some((if negative { -v } else { v }, &s[i + digits..]))
}

pub(crate) fn parse_span(s: &[u8]) -> i32 {
    let s = &s[s.iter().take_while(|&&c| c == b' ' || c == b'\t').count()..];
    match strtol(s) {
        Some((v, _)) => v.clamp(1, i64::from(ROWS_MAX)) as i32,
        None => 1,
    }
}

pub(crate) fn span_is_count(s: &[u8]) -> bool {
    match strtol(skip_spaces(s)) {
        Some((n, rest)) if n >= 1 => skip_spaces(rest).is_empty(),
        _ => false,
    }
}

pub(crate) fn span_body(s: &[u8]) -> Option<&[u8]> {
    s.strip_prefix(b"span ")
}
