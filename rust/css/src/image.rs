//! Southstar — CSS image values: the canonical form of a list of image layers, image-set() with its resolutions and types, unicode-range lists, the url() helpers css.c's image references use, and picking the image-set() candidate for one device pixel.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::calc::{self, MAX_DEPTH};
use crate::content;
use crate::ffi;
use crate::gradient::{self, token_is_math_fn};
use crate::scan::{
    is_ws, match_paren_quoted, scan_until, skip_ws, starts_with_ci, strip, trim_range,
};
use crate::units;

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

pub(crate) fn text_starts_image_set(text: &[u8]) -> bool {
    let rest = &text[skip_ws(text, 0, text.len())..];
    starts_with_ci(rest, b"image-set(") || starts_with_ci(rest, b"-webkit-image-set(")
}

pub(crate) fn image_value_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let end = text.len();
    let mut out = Vec::new();
    let mut first = true;
    let mut p = 0;
    while p < end {
        let (seg_end, term) = scan_until(text, p, end, b",");
        let layer = trim_range(text, p, seg_end);
        let canon = if gradient::text_starts_gradient(layer) {
            let (specified, gend) = gradient::specified_with_end(layer)?;
            if skip_ws(layer, gend, layer.len()) < layer.len() {
                return None;
            }
            Some(specified)
        } else if text_starts_image_set(layer) {
            Some(image_set_canonical(layer, false)?)
        } else {
            None
        };
        if !first {
            out.extend_from_slice(b", ");
        }
        first = false;
        out.extend_from_slice(canon.as_deref().unwrap_or(layer));
        p = if term == b',' { seg_end + 1 } else { seg_end };
    }
    Some(out)
}

fn split_ws(text: &[u8], max: usize) -> Option<Vec<&[u8]>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < text.len() && out.len() < max {
        p = skip_ws(text, p, text.len());
        if p >= text.len() {
            break;
        }
        let start = p;
        let mut depth = 0u32;
        while p < text.len() {
            let c = text[p];
            if c == b'"' || c == b'\'' {
                p = content::scan_string_end(text, p)? + 1;
                continue;
            }
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                depth = depth.saturating_sub(1);
            } else if is_ws(c) && depth == 0 {
                break;
            }
            p += 1;
        }
        out.push(&text[start..p]);
    }
    Some(out)
}

#[derive(Clone, Copy, Default)]
struct ResTerm {
    value: f64,
    resolution: bool,
    known: bool,
}

fn resolution_factor(unit: &[u8]) -> Option<f64> {
    if unit.eq_ignore_ascii_case(b"x") || unit.eq_ignore_ascii_case(b"dppx") {
        Some(1.0)
    } else if unit.eq_ignore_ascii_case(b"dpi") {
        Some(1.0 / 96.0)
    } else if unit.eq_ignore_ascii_case(b"dpcm") {
        Some(2.54 / 96.0)
    } else {
        None
    }
}

const RESOLUTION_FUNCTIONS: &[&[u8]] = &[
    b"calc",
    b"sign",
    b"sibling-index",
    b"sibling-count",
    b"min",
    b"max",
    b"clamp",
    b"abs",
];

