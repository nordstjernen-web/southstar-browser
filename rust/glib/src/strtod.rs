//! Southstar — a locale-independent strtod over byte strings, matching g_ascii_strtod.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn digits_end(text: &[u8], mut at: usize, accept: fn(&u8) -> bool) -> usize {
    while text.get(at).is_some_and(accept) {
        at += 1;
    }
    at
}

fn starts_with_ignore_case(text: &[u8], at: usize, word: &[u8]) -> bool {
    text.get(at..at + word.len())
        .is_some_and(|got| got.eq_ignore_ascii_case(word))
}

fn special_end(text: &[u8], at: usize) -> Option<(f64, usize)> {
    if starts_with_ignore_case(text, at, b"infinity") {
        return Some((f64::INFINITY, at + 8));
    }
    if starts_with_ignore_case(text, at, b"inf") {
        return Some((f64::INFINITY, at + 3));
    }
    if !starts_with_ignore_case(text, at, b"nan") {
        return None;
    }
    let after = at + 3;
    if text.get(after) == Some(&b'(') {
        let close = digits_end(text, after + 1, |b| b.is_ascii_alphanumeric() || *b == b'_');
        if text.get(close) == Some(&b')') {
            return Some((f64::NAN, close + 1));
        }
    }
    Some((f64::NAN, after))
}

fn exponent_end(text: &[u8], at: usize, marker: u8) -> (i64, usize) {
    if !text
        .get(at)
        .is_some_and(|b| b.eq_ignore_ascii_case(&marker))
    {
        return (0, at);
    }
    let mut digits_at = at + 1;
    let negative = text.get(digits_at) == Some(&b'-');
    if matches!(text.get(digits_at), Some(b'+' | b'-')) {
        digits_at += 1;
    }
    let end = digits_end(text, digits_at, u8::is_ascii_digit);
    if end == digits_at {
        return (0, at);
    }
    let magnitude = text[digits_at..end].iter().fold(0i64, |acc, d| {
        acc.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    });
    (if negative { -magnitude } else { magnitude }, end)
}

fn hex_end(text: &[u8], at: usize) -> Option<(f64, usize)> {
    let mut mantissa: u64 = 0;
    let mut scale: i64 = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut i = at;
    while let Some(&b) = text.get(i) {
        if b == b'.' && !seen_dot {
            seen_dot = true;
        } else if let Some(d) = char::from(b).to_digit(16) {
            seen_digit = true;
            if mantissa < 1 << 59 {
                mantissa = mantissa * 16 + u64::from(d);
                if seen_dot {
                    scale -= 4;
                }
            } else if !seen_dot {
                scale += 4;
            }
        } else {
            break;
        }
        i += 1;
    }
    if !seen_digit {
        return None;
    }
    let (exponent, end) = exponent_end(text, i, b'p');
    let mut power = scale.saturating_add(exponent).clamp(-2200, 2200);
    let mut value = mantissa as f64;
    while power != 0 && value != 0.0 && value.is_finite() {
        let step = power.clamp(-1000, 1000);
        value *= 2f64.powi(step as i32);
        power -= step;
    }
    Some((value, end))
}

fn decimal_end(text: &[u8], at: usize) -> Option<usize> {
    let int_end = digits_end(text, at, u8::is_ascii_digit);
    let mut end = int_end;
    let mut has_digits = int_end > at;
    if text.get(end) == Some(&b'.') {
        let frac_end = digits_end(text, end + 1, u8::is_ascii_digit);
        has_digits |= frac_end > end + 1;
        end = frac_end;
    }
    if !has_digits {
        return None;
    }
    Some(exponent_end(text, end, b'e').1)
}

pub fn parse_prefix(text: &[u8]) -> (f64, usize) {
    let start = digits_end(text, 0, |b| is_c_space(*b));
    let negative = text.get(start) == Some(&b'-');
    let body = start + usize::from(matches!(text.get(start), Some(b'+' | b'-')));
    let sign = if negative { -1.0 } else { 1.0 };
    if let Some((value, end)) = special_end(text, body) {
        return (sign * value, end);
    }
    let is_hex = text.get(body) == Some(&b'0') && matches!(text.get(body + 1), Some(b'x' | b'X'));
    if is_hex && let Some((value, end)) = hex_end(text, body + 2) {
        return (sign * value, end);
    }
    let Some(end) = decimal_end(text, body) else {
        return (0.0, 0);
    };
    let value = core::str::from_utf8(&text[body..end])
        .ok()
        .and_then(|number| number.parse::<f64>().ok())
        .unwrap_or(0.0);
    (sign * value, end)
}
