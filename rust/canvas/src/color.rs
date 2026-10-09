//! Southstar — canvas colours: parsing CSS and color() values to sRGB and serializing them as a 2D context reports them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi;

const FUNCTION_NAMES: [&str; 5] = ["color", "lab", "lch", "oklab", "oklch"];

pub(crate) fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

pub(crate) fn skip_spaces(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    &s[start..]
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn srgb_encode(v: f64) -> f64 {
    if v <= 0.0 {
        return 0.0;
    }
    if v >= 1.0 {
        return 1.0;
    }
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn component(token: &[u8]) -> Option<f64> {
    if token == b"none" {
        return Some(0.0);
    }
    let (v, used) = ffi::strtod_prefix(token);
    if used == 0 {
        return None;
    }
    match &token[used..] {
        b"%" => Some(v / 100.0),
        b"" => Some(v),
        _ => None,
    }
}

fn function_parts(body: &[u8]) -> Vec<Vec<u8>> {
    let mut spaced = Vec::with_capacity(body.len());
    for &c in body {
        if c == b'/' {
            spaced.extend_from_slice(b" / ");
        } else if is_space(c) {
            spaced.push(b' ');
        } else {
            spaced.push(c.to_ascii_lowercase());
        }
    }
    spaced
        .split(|&c| c == b' ')
        .filter(|part| !part.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

fn display_p3_to_srgb(c: &mut [f64; 3]) {
    let (lr, lg, lb) = (srgb_decode(c[0]), srgb_decode(c[1]), srgb_decode(c[2]));
    c[0] = srgb_encode(1.2249401 * lr - 0.2249404 * lg);
    c[1] = srgb_encode(-0.0420569 * lr + 1.0420571 * lg);
    c[2] = srgb_encode(-0.0196376 * lr - 0.0786361 * lg + 1.0982735 * lb);
}

fn parse_color_function(s: &[u8]) -> Option<[f64; 4]> {
    let s = skip_spaces(s);
    let close = s.iter().rposition(|&c| c == b')')?;
    if s.len() < 6 || !s[..6].eq_ignore_ascii_case(b"color(") || close < 6 {
        return None;
    }
    let parts = function_parts(&s[6..close]);
    if parts.len() < 4 {
        return None;
    }
    let p3 = parts[0] == b"display-p3";
    if !p3 && parts[0] != b"srgb" {
        return None;
    }
    if parts.len() != 4 && !(parts.len() == 6 && parts[4] == b"/") {
        return None;
    }
    let mut c = [0.0; 3];
    for (i, value) in c.iter_mut().enumerate() {
        *value = component(&parts[i + 1])?;
    }
    let alpha = if parts.len() == 6 {
        component(&parts[5])?
    } else {
        1.0
    };
    if p3 {
        display_p3_to_srgb(&mut c);
    }
    Some([clamp01(c[0]), clamp01(c[1]), clamp01(c[2]), clamp01(alpha)])
}

pub(crate) fn parse(s: &[u8]) -> Option<[f64; 4]> {
    match ffi::css_parse_color(s) {
        Some([r, g, b, a]) => Some([
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            f64::from(a) / 255.0,
        ]),
        None => parse_color_function(s),
    }
}

fn serialize(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    if a == 255 {
        return format!("#{r:02x}{g:02x}{b:02x}").into_bytes();
    }
    let mut alpha = (f64::from(a) / 255.0 * 100.0).round() / 100.0;
    if (alpha * 255.0).round() as i32 != i32::from(a) {
        alpha = (f64::from(a) / 255.0 * 1000.0).round() / 1000.0;
    }
    let mut out = format!("rgba({r}, {g}, {b}, ").into_bytes();
    out.extend_from_slice(&ffi::format_g(alpha));
    out.push(b')');
    out
}

fn function_index(name: &[u8]) -> Option<usize> {
    FUNCTION_NAMES
        .iter()
        .position(|known| known.as_bytes().eq_ignore_ascii_case(name))
}

fn function_tokens(body: &[u8]) -> Vec<Vec<u8>> {
    let mut spaced = Vec::with_capacity(body.len());
    for &c in body {
        match c {
            b'/' => spaced.extend_from_slice(b" / "),
            b'\t' | b'\n' | b'\r' => spaced.push(b' '),
            _ => spaced.push(c),
        }
    }
    spaced.split(|&c| c == b' ').map(<[u8]>::to_vec).collect()
}

fn percent_scaled(token: &[u8], scale: f64) -> f64 {
    ffi::strtod(token) * scale
}

fn function_string(css: &[u8]) -> Option<Vec<u8>> {
    let p = skip_spaces(css);
    let open = p.iter().position(|&c| c == b'(')?;
    let close = p.iter().rposition(|&c| c == b')')?;
    if close < open {
        return None;
    }
    let function = function_index(&p[..open])?;
    let mut out = FUNCTION_NAMES[function].as_bytes().to_vec();
    out.push(b'(');
    let mut alpha = None;
    let (mut slash, mut first) = (false, true);
    for token in function_tokens(&p[open + 1..close]) {
        if token.is_empty() {
            continue;
        }
        if token == b"/" {
            slash = true;
            continue;
        }
        if slash {
            alpha = Some(token);
            break;
        }
        if !first {
            out.push(b' ');
        }
        if function >= 1 && first && token.ends_with(b"%") {
            let scale = if function >= 3 { 0.01 } else { 1.0 };
            out.extend_from_slice(&ffi::format_g(percent_scaled(&token, scale)));
        } else {
            out.extend_from_slice(&token);
        }
        first = false;
    }
    if let Some(alpha) = alpha {
        let scale = if alpha.ends_with(b"%") { 0.01 } else { 1.0 };
        let v = percent_scaled(&alpha, scale);
        if v < 1.0 {
            out.extend_from_slice(b" / ");
            out.extend_from_slice(&ffi::format_g(if v < 0.0 { 0.0 } else { v }));
        }
    }
    out.push(b')');
    Some(out)
}

pub(crate) fn to_string(css: &[u8]) -> Option<Vec<u8>> {
    let [r, g, b, a] = parse(css)?;
    if let Some(function) = function_string(css) {
        return Some(function);
    }
    let byte = |v: f64| (v * 255.0).round() as u8;
    Some(serialize(byte(r), byte(g), byte(b), byte(a)))
}