fn res_atom(text: &CString, pos: &mut usize, end: usize, depth: i32) -> Option<ResTerm> {
    if depth > MAX_DEPTH {
        return None;
    }
    let s = text.to_bytes();
    let mut p = skip_ws(s, *pos, end);
    if p >= end {
        return None;
    }
    if s[p] == b'(' {
        p += 1;
        let out = res_sum(text, &mut p, end, depth + 1)?;
        p = skip_ws(s, p, end);
        if p >= end || s[p] != b')' {
            return None;
        }
        *pos = p + 1;
        return Some(out);
    }
    if s[p].is_ascii_alphabetic() || s[p] == b'-' {
        let start = p;
        while p < end && (s[p].is_ascii_alphanumeric() || s[p] == b'-') {
            p += 1;
        }
        if p >= end || s[p] != b'(' {
            return None;
        }
        let close = match_paren_quoted(s, p, end)?;
        let name = &s[start..p];
        let known = RESOLUTION_FUNCTIONS
            .iter()
            .any(|f| name.eq_ignore_ascii_case(f));
        let out = if name.eq_ignore_ascii_case(b"calc") {
            let mut inner = p + 1;
            res_sum(text, &mut inner, close, depth + 1)?
        } else if name.eq_ignore_ascii_case(b"sign") {
            let arg = &s[p + 1..close];
            let relative = units::has_relative_unit(arg);
            let (parsed, resolved) = if relative {
                (false, calc::Resolved::default())
            } else {
                calc::resolve_to_px_pct(arg, false)
            };
            ResTerm {
                resolution: false,
                known: parsed && resolved.pct == 0.0,
                value: if resolved.px > 0.0 {
                    1.0
                } else if resolved.px < 0.0 {
                    -1.0
                } else {
                    0.0
                },
            }
        } else if known {
            ResTerm::default()
        } else {
            return None;
        };
        *pos = close + 1;
        return Some(out);
    }
    let (v, number_end) = ffi::strtod(text, p);
    if number_end == p {
        return None;
    }
    p = number_end;
    let unit_start = p;
    while p < end && s[p].is_ascii_alphabetic() {
        p += 1;
    }
    let mut out = ResTerm {
        value: v,
        known: true,
        resolution: false,
    };
    if p > unit_start {
        let factor = resolution_factor(&s[unit_start..p])?;
        out.value = v * factor;
        out.resolution = true;
    }
    *pos = p;
    Some(out)
}

fn res_product(text: &CString, pos: &mut usize, end: usize, depth: i32) -> Option<ResTerm> {
    let s = text.to_bytes();
    let mut out = res_atom(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'*' && s[p] != b'/') {
            return Some(out);
        }
        let op = s[p];
        p += 1;
        let rhs = res_atom(text, &mut p, end, depth)?;
        if op == b'*' {
            if out.resolution && rhs.resolution {
                return None;
            }
            out.resolution = out.resolution || rhs.resolution;
            out.value *= rhs.value;
        } else {
            if rhs.resolution || (rhs.known && rhs.value == 0.0) {
                return None;
            }
            out.value = if rhs.value != 0.0 {
                out.value / rhs.value
            } else {
                0.0
            };
        }
        out.known = out.known && rhs.known;
        *pos = p;
    }
}

fn res_sum(text: &CString, pos: &mut usize, end: usize, depth: i32) -> Option<ResTerm> {
    let s = text.to_bytes();
    let mut out = res_product(text, pos, end, depth)?;
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'+' && s[p] != b'-') {
            return Some(out);
        }
        let op = s[p];
        if !(p + 1 < end && is_ws(s[p + 1])) {
            return None;
        }
        p += 1;
        let rhs = res_product(text, &mut p, end, depth)?;
        if out.resolution != rhs.resolution {
            return None;
        }
        out.value = if op == b'+' {
            out.value + rhs.value
        } else {
            out.value - rhs.value
        };
        out.known = out.known && rhs.known;
        *pos = p;
    }
}

fn number_with(v: f64, suffix: &[u8]) -> Vec<u8> {
    let mut out = ffi::format_double(c"%g", v);
    out.extend_from_slice(suffix);
    out
}

fn resolution_canonical(tok: &[u8], computed: bool) -> Option<Vec<u8>> {
    let text = c_text(tok);
    let s = text.to_bytes();
    if token_is_math_fn(s) {
        let mut p = 0;
        let t = res_atom(&text, &mut p, s.len(), 0)?;
        if !t.resolution || skip_ws(s, p, s.len()) < s.len() {
            return None;
        }
        if !t.known {
            return Some(s.to_vec());
        }
        return Some(if computed {
            number_with(t.value, b"dppx")
        } else {
            let mut out = b"calc(".to_vec();
            out.extend_from_slice(&number_with(t.value, b"dppx)"));
            out
        });
    }
    let (v, end) = ffi::strtod(&text, 0);
    if end == 0 || end == s.len() {
        return None;
    }
    let unit = &s[end..];
    let factor = resolution_factor(unit)?;
    if v < 0.0 {
        return None;
    }
    Some(if computed {
        number_with(v * factor, b"dppx")
    } else {
        number_with(v, &unit.to_ascii_lowercase())
    })
}

