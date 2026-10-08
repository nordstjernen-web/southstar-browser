//! Southstar — Intl.Locale, locale canonicalization and Intl.getCanonicalLocales and supportedValuesOf.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{Method, Parts, arg, c_string, hide_text, new_instance, opt_str, set, set_text, text};

pub(crate) const METHODS: [Method; 3] = [
    ("toString", to_string, 0),
    ("maximize", identity, 0),
    ("minimize", identity, 0),
];

const CALENDARS: [&str; 2] = ["gregory", "iso8601"];
const COLLATIONS: [&str; 3] = ["default", "emoji", "eor"];
const NUMBERING_SYSTEMS: [&str; 1] = ["latn"];
const TIME_ZONES: [&str; 1] = ["UTC"];
const CURRENCIES: [&str; 18] = [
    "AUD", "BRL", "CAD", "CHF", "CNY", "EUR", "GBP", "HKD", "INR", "JPY", "KRW", "MXN", "NOK",
    "NZD", "RUB", "SEK", "USD", "ZAR",
];
const UNITS: [&str; 30] = [
    "acre",
    "bit",
    "byte",
    "celsius",
    "centimeter",
    "day",
    "degree",
    "fahrenheit",
    "gigabyte",
    "gram",
    "hectare",
    "hour",
    "inch",
    "kilogram",
    "kilometer",
    "liter",
    "megabyte",
    "meter",
    "mile",
    "milliliter",
    "millimeter",
    "millisecond",
    "minute",
    "month",
    "ounce",
    "percent",
    "pound",
    "second",
    "week",
    "year",
];

fn subtags(tag: &[u8]) -> Vec<&[u8]> {
    if tag.is_empty() {
        Vec::new()
    } else {
        tag.split(|&c| c == b'-').collect()
    }
}

pub(crate) fn canonicalize(tag: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(tag.len());
    for (i, part) in subtags(tag).into_iter().enumerate() {
        if !out.is_empty() {
            out.push(b'-');
        }
        let leading_alpha = part.first().is_some_and(u8::is_ascii_alphabetic);
        if i == 0 {
            out.extend(part.iter().map(u8::to_ascii_lowercase));
        } else if part.len() == 4 && leading_alpha {
            out.push(part[0].to_ascii_uppercase());
            out.extend(part[1..].iter().map(u8::to_ascii_lowercase));
        } else if part.len() == 2 && leading_alpha {
            out.extend(part.iter().map(u8::to_ascii_uppercase));
        } else {
            out.extend(part.iter().map(u8::to_ascii_lowercase));
        }
    }
    out
}

pub(crate) fn get_canonical_locales(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let mut out = Parts::new(scope);
    let Some(source) = args.first().filter(|v| !v.is_undefined() && !v.is_null()) else {
        return Ok(out.array);
    };
    let is_array = source.is_array();
    let count = if is_array {
        crate::length(scope, source)
    } else {
        1
    };
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for i in 0..count {
        let entry = if is_array {
            scope.get_index(source, i)
        } else {
            Ok(source.clone())
        };
        let Ok(tag) = entry.and_then(|entry| c_string(scope, &entry)) else {
            continue;
        };
        let canonical = canonicalize(&tag);
        if !seen.contains(&canonical) {
            let value = text(scope, &canonical);
            out.push_value(scope, value);
            seen.push(canonical);
        }
    }
    Ok(out.array)
}

pub(crate) fn supported_values_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let key = match args.first() {
        Some(key) => c_string(scope, key).ok(),
        None => None,
    };
    let values: &[&str] = match key.as_deref() {
        Some(b"calendar") => &CALENDARS,
        Some(b"collation") => &COLLATIONS,
        Some(b"currency") => &CURRENCIES,
        Some(b"numberingSystem") => &NUMBERING_SYSTEMS,
        Some(b"timeZone") => &TIME_ZONES,
        Some(b"unit") => &UNITS,
        _ => &[],
    };
    let mut out = Parts::new(scope);
    for value in values {
        let value = scope.string(value);
        out.push_value(scope, value);
    }
    Ok(out.array)
}

fn subtag_after(tag: &[u8], want: &[u8]) -> Option<Vec<u8>> {
    let parts = subtags(tag);
    for (i, part) in parts.iter().enumerate() {
        if !part.eq_ignore_ascii_case(b"u") {
            continue;
        }
        for j in i + 1..parts.len() {
            if parts[j].eq_ignore_ascii_case(want) && j + 1 < parts.len() {
                return Some(parts[j + 1].to_ascii_lowercase());
            }
        }
    }
    None
}

fn set_optional(scope: &mut Scope<'_>, object: &Value, key: &str, value: Option<&[u8]>) {
    match value {
        Some(value) => set_text(scope, object, key, value),
        None => set(scope, object, key, Value::undefined()),
    }
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let tag = match args.first() {
        Some(tag) if tag.is_string() => c_string(scope, tag).ok(),
        _ => None,
    };
    let canonical = canonicalize(tag.as_deref().unwrap_or(b"und"));
    let options = arg(args, 1);
    let parts = subtags(&canonical);
    let language = parts
        .first()
        .map_or_else(|| b"und".to_vec(), |first| first.to_ascii_lowercase());
    let mut script: Option<Vec<u8>> = None;
    let mut region: Option<Vec<u8>> = None;
    for part in parts.iter().skip(1) {
        if script.is_none() && part.len() == 4 && part[0].is_ascii_alphabetic() {
            script = Some(part.to_vec());
        } else if region.is_none() && (part.len() == 2 || part.len() == 3) {
            region = Some(part.to_ascii_uppercase());
        }
    }
    let calendar = opt_str(scope, &options, "calendar").or_else(|| subtag_after(&canonical, b"ca"));
    let collation =
        opt_str(scope, &options, "collation").or_else(|| subtag_after(&canonical, b"co"));
    let numbering =
        opt_str(scope, &options, "numberingSystem").or_else(|| subtag_after(&canonical, b"nu"));
    let hour_cycle =
        opt_str(scope, &options, "hourCycle").or_else(|| subtag_after(&canonical, b"hc"));

    let locale = new_instance(scope, this, "Locale");
    let mut base = language.clone();
    for subtag in [&script, &region].into_iter().flatten() {
        base.push(b'-');
        base.extend_from_slice(subtag);
    }
    set_text(scope, &locale, "baseName", &base);
    set_text(scope, &locale, "language", &language);
    set_optional(scope, &locale, "script", script.as_deref());
    set_optional(scope, &locale, "region", region.as_deref());
    set_optional(scope, &locale, "calendar", calendar.as_deref());
    set_optional(scope, &locale, "collation", collation.as_deref());
    set_optional(scope, &locale, "numberingSystem", numbering.as_deref());
    set_optional(scope, &locale, "hourCycle", hour_cycle.as_deref());
    hide_text(scope, &locale, "_tag", &canonical);
    Ok(locale)
}

fn to_string(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    match scope.get(this, "baseName") {
        Ok(base) if base.is_string() => Ok(base),
        _ => Ok(scope.string("und")),
    }
}

fn identity(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(this.clone())
}
