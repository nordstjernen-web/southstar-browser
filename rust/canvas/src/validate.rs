//! Southstar — the syntax checks the canvas 2D attributes apply to filter and length strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::color::skip_spaces;
use crate::ffi;

const FILTER_NAMES: [&str; 11] = [
    "blur",
    "brightness",
    "contrast",
    "drop-shadow",
    "grayscale",
    "hue-rotate",
    "invert",
    "opacity",
    "saturate",
    "sepia",
    "url",
];

const LENGTH_UNITS: [&str; 15] = [
    "px", "em", "rem", "ex", "ch", "pt", "pc", "cm", "mm", "in", "q", "vw", "vh", "vmin", "vmax",
];

fn filter_name_known(name: &[u8]) -> bool {
    FILTER_NAMES
        .iter()
        .any(|known| known.as_bytes().eq_ignore_ascii_case(name))
}

fn skip_arguments(p: &[u8]) -> Option<&[u8]> {
    let mut depth = 0;
    for (i, &c) in p.iter().enumerate() {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(&p[i + 1..]);
            }
        }
    }
    None
}

pub(crate) fn filter_valid(s: &[u8]) -> bool {
    let mut p = skip_spaces(s);
    if p.is_empty() {
        return false;
    }
    if p.len() >= 4 && p[..4].eq_ignore_ascii_case(b"none") {
        return skip_spaces(&p[4..]).is_empty();
    }
    while !p.is_empty() {
        let name_len = p
            .iter()
            .position(|&c| !c.is_ascii_alphabetic() && c != b'-')
            .unwrap_or(p.len());
        if !filter_name_known(&p[..name_len]) || p.get(name_len) != Some(&b'(') {
            return false;
        }
        let Some(rest) = skip_arguments(&p[name_len..]) else {
            return false;
        };
        p = skip_spaces(rest);
    }
    true
}

pub(crate) fn length_valid(s: &[u8]) -> bool {
    let (value, used) = ffi::strtod_prefix(s);
    let starts_number = s
        .first()
        .is_some_and(|&c| matches!(c, b'+' | b'-' | b'.') || c.is_ascii_digit());
    if used == 0 || !starts_number {
        return false;
    }
    let unit = &s[used..];
    if unit.is_empty() {
        return value == 0.0;
    }
    LENGTH_UNITS
        .iter()
        .any(|known| known.as_bytes().eq_ignore_ascii_case(unit))
}