pub(crate) fn image_set_canonical(text: &[u8], computed: bool) -> Option<Vec<u8>> {
    let mut p = skip_ws(text, 0, text.len());
    if starts_with_ci(&text[p..], b"-webkit-image-set(") {
        p += 18;
    } else if starts_with_ci(&text[p..], b"image-set(") {
        p += 10;
    } else {
        return None;
    }
    let close = match_paren_quoted(text, p - 1, text.len())?;
    if skip_ws(text, close + 1, text.len()) < text.len() {
        return None;
    }
    let options = content::split_args(&text[p..close])?;
    if options.is_empty() {
        return None;
    }
    let mut out = b"image-set(".to_vec();
    for (i, option) in options.iter().enumerate() {
        let tok = split_ws(option, 6)?;
        if tok.is_empty() {
            return None;
        }
        let mut image = None;
        let mut resolution: Option<Vec<u8>> = None;
        let mut kind: Option<&[u8]> = None;
        for (k, t) in tok.iter().enumerate() {
            if k == 0 {
                image = Some(if content::is_string_token(t) {
                    let mut url = b"url(".to_vec();
                    url.extend_from_slice(&content::string_canonical(&t[1..t.len() - 1]));
                    url.push(b')');
                    url
                } else if [&b"url("[..], b"src(", b"image(", b"cross-fade("]
                    .iter()
                    .any(|name| starts_with_ci(t, name))
                {
                    t.to_vec()
                } else if gradient::text_starts_gradient(t) {
                    gradient::specified_with_end(t)?.0
                } else {
                    return None;
                });
            } else if starts_with_ci(t, b"type(") {
                if kind.is_some() {
                    return None;
                }
                kind = Some(t);
            } else {
                if resolution.is_some() {
                    return None;
                }
                resolution = Some(resolution_canonical(t, computed)?);
            }
        }
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(image.as_deref().unwrap_or_default());
        out.push(b' ');
        out.extend_from_slice(resolution.as_deref().unwrap_or(if computed {
            b"1dppx"
        } else {
            b"1x"
        }));
        if let Some(kind) = kind {
            out.push(b' ');
            out.extend_from_slice(kind);
        }
    }
    out.push(b')');
    Some(out)
}

fn hex_value(c: u8) -> Option<u32> {
    char::from(c).to_digit(16)
}

pub(crate) fn unicode_range_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let mut clean = Vec::with_capacity(text.len());
    let mut p = 0;
    while p < text.len() {
        if text[p] == b'/' && text.get(p + 1) == Some(&b'*') {
            let close = text[p + 2..].windows(2).position(|w| w == b"*/")?;
            p = p + 2 + close + 2;
            continue;
        }
        clean.push(text[p]);
        p += 1;
    }
    let mut out = Vec::new();
    for part in clean.split(|&c| c == b',') {
        let r = strip(part);
        if r.len() < 2 || (r[0] != b'u' && r[0] != b'U') || r[1] != b'+' {
            return None;
        }
        let mut q = 2;
        let (mut start, mut end) = (0u32, 0u32);
        let (mut hex, mut wild) = (0, 0);
        while q < r.len() && hex < 7 {
            let Some(digit) = hex_value(r[q]) else {
                break;
            };
            start = start * 16 + digit;
            hex += 1;
            q += 1;
        }
        while q < r.len() && r[q] == b'?' && wild < 7 {
            wild += 1;
            q += 1;
        }
        if hex + wild == 0 || hex + wild > 6 {
            return None;
        }
        if wild > 0 {
            if q < r.len() {
                return None;
            }
            end = start;
            for _ in 0..wild {
                start *= 16;
                end = end * 16 + 15;
            }
        } else if q < r.len() && r[q] == b'-' {
            q += 1;
            let mut hex2 = 0;
            while q < r.len() && hex2 < 7 {
                let Some(digit) = hex_value(r[q]) else {
                    break;
                };
                end = end * 16 + digit;
                hex2 += 1;
                q += 1;
            }
            if hex2 == 0 || hex2 > 6 || q < r.len() {
                return None;
            }
        } else if q < r.len() {
            return None;
        } else {
            end = start;
        }
        if start > end || end > 0x10FFFF {
            return None;
        }
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        if start == end {
            out.extend_from_slice(format!("U+{start:X}").as_bytes());
        } else {
            out.extend_from_slice(format!("U+{start:X}-{end:X}").as_bytes());
        }
    }
    (!out.is_empty()).then_some(out)
}

