//! Southstar — Intl.NumberFormat: decimal, percent, currency, unit and compact formatting into parts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{
    Method, Parts, arg, arg_locale, bind_bound, hget_int, hget_str, hide, hide_text, join_parts,
    join_range, lang_in, new_instance, number_arg, opt_bool, opt_num, opt_str, printf, set,
    set_text,
};

pub(crate) const METHODS: [Method; 5] = [
    ("format", format, 1),
    ("formatToParts", format_to_parts, 1),
    ("formatRange", format_range, 2),
    ("formatRangeToParts", format_to_parts, 2),
    ("resolvedOptions", resolved, 0),
];

const COMMA_DECIMAL: [&[u8]; 35] = [
    b"de", b"fr", b"es", b"it", b"nl", b"pt", b"ru", b"pl", b"tr", b"sv", b"nb", b"nn", b"no",
    b"da", b"fi", b"cs", b"el", b"hu", b"ro", b"uk", b"id", b"vi", b"ca", b"hr", b"sk", b"sl",
    b"bg", b"lt", b"lv", b"et", b"is", b"af", b"sr", b"gl", b"eu",
];
const SPACE_GROUP: [&[u8]; 12] = [
    b"fr", b"ru", b"pl", b"uk", b"fi", b"sv", b"cs", b"hu", b"sk", b"nb", b"nn", b"no",
];
const CURRENCY_SYMBOLS: [(&str, &str); 19] = [
    ("USD", "$"),
    ("CAD", "CA$"),
    ("AUD", "A$"),
    ("NZD", "NZ$"),
    ("EUR", "\u{20ac}"),
    ("GBP", "\u{a3}"),
    ("JPY", "\u{a5}"),
    ("CNY", "CN\u{a5}"),
    ("INR", "\u{20b9}"),
    ("KRW", "\u{20a9}"),
    ("RUB", "\u{20bd}"),
    ("BRL", "R$"),
    ("ZAR", "R"),
    ("MXN", "MX$"),
    ("CHF", "CHF\u{a0}"),
    ("SEK", "kr"),
    ("NOK", "kr"),
    ("DKK", "kr"),
    ("HKD", "HK$"),
];
const COMPACT_UNITS: [(f64, &[u8]); 4] = [(1e12, b"T"), (1e9, b"B"), (1e6, b"M"), (1e3, b"K")];
const PRINTF_BUFFER: usize = 63;
const INTEGER_BUFFER: usize = 79;
const FRACTION_BUFFER: usize = 31;
const EXACT_DIGITS: usize = 1100;

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let locale = arg_locale(scope, &arg(args, 0));
    let options = arg(args, 1);
    let format = new_instance(scope, this, "NumberFormat");
    let style = opt_str(scope, &options, "style");
    let currency = opt_str(scope, &options, "currency");
    let currency_display = opt_str(scope, &options, "currencyDisplay");
    let unit = opt_str(scope, &options, "unit");
    let unit_display = opt_str(scope, &options, "unitDisplay");
    let notation = opt_str(scope, &options, "notation");
    let sign_display = opt_str(scope, &options, "signDisplay");

    hide_text(scope, &format, "_locale", &locale);
    hide_text(
        scope,
        &format,
        "_style",
        style.as_deref().unwrap_or(b"decimal"),
    );
    if let Some(currency) = &currency {
        hide_text(scope, &format, "_currency", currency);
    }
    hide_text(
        scope,
        &format,
        "_currencyDisplay",
        currency_display.as_deref().unwrap_or(b"symbol"),
    );
    if let Some(unit) = &unit {
        hide_text(scope, &format, "_unit", unit);
    }
    hide_text(
        scope,
        &format,
        "_unitDisplay",
        unit_display.as_deref().unwrap_or(b"short"),
    );
    hide_text(
        scope,
        &format,
        "_notation",
        notation.as_deref().unwrap_or(b"standard"),
    );
    hide_text(
        scope,
        &format,
        "_signDisplay",
        sign_display.as_deref().unwrap_or(b"auto"),
    );

    let (mut minimum_fraction, mut maximum_fraction, mut minimum_integer) = (-1, -1, 1);
    let mut out_of_range = false;
    if let Some(d) = opt_num(scope, &options, "minimumFractionDigits") {
        out_of_range |= !(0.0..=100.0).contains(&d);
        minimum_fraction = crate::c_trunc_i32(d);
    }
    if let Some(d) = opt_num(scope, &options, "maximumFractionDigits") {
        out_of_range |= !(0.0..=100.0).contains(&d);
        maximum_fraction = crate::c_trunc_i32(d);
    }
    if let Some(d) = opt_num(scope, &options, "minimumIntegerDigits") {
        out_of_range |= !(1.0..=21.0).contains(&d);
        minimum_integer = crate::c_trunc_i32(d);
    }
    hide(scope, &format, "_minfd", Value::int(minimum_fraction));
    hide(scope, &format, "_maxfd", Value::int(maximum_fraction));
    hide(
        scope,
        &format,
        "_minid",
        Value::int(minimum_integer.clamp(1, 21)),
    );
    let grouping = opt_bool(scope, &options, "useGrouping", 1) != 0;
    hide(scope, &format, "_grouping", Value::boolean(grouping));
    bind_bound(scope, &format, "format", format_bound, 1);
    if out_of_range {
        return Err(scope.range_error("Intl.NumberFormat digit option is out of range"));
    }
    Ok(format)
}

