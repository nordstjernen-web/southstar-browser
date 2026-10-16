//! Southstar — number formatting for serialized colours: six significant digits, never an exponent.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const SIGNIFICANT: i32 = 6;

pub fn number(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "calc(infinity)"
        } else {
            "calc(-infinity)"
        }
        .into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let magnitude = v.abs().log10().floor() as i32;
    let decimals = (SIGNIFICANT - 1 - magnitude).clamp(0, 20) as usize;
    let mut text = format!("{v:.decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".into() } else { text }
}

pub(crate) fn calc_number(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "infinity" } else { "-infinity" }.into();
    }
    number(v)
}

pub fn alpha_byte_text(a: u8) -> String {
    let f = f64::from(a) / 255.0;
    let mut text = String::new();
    for decimals in 1..=5usize {
        text = format!("{f:.decimals$}");
        let back: f64 = text.parse().unwrap_or(0.0);
        if (back * 255.0 + 0.5) as i32 == i32::from(a) {
            break;
        }
    }
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}
