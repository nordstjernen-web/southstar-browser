//! Southstar — turning a rounded number into Intl.NumberFormat parts: notation, grouping, signs, styles and ranges.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::NumberFormatOptions;
use super::data::{self, CompactStep, LocaleData};
use super::decimal::{Decimal, Numeric};
use super::digits::DigitOptions;
use crate::plural::category_of;

const NBSP: &str = "\u{a0}";

pub(crate) struct Part {
    pub kind: &'static str,
    pub value: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignClass {
    Positive,
    Zero,
    NegativeZero,
    Negative,
    NotANumber,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sign {
    Plus,
    Minus,
}

fn part(kind: &'static str, value: impl Into<String>) -> Part {
    Part {
        kind,
        value: value.into(),
    }
}

pub(crate) fn join(parts: &[Part]) -> String {
    parts.iter().map(|p| p.value.as_str()).collect()
}

pub(crate) fn compact_steps(
    data_locale: &str,
    notation: &str,
    display: &str,
) -> &'static [CompactStep] {
    if notation != "compact" {
        return &[];
    }
    let data = data::locale_data(data_locale, "latn");
    if display == "long" {
        data.compact_long
    } else {
        data.compact_short
    }
}

fn step_for(steps: &[CompactStep], magnitude: i64) -> Option<&CompactStep> {
    steps.iter().rev().find(|step| step.from <= magnitude)
}

fn exponent_for_magnitude(notation: &str, steps: &[CompactStep], magnitude: i64) -> i64 {
    match notation {
        "scientific" => magnitude,
        "engineering" => magnitude.div_euclid(3) * 3,
        "compact" => step_for(steps, magnitude).map_or(0, |step| step.exponent),
        _ => 0,
    }
}

pub(crate) fn compute_exponent(
    digits: &DigitOptions,
    notation: &str,
    steps: &[CompactStep],
    negative: bool,
    x: &Decimal,
) -> (i64, i64) {
    if x.is_zero() || notation == "standard" {
        return (0, 0);
    }
    let magnitude = x.magnitude();
    let exponent = exponent_for_magnitude(notation, steps, magnitude);
    let rounded = digits.round(negative, &x.scaled(-exponent));
    if rounded.value.is_zero() || rounded.value.magnitude() == magnitude - exponent {
        return (exponent, magnitude);
    }
    (
        exponent_for_magnitude(notation, steps, magnitude + 1),
        magnitude + 1,
    )
}

fn affix_parts(text: &str, kind: &'static str) -> Vec<Part> {
    let is_space = |c: char| matches!(c, ' ' | '\u{a0}' | '\u{202f}');
    let body = text.trim_matches(is_space);
    let mut parts = Vec::new();
    if body.is_empty() {
        if !text.is_empty() {
            parts.push(part("literal", text));
        }
        return parts;
    }
    let start = text.find(body).unwrap_or(0);
    if start > 0 {
        parts.push(part("literal", &text[..start]));
    }
    parts.push(part(kind, body));
    if start + body.len() < text.len() {
        parts.push(part("literal", &text[start + body.len()..]));
    }
    parts
}

fn grouped_integer(nf: &NumberFormatOptions, data: &LocaleData, integer: &str) -> Vec<Part> {
    let numbering = nf.numbering_system.as_str();
    let minimum = match nf.use_grouping {
        None => usize::MAX,
        Some("always") => 1,
        Some("min2") => 2,
        Some(_) => data.minimum_grouping,
    };
    if minimum == usize::MAX || integer.len() < 3 + minimum {
        return vec![part("integer", data::transliterate(integer, numbering))];
    }
    let mut groups: Vec<&str> = Vec::new();
    let mut end = integer.len();
    let mut size = 3;
    while end > size {
        groups.push(&integer[end - size..end]);
        end -= size;
        size = data.secondary_group;
    }
    groups.push(&integer[..end]);
    let mut parts = Vec::new();
    for (index, group) in groups.iter().rev().enumerate() {
        if index > 0 {
            parts.push(part("group", data.group));
        }
        parts.push(part("integer", data::transliterate(group, numbering)));
    }
    parts
}

fn digit_parts(nf: &NumberFormatOptions, data: &LocaleData, text: &str) -> Vec<Part> {
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    let mut parts = grouped_integer(nf, data, integer);
    if !fraction.is_empty() {
        parts.push(part("decimal", data.decimal));
        parts.push(part(
            "fraction",
            data::transliterate(fraction, &nf.numbering_system),
        ));
    }
    parts
}

struct FormattedNumber {
    class: SignClass,
    parts: Vec<Part>,
    plural_text: String,
}

fn format_number(nf: &NumberFormatOptions, data: &LocaleData, x: &Numeric) -> FormattedNumber {
    let (negative, magnitude) = match x {
        Numeric::NotANumber => {
            return FormattedNumber {
                class: SignClass::NotANumber,
                parts: vec![part("nan", data.nan)],
                plural_text: String::new(),
            };
        }
        Numeric::Infinite { negative } => {
            return FormattedNumber {
                class: if *negative {
                    SignClass::Negative
                } else {
                    SignClass::Positive
                },
                parts: vec![part("infinity", "\u{221e}")],
                plural_text: String::new(),
            };
        }
        Numeric::Finite {
            negative,
            magnitude,
        } => (*negative, magnitude),
    };
    let magnitude = if nf.style == "percent" {
        magnitude.scaled(2)
    } else {
        magnitude.clone()
    };
    let steps = compact_steps(&nf.data_locale, nf.notation, nf.compact_display);
    let (exponent, used_magnitude) =
        compute_exponent(&nf.digits, nf.notation, steps, negative, &magnitude);
    let rounded = nf.digits.round(negative, &magnitude.scaled(-exponent));
    let class = match (rounded.value.is_zero(), negative) {
        (true, false) => SignClass::Zero,
        (true, true) => SignClass::NegativeZero,
        (false, false) => SignClass::Positive,
        (false, true) => SignClass::Negative,
    };
    let mut parts = digit_parts(nf, data, &rounded.text);
    match nf.notation {
        "scientific" | "engineering" => {
            parts.push(part("exponentSeparator", "E"));
            if exponent < 0 {
                parts.push(part("exponentMinusSign", data.minus));
            }
            let digits = exponent.unsigned_abs().to_string();
            parts.push(part(
                "exponentInteger",
                data::transliterate(&digits, &nf.numbering_system),
            ));
        }
        "compact" => {
            if let Some(step) = step_for(steps, used_magnitude).filter(|step| step.exponent != 0) {
                let one = category_of(&nf.data_locale, false, &rounded.text, 0) == "one";
                let text = if one { step.one } else { step.other };
                parts.extend(affix_parts(text, "compact"));
            }
        }
        _ => {}
    }
    FormattedNumber {
        class,
        parts,
        plural_text: rounded.text,
    }
}

fn sign_for(display: &str, class: SignClass) -> Option<Sign> {
    use SignClass::{Negative, NegativeZero, NotANumber, Positive, Zero};
    match (display, class) {
        ("never", _) => None,
        ("auto", Negative | NegativeZero) => Some(Sign::Minus),
        ("auto", _) => None,
        ("always", Negative | NegativeZero) => Some(Sign::Minus),
        ("always", _) => Some(Sign::Plus),
        ("exceptZero", Positive) => Some(Sign::Plus),
        ("exceptZero", Negative) => Some(Sign::Minus),
        ("exceptZero", NotANumber | Zero | NegativeZero) => None,
        ("negative", Negative) => Some(Sign::Minus),
        _ => None,
    }
}

fn sign_parts(data: &LocaleData, sign: Option<Sign>, approximately: bool) -> Vec<Part> {
    let mut parts = Vec::new();
    if approximately {
        parts.push(part("approximatelySign", data.approximately));
    }
    match sign {
        Some(Sign::Minus) => parts.push(part("minusSign", data.minus)),
        Some(Sign::Plus) => parts.push(part("plusSign", data.plus)),
        None => {}
    }
    parts
}

fn currency_symbol(nf: &NumberFormatOptions, code: &str) -> String {
    match nf.currency_display {
        "code" => code.to_owned(),
        "narrowSymbol" => data::currency_symbol(&nf.data_locale, code, true),
        _ => data::currency_symbol(&nf.data_locale, code, false),
    }
}

fn currency_parts(
    nf: &NumberFormatOptions,
    data: &LocaleData,
    sign: Option<Sign>,
    approximately: bool,
    number: Vec<Part>,
    plural_one: bool,
) -> Vec<Part> {
    let code = nf.currency.as_deref().unwrap_or("XXX");
    if nf.currency_display == "name" {
        let mut parts = sign_parts(data, sign, approximately);
        parts.extend(number);
        parts.push(part("literal", " "));
        parts.push(part(
            "currency",
            data::currency_name(&nf.data_locale, code, plural_one),
        ));
        return parts;
    }
    let symbol = currency_symbol(nf, code);
    let letter_edge = |c: Option<char>| c.is_some_and(char::is_alphabetic);
    if !data.currency.before {
        let space = if data.currency.space.is_empty() && letter_edge(symbol.chars().next()) {
            NBSP
        } else {
            data.currency.space
        };
        let mut parts = sign_parts(data, sign, approximately);
        parts.extend(number);
        if !space.is_empty() {
            parts.push(part("literal", space));
        }
        parts.push(part("currency", symbol));
        return parts;
    }
    let space = if !data.currency.space.is_empty() {
        data.currency.space
    } else if letter_edge(symbol.chars().last()) {
        NBSP
    } else {
        ""
    };
    let parenthesized = nf.currency_sign == "accounting"
        && data.accounting_parentheses
        && sign == Some(Sign::Minus);
    let mut parts = sign_parts(data, sign.filter(|_| !parenthesized), approximately);
    if parenthesized {
        parts.push(part("literal", "("));
    }
    parts.push(part("currency", symbol));
    if !space.is_empty() {
        parts.push(part("literal", space));
    }
    parts.extend(number);
    if parenthesized {
        parts.push(part("literal", ")"));
    }
    parts
}

fn style_parts(
    nf: &NumberFormatOptions,
    data: &LocaleData,
    sign: Option<Sign>,
    approximately: bool,
    number: Vec<Part>,
    plural_one: bool,
) -> Vec<Part> {
    match nf.style {
        "percent" => {
            let mut parts = sign_parts(data, sign, approximately);
            let space = data.percent.space;
            if data.percent.before {
                parts.push(part("percentSign", data.percent_sign));
                if !space.is_empty() {
                    parts.push(part("literal", space));
                }
                parts.extend(number);
            } else {
                parts.extend(number);
                if !space.is_empty() {
                    parts.push(part("literal", space));
                }
                parts.push(part("percentSign", data.percent_sign));
            }
            parts
        }
        "currency" => currency_parts(nf, data, sign, approximately, number, plural_one),
        "unit" => {
            let unit = nf.unit.as_deref().unwrap_or("");
            let pattern = data::unit_pattern(&nf.data_locale, unit, nf.unit_display, plural_one);
            let mut parts = affix_parts(&pattern.prefix, "unit");
            parts.extend(sign_parts(data, sign, approximately));
            parts.extend(number);
            parts.extend(affix_parts(&pattern.suffix, "unit"));
            parts
        }
        _ => {
            let mut parts = sign_parts(data, sign, approximately);
            parts.extend(number);
            parts
        }
    }
}

fn partition_with(nf: &NumberFormatOptions, x: &Numeric, approximately: bool) -> Vec<Part> {
    let data = data::locale_data(&nf.data_locale, &nf.numbering_system);
    let formatted = format_number(nf, data, x);
    let sign = sign_for(nf.sign_display, formatted.class);
    let plural_one = !formatted.plural_text.is_empty()
        && category_of(&nf.data_locale, false, &formatted.plural_text, 0) == "one";
    style_parts(nf, data, sign, approximately, formatted.parts, plural_one)
}

pub(crate) fn partition(nf: &NumberFormatOptions, x: &Numeric) -> Vec<Part> {
    partition_with(nf, x, false)
}

fn is_number_core(kind: &str) -> bool {
    matches!(
        kind,
        "integer"
            | "group"
            | "decimal"
            | "fraction"
            | "nan"
            | "infinity"
            | "exponentSeparator"
            | "exponentMinusSign"
            | "exponentInteger"
    )
}

fn core_bounds(parts: &[Part]) -> (usize, usize) {
    let start = parts
        .iter()
        .position(|p| is_number_core(p.kind))
        .unwrap_or(0);
    let end = parts
        .iter()
        .rposition(|p| is_number_core(p.kind))
        .map_or(parts.len(), |last| last + 1);
    (start, end)
}

pub(crate) fn partition_range(
    nf: &NumberFormatOptions,
    x: &Numeric,
    y: &Numeric,
) -> Vec<(Part, &'static str)> {
    let start = partition(nf, x);
    let end = partition(nf, y);
    if join(&start) == join(&end) {
        return partition_with(nf, x, true)
            .into_iter()
            .map(|p| (p, "shared"))
            .collect();
    }
    let data = data::locale_data(&nf.data_locale, &nf.numbering_system);
    let separator = data.range_separator;
    let (start_from, start_to) = core_bounds(&start);
    let (end_from, end_to) = core_bounds(&end);
    let affix_text = |parts: &[Part]| join(parts);
    let same_affixes = affix_text(&start[..start_from]) == affix_text(&end[..end_from])
        && affix_text(&start[start_to..]) == affix_text(&end[end_to..]);
    let affix_length = affix_text(&start[..start_from]).chars().count()
        + affix_text(&start[start_to..]).chars().count();
    let mut out: Vec<(Part, &'static str)> = Vec::new();
    if same_affixes && affix_length > 1 {
        let mut end = end;
        let end_core: Vec<Part> = end.drain(end_from..end_to).collect();
        let mut start = start;
        let suffix: Vec<Part> = start.drain(start_to..).collect();
        let core: Vec<Part> = start.drain(start_from..).collect();
        out.extend(start.into_iter().map(|p| (p, "shared")));
        out.extend(core.into_iter().map(|p| (p, "startRange")));
        out.push((part("literal", separator), "shared"));
        out.extend(end_core.into_iter().map(|p| (p, "endRange")));
        out.extend(suffix.into_iter().map(|p| (p, "shared")));
        return out;
    }
    let has_affixes =
        start_from > 0 || start_to < start.len() || end_from > 0 || end_to < end.len();
    let spaced = if has_affixes && !separator.starts_with(' ') {
        format!(" {separator} ")
    } else {
        separator.to_owned()
    };
    out.extend(start.into_iter().map(|p| (p, "startRange")));
    out.push((part("literal", spaced), "shared"));
    out.extend(end.into_iter().map(|p| (p, "endRange")));
    out
}