fn separators(locale: &[u8]) -> (&'static [u8], &'static [u8]) {
    let comma_decimal = lang_in(locale, &COMMA_DECIMAL);
    let decimal: &[u8] = if comma_decimal { b"," } else { b"." };
    if lang_in(locale, &SPACE_GROUP) {
        return ("\u{a0}".as_bytes(), decimal);
    }
    (if comma_decimal { b"." } else { b"," }, decimal)
}

fn currency_symbol(code: &[u8]) -> Vec<u8> {
    CURRENCY_SYMBOLS
        .iter()
        .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(code))
        .map_or_else(|| code.to_vec(), |(_, symbol)| symbol.as_bytes().to_vec())
}

fn group(digits: &[u8], separator: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(digits.len() * 2);
    for (i, &digit) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.extend_from_slice(separator);
        }
        out.push(digit);
    }
    out
}

fn fixed_buffer(value: f64, precision: usize) -> Vec<u8> {
    let mut text = printf::fixed(value, precision.min(EXACT_DIGITS)).into_bytes();
    if precision > EXACT_DIGITS {
        text.resize(text.len().max(PRINTF_BUFFER), b'0');
    }
    text.truncate(PRINTF_BUFFER);
    text
}

pub(crate) fn parts(scope: &mut Scope<'_>, format: &Value, number: f64) -> Value {
    let mut out = Parts::new(scope);
    let style = hget_str(scope, format, "_style");
    let locale = hget_str(scope, format, "_locale");
    let notation = hget_str(scope, format, "_notation");
    let sign_display = hget_str(scope, format, "_signDisplay");
    let mut minimum_fraction = hget_int(scope, format, "_minfd", -1);
    let mut maximum_fraction = hget_int(scope, format, "_maxfd", -1);
    let minimum_integer = hget_int(scope, format, "_minid", 1);
    let grouping = hget_int(scope, format, "_grouping", 1);
    let is_currency = style.as_deref() == Some(b"currency");
    let is_percent = style.as_deref() == Some(b"percent");
    let is_unit = style.as_deref() == Some(b"unit");
    let compact = notation.as_deref() == Some(b"compact");

    if number.is_nan() {
        out.push(scope, b"nan", b"NaN");
        return out.array;
    }
    let negative = number < 0.0 || (number == 0.0 && number.is_sign_negative());
    let mut magnitude = number.abs();
    if is_percent {
        magnitude *= 100.0;
    }
    if !magnitude.is_finite() {
        if negative {
            out.push(scope, b"minusSign", b"-");
        }
        out.push(scope, b"infinity", "\u{221e}".as_bytes());
        return out.array;
    }

    let mut suffix: &[u8] = b"";
    if compact
        && magnitude >= 1000.0
        && let Some(&(unit, label)) = COMPACT_UNITS.iter().find(|(unit, _)| magnitude >= *unit)
    {
        magnitude /= unit;
        suffix = label;
    }

    if minimum_fraction < 0 {
        minimum_fraction = if is_currency { 2 } else { 0 };
    }
    if maximum_fraction < 0 {
        maximum_fraction = if is_currency {
            2
        } else if is_percent {
            0
        } else if compact {
            minimum_fraction.max(1)
        } else {
            3
        };
    }
    if maximum_fraction < minimum_fraction {
        maximum_fraction = minimum_fraction;
    }

    let precision = maximum_fraction as usize;
    let scale = 10f64.powf(f64::from(maximum_fraction));
    let scaled = magnitude * scale;
    let digits = if scaled.is_finite() && scaled - scaled.floor() == 0.5 {
        fixed_buffer((scaled.floor() + 1.0) / scale, precision)
    } else {
        fixed_buffer(magnitude, precision)
    };
    let (mut integer, mut fraction) = match digits.iter().position(|&c| c == b'.') {
        Some(dot) => (
            digits[..dot.min(INTEGER_BUFFER)].to_vec(),
            digits[dot + 1..]
                .iter()
                .take(FRACTION_BUFFER)
                .copied()
                .collect(),
        ),
        None => (
            digits[..digits.len().min(INTEGER_BUFFER)].to_vec(),
            Vec::new(),
        ),
    };
    while fraction.len() as i64 > i64::from(minimum_fraction) && fraction.last() == Some(&b'0') {
        fraction.pop();
    }
    while (integer.len() as i64) < i64::from(minimum_integer) && integer.len() < INTEGER_BUFFER {
        integer.insert(0, b'0');
    }

    let (group_separator, decimal_separator) = separators(locale.as_deref().unwrap_or_default());
    if negative {
        out.push(scope, b"minusSign", b"-");
    } else if matches!(sign_display.as_deref(), Some(b"always" | b"exceptZero")) && magnitude != 0.0
    {
        out.push(scope, b"plusSign", b"+");
    }
    if is_currency {
        let code = hget_str(scope, format, "_currency");
        let display = hget_str(scope, format, "_currencyDisplay");
        let symbol = if display.as_deref() == Some(b"code") {
            code.unwrap_or_default()
        } else {
            currency_symbol(code.as_deref().unwrap_or(b"USD"))
        };
        out.push(scope, b"currency", &symbol);
    }
    let integer = if grouping != 0 && !compact {
        group(&integer, group_separator)
    } else {
        integer
    };
    out.push(scope, b"integer", &integer);
    if !fraction.is_empty() {
        out.push(scope, b"decimal", decimal_separator);
        out.push(scope, b"fraction", &fraction);
    }
    if !suffix.is_empty() {
        out.push(scope, b"compact", suffix);
    }
    if is_percent {
        out.push(scope, b"literal", b"");
        out.push(scope, b"percentSign", b"%");
    }
    if is_unit {
        let unit = hget_str(scope, format, "_unit");
        out.push(scope, b"literal", "\u{a0}".as_bytes());
        out.push(scope, b"unit", unit.as_deref().unwrap_or_default());
    }
    out.array
}

