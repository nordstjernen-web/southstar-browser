//! Southstar — the computed text of a grid track list: its lengths and math functions in em, rem and px resolved to px, percentages kept, and line names, keywords and flexible lengths left as written.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::calc::{self, Parsed};
use crate::scan::{byte, is_ident, match_close_paren};
use crate::transform::is_math_fn_start;
use crate::units::{self, EM, PERCENT, PX, REM, Unit};

fn absolute_px(unit: Unit, v: f64, font_px: f64, root_px: f64) -> Option<f64> {
    match unit {
        PX => Some(v),
        EM => Some(v * font_px),
        REM => Some(v * root_px),
        _ => None,
    }
}

struct Resolved {
    px: f64,
    pct: f64,
    has_pct: bool,
}

fn length(unit: Unit, v: f64, font_px: f64, root_px: f64) -> Option<Resolved> {
    if unit == PERCENT {
        return Some(Resolved {
            px: 0.0,
            pct: v,
            has_pct: true,
        });
    }
    absolute_px(unit, v, font_px, root_px).map(|px| Resolved {
        px,
        pct: 0.0,
        has_pct: false,
    })
}

fn resolve(text: &[u8], math: bool, font_px: f64, root_px: f64) -> Option<Resolved> {
    let ctext = CString::new(text).ok()?;
    if !math {
        let (v, unit) = units::parse_length(&ctext)?;
        return length(unit, v, font_px, root_px);
    }
    match calc::parse_calc(&ctext)? {
        Parsed::Length(v, unit) => length(unit, v, font_px, root_px),
        Parsed::Calc(c)
            if c.func == 0 && c.vw == 0.0 && c.vh == 0.0 && c.vmin == 0.0 && c.vmax == 0.0 =>
        {
            Some(Resolved {
                px: c.px + c.em * font_px + c.rem * root_px,
                pct: c.pct,
                has_pct: text.contains(&b'%'),
            })
        }
        Parsed::Calc(_) => None,
    }
}

fn append_computed(out: &mut Vec<u8>, text: &[u8], font_px: f64, root_px: f64) -> bool {
    let math = is_math_fn_start(text);
    let Some(r) = resolve(text, math, font_px, root_px) else {
        return false;
    };
    let pct = units::number_text(r.pct);
    let px = units::number_text(if r.has_pct {
        r.px.abs()
    } else if r.px > 0.0 {
        r.px
    } else {
        0.0
    });
    if !r.has_pct {
        out.extend_from_slice(&px);
        out.extend_from_slice(b"px");
    } else if !math {
        out.extend_from_slice(&pct);
        out.push(b'%');
    } else {
        out.extend_from_slice(b"calc(");
        out.extend_from_slice(&pct);
        out.extend_from_slice(if r.px < 0.0 { b"% - " } else { b"% + " });
        out.extend_from_slice(&px);
        out.extend_from_slice(b"px)");
    }
    true
}

fn number_start(s: &[u8], mut p: usize) -> bool {
    if matches!(byte(s, p), b'+' | b'-') {
        p += 1;
    }
    if byte(s, p) == b'.' {
        p += 1;
    }
    byte(s, p).is_ascii_digit()
}

fn number_token(s: &[u8], p: usize) -> (usize, usize) {
    let end = s.len();
    let mut q = p + 1;
    while q < end && (s[q].is_ascii_digit() || s[q] == b'.') {
        q += 1;
    }
    if q < end && (s[q] == b'e' || s[q] == b'E') && number_start(s, q + 1) {
        q += 2;
        while q < end && s[q].is_ascii_digit() {
            q += 1;
        }
    }
    let unit = q;
    while q < end && (s[q].is_ascii_alphabetic() || s[q] == b'%') {
        q += 1;
    }
    (unit, q)
}

pub(crate) fn tracks_computed_text(text: &[u8], font_px: f64, root_px: f64) -> Vec<u8> {
    let end = text.len();
    let mut out = Vec::with_capacity(end);
    let mut p = 0;
    while p < end {
        let boundary = p == 0 || !(is_ident(text[p - 1]) || text[p - 1] == b'.');
        if text[p] == b'[' {
            let stop = text[p..]
                .iter()
                .position(|&c| c == b']')
                .map_or(end, |i| p + i + 1);
            out.extend_from_slice(&text[p..stop]);
            p = stop;
            continue;
        }
        if boundary && is_math_fn_start(&text[p..]) {
            let open = p + text[p..].iter().position(|&c| c == b'(').unwrap_or(0) + 1;
            let stop = match_close_paren(text, open, end).map_or(end, |close| close + 1);
            if !append_computed(&mut out, &text[p..stop], font_px, root_px) {
                out.extend_from_slice(&text[p..stop]);
            }
            p = stop;
            continue;
        }
        if boundary && number_start(text, p) {
            let (unit, q) = number_token(text, p);
            let token = &text[p..q];
            let flex = text[unit..q].eq_ignore_ascii_case(b"fr");
            if flex || !append_computed(&mut out, token, font_px, root_px) {
                out.extend_from_slice(token);
            }
            p = q;
            continue;
        }
        out.push(text[p]);
        p += 1;
    }
    out
}
