//! Southstar — C printf's %.Pg and %.Pf for doubles, byte for byte and whatever the locale, and ECMAScript's Number::toString.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

fn non_finite(value: f64) -> Option<String> {
    if value.is_nan() {
        Some(
            if value.is_sign_negative() {
                "-nan"
            } else {
                "nan"
            }
            .to_owned(),
        )
    } else if value.is_infinite() {
        Some(if value < 0.0 { "-inf" } else { "inf" }.to_owned())
    } else {
        None
    }
}

fn strip_fraction_zeros(mut text: String) -> String {
    if text.contains('.') {
        let kept = text.trim_end_matches('0').trim_end_matches('.').len();
        text.truncate(kept);
    }
    text
}

pub fn fixed(value: f64, precision: usize) -> String {
    non_finite(value).unwrap_or_else(|| format!("{value:.precision$}"))
}

pub fn general(value: f64, precision: usize) -> String {
    if let Some(text) = non_finite(value) {
        return text;
    }
    let precision = precision.max(1);
    let scientific = format!("{:.*e}", precision - 1, value);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i64 = exponent.parse().unwrap_or(0);
    let precision = precision as i64;
    if exponent < -4 || exponent >= precision {
        let sign = if exponent < 0 { '-' } else { '+' };
        let mantissa = strip_fraction_zeros(mantissa.to_owned());
        return format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs());
    }
    let decimals = usize::try_from(precision - 1 - exponent).unwrap_or(0);
    strip_fraction_zeros(format!("{value:.decimals$}"))
}

pub fn g(value: f64) -> String {
    general(value, 6)
}

pub fn js(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let shortest = format!("{:e}", value.abs());
    let (mantissa, exponent) = shortest.split_once('e').unwrap_or((&shortest, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i64;
    let n = exponent.parse::<i64>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        format!("{int}.{frac}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let exp_sign = if n - 1 < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        let dot = if rest.is_empty() { "" } else { "." };
        format!("{first}{dot}{rest}e{exp_sign}{}", (n - 1).unsigned_abs())
    };
    format!("{sign}{body}")
}
