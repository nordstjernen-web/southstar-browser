//! Southstar — box-shadow and text-shadow: the shadow lists css.c stores, their canonical specified spelling and their computed text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::calc::{self, Parsed};
use crate::color::{color_text, parse_color};
use crate::ffi;
use crate::math;
use crate::scan::{is_ws, scan_until, split_ws_limit, trim_range};
use crate::text::{add_leading_zeros, normalize_negative_zero};
use crate::units::{self, CAP, CH, EM, EX, IC, NUMBER, PERCENT, REM};

pub(crate) const SHADOWS_MAX: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub spread: f64,
    pub em: [f64; 4],
    pub rem: [f64; 4],
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub inset: i32,
    pub currentcolor: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ShadowList {
    pub n: i32,
    pub is_text: i32,
    pub s: [Shadow; SHADOWS_MAX],
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(core::mem::size_of::<Shadow>() == 112 && core::mem::size_of::<ShadowList>() == 904);

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn gclamp(value: f64, low: f64, high: f64) -> f64 {
    if value > high {
        high
    } else if value < low {
        low
    } else {
        value
    }
}

fn text_is_ident(t: &[u8]) -> bool {
    !t.is_empty()
        && !t[0].is_ascii_digit()
        && t.iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn length_token(tok: &[u8], allow_negative: bool, out: &mut Vec<u8>) -> bool {
    let text = c_text(tok);
    if let Some((num, unit)) = units::parse_length(&text) {
        if unit == PERCENT || (unit == NUMBER && num != 0.0) || (!allow_negative && num < 0.0) {
            return false;
        }
        if num == 0.0 {
            out.extend_from_slice(b"0px");
            return true;
        }
        let mut canon = normalize_negative_zero(&add_leading_zeros(tok));
        let suffix = canon.len()
            - canon
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_alphabetic())
                .count();
        canon[suffix..].make_ascii_lowercase();
        out.extend_from_slice(&canon);
        return true;
    }
    let ok = match calc::parse_calc(&text) {
        Some(Parsed::Calc(c)) => c.pct == 0.0,
        Some(Parsed::Length(_, unit)) => unit != PERCENT && unit != NUMBER,
        None => false,
    };
    if !ok {
        return false;
    }
    let canon = math::math_canonical(&text).unwrap_or_else(|| add_leading_zeros(tok));
    out.extend_from_slice(&canon);
    true
}

fn specified_one(text: &[u8], is_text: bool, out: &mut Vec<u8>) -> bool {
    let tokens = split_ws_limit(text, 8);
    let mut color: Option<Vec<u8>> = None;
    let mut lengths = Vec::new();
    let mut n_lengths = 0;
    let mut lengths_closed = false;
    let mut inset = false;
    if tokens.is_empty() {
        return false;
    }
    for tok in &tokens {
        if tok.eq_ignore_ascii_case(b"inset") {
            if is_text || inset {
                return false;
            }
            inset = true;
            if n_lengths > 0 {
                lengths_closed = true;
            }
            continue;
        }
        let parsed = parse_color(&c_text(tok));
        if parsed.is_some() || tok.eq_ignore_ascii_case(b"currentcolor") {
            if color.is_some() {
                return false;
            }
            color = Some(match parsed {
                Some(rgba) if !text_is_ident(tok) => color_text(rgba),
                _ => tok.to_ascii_lowercase(),
            });
            if n_lengths > 0 {
                lengths_closed = true;
            }
            continue;
        }
        if lengths_closed || n_lengths >= if is_text { 3 } else { 4 } {
            return false;
        }
        if !lengths.is_empty() {
            lengths.push(b' ');
        }
        if !length_token(tok, n_lengths != 2, &mut lengths) {
            return false;
        }
        n_lengths += 1;
    }
    if n_lengths < 2 {
        return false;
    }
    if let Some(color) = color {
        out.extend_from_slice(&color);
        out.push(b' ');
    }
    out.extend_from_slice(&lengths);
    if inset {
        out.extend_from_slice(b" inset");
    }
    true
}

pub(crate) fn specified_canonical(value: &[u8], is_text: bool) -> Option<Vec<u8>> {
    let start = value.iter().position(|&c| !is_ws(c)).unwrap_or(value.len());
    let value = &value[start..];
    if value.is_empty() || value.windows(4).any(|w| w == b"var(") {
        return None;
    }
    if value.eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    let end = value.len();
    let mut out = Vec::new();
    let mut p = 0;
    while p < end {
        let (seg_end, term) = scan_until(value, p, end, b",");
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        if !specified_one(trim_range(value, p, seg_end), is_text, &mut out) {
            return None;
        }
        p = if term == b',' { seg_end + 1 } else { seg_end };
        if term == b',' && p >= end {
            return None;
        }
    }
    Some(out)
}

fn parse_one(text: &[u8]) -> Option<Shadow> {
    let mut inset = false;
    let mut color: Option<[u8; 4]> = None;
    let (mut lens, mut ems, mut rems) = ([0.0f64; 4], [0.0f64; 4], [0.0f64; 4]);
    let mut n_lens = 0;
    for tok in split_ws_limit(text, 8) {
        let text = c_text(tok);
        if let Some(rgba) = parse_color(&text) {
            color = Some(rgba);
            continue;
        }
        if n_lens < 4 {
            if let Some((mut num, unit)) = units::parse_length(&text) {
                match unit {
                    EM => {
                        ems[n_lens] = num;
                        num = 0.0;
                    }
                    REM => {
                        rems[n_lens] = num;
                        num = 0.0;
                    }
                    EX | CH => num *= 8.0,
                    CAP => num *= 11.2,
                    IC => num *= 16.0,
                    _ => {}
                }
                lens[n_lens] = num;
                n_lens += 1;
                continue;
            }
            if crate::scan::starts_with_ci(tok, b"calc(") {
                if let Some(Parsed::Calc(c)) = calc::parse_calc(&text) {
                    lens[n_lens] = c.px;
                    ems[n_lens] = c.em;
                    rems[n_lens] = c.rem;
                    n_lens += 1;
                }
                continue;
            }
        }
        if tok.eq_ignore_ascii_case(b"inset") {
            inset = true;
        }
    }
    if n_lens < 2 {
        return None;
    }
    let [r, g, b, a] = color.unwrap_or([0, 0, 0, 255]);
    Some(Shadow {
        x: lens[0],
        y: lens[1],
        blur: if n_lens >= 3 {
            gclamp(lens[2], 0.0, 1000.0)
        } else {
            0.0
        },
        spread: if n_lens >= 4 {
            gclamp(lens[3], -1000.0, 1000.0)
        } else {
            0.0
        },
        em: ems,
        rem: rems,
        r,
        g,
        b,
        a,
        inset: i32::from(inset),
        currentcolor: i32::from(color.is_none()),
    })
}

pub(crate) fn parse_list(text: &[u8]) -> Option<ShadowList> {
    let start = text.iter().position(|&c| !is_ws(c)).unwrap_or(text.len());
    let text = &text[start..];
    if text.is_empty() {
        return None;
    }
    let mut list = ShadowList::default();
    let mut depth = 0u32;
    let mut seg = 0;
    for q in 0..=text.len() {
        let c = text.get(q).copied().unwrap_or(0);
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth = depth.saturating_sub(1);
        }
        if (c == b',' && depth == 0) || q == text.len() {
            if (list.n as usize) < SHADOWS_MAX
                && let Some(shadow) = parse_one(&text[seg..q])
            {
                list.s[list.n as usize] = shadow;
                list.n += 1;
            }
            seg = q + 1;
        }
    }
    (list.n > 0).then_some(list)
}

pub(crate) fn serialize(list: &ShadowList) -> Vec<u8> {
    let mut out = Vec::new();
    let n = usize::try_from(list.n).unwrap_or(0).min(SHADOWS_MAX);
    for (i, sh) in list.s[..n].iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(&color_text([sh.r, sh.g, sh.b, sh.a]));
        let mut values = vec![sh.x, sh.y, sh.blur];
        if list.is_text == 0 {
            values.push(sh.spread);
        }
        for v in values {
            out.push(b' ');
            out.extend_from_slice(&ffi::format_double(c"%g", v));
            out.extend_from_slice(b"px");
        }
        if sh.inset != 0 {
            out.extend_from_slice(b" inset");
        }
    }
    out
}