pub(crate) fn quoted_end(u: &[u8], quote: u8) -> Option<usize> {
    let mut p = 0;
    while p < u.len() {
        if u[p] == b'\\' && p + 1 < u.len() {
            p += 2;
            continue;
        }
        if u[p] == quote {
            return Some(p);
        }
        p += 1;
    }
    None
}

pub(crate) fn unescape_url(u: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(u.len());
    let mut i = 0;
    while i < u.len() {
        if u[i] == b'\\' && i + 1 < u.len() {
            i += 1;
        }
        out.push(u[i]);
        i += 1;
    }
    out
}

pub(crate) fn pick_image_set_url(t: &[u8]) -> Option<Vec<u8>> {
    let at = |i: usize| t.get(i).copied().unwrap_or(0);
    let mut p = skip_ws(t, 0, t.len());
    if starts_with_ci(&t[p..], b"-webkit-image-set(") {
        p += 18;
    } else if starts_with_ci(&t[p..], b"image-set(") {
        p += 10;
    } else {
        return None;
    }
    let target = 1.0;
    let mut best: Option<Vec<u8>> = None;
    let mut best_res = 0.0f64;
    while at(p) != 0 && at(p) != b')' {
        while at(p) != 0 && (is_ws(at(p)) || at(p) == b',') {
            p += 1;
        }
        if at(p) == 0 || at(p) == b')' {
            break;
        }
        let mut url = None;
        if starts_with_ci(&t[p..], b"url(") {
            let mut u = skip_ws(t, p + 4, t.len());
            let mut quote = 0u8;
            if at(u) == b'"' || at(u) == b'\'' {
                quote = at(u);
                u += 1;
            }
            let end = if quote != 0 {
                quoted_end(&t[u..], quote).map(|e| u + e)
            } else {
                let mut e = u;
                while at(e) != 0 && at(e) != b')' && !is_ws(at(e)) {
                    e += 1;
                }
                Some(e)
            };
            if let Some(end) = end {
                if end > u {
                    url = Some(unescape_url(&t[u..end]));
                }
            }
            p = end.unwrap_or(p + 4);
            while at(p) != 0 && at(p) != b')' {
                p += 1;
            }
            if at(p) == b')' {
                p += 1;
            }
        }
        let mut res = 1.0;
        p = skip_ws(t, p, t.len());
        if at(p) != 0 && at(p) != b',' && at(p) != b')' {
            res = ffi::strtod(&c_text(&t[p..]), 0).0;
            while at(p) != 0 && at(p) != b',' && at(p) != b')' {
                p += 1;
            }
        }
        match url {
            Some(url) => {
                if best.is_none() || (res - target).abs() < (best_res - target).abs() {
                    best = Some(url);
                    best_res = res;
                }
            }
            None => {
                while at(p) != 0 && at(p) != b',' && at(p) != b')' {
                    p += 1;
                }
            }
        }
    }
    best
}