fn format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let number = number_arg(scope, args, 0, 0.0);
    let parts = parts(scope, this, number);
    Ok(join_parts(scope, &parts))
}

fn format_bound(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    format(scope, &arg(data, 0), args)
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let number = number_arg(scope, args, 0, 0.0);
    Ok(parts(scope, this, number))
}

fn format_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let a = number_arg(scope, args, 0, 0.0);
    let b = number_arg(scope, args, 1, 0.0);
    let parts_a = parts(scope, this, a);
    let start = join_parts(scope, &parts_a);
    let parts_b = parts(scope, this, b);
    let end = join_parts(scope, &parts_b);
    Ok(join_range(scope, &start, &end))
}

fn resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let options = scope.new_object();
    let locale = hget_str(scope, this, "_locale");
    let style = hget_str(scope, this, "_style");
    let notation = hget_str(scope, this, "_notation");
    let currency = hget_str(scope, this, "_currency");
    set_text(
        scope,
        &options,
        "locale",
        locale.as_deref().unwrap_or(b"en-US"),
    );
    set_text(scope, &options, "numberingSystem", b"latn");
    set_text(
        scope,
        &options,
        "style",
        style.as_deref().unwrap_or(b"decimal"),
    );
    set_text(
        scope,
        &options,
        "notation",
        notation.as_deref().unwrap_or(b"standard"),
    );
    if let Some(currency) = &currency {
        set_text(scope, &options, "currency", currency);
    }
    let grouping = hget_int(scope, this, "_grouping", 1) != 0;
    set(scope, &options, "useGrouping", Value::boolean(grouping));
    let mut minimum_fraction = hget_int(scope, this, "_minfd", -1);
    let mut maximum_fraction = hget_int(scope, this, "_maxfd", -1);
    let is_currency = style.as_deref() == Some(b"currency");
    if minimum_fraction < 0 {
        minimum_fraction = if is_currency { 2 } else { 0 };
    }
    if maximum_fraction < 0 {
        maximum_fraction = if is_currency {
            2
        } else if style.as_deref() == Some(b"percent") {
            0
        } else {
            3
        };
    }
    let minimum_integer = hget_int(scope, this, "_minid", 1);
    set(
        scope,
        &options,
        "minimumIntegerDigits",
        Value::int(minimum_integer),
    );
    set(
        scope,
        &options,
        "minimumFractionDigits",
        Value::int(minimum_fraction),
    );
    set(
        scope,
        &options,
        "maximumFractionDigits",
        Value::int(maximum_fraction),
    );
    Ok(options)
}
