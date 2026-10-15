//! Southstar — Intl.PluralRules and the CLDR plural rules NumberFormat and PluralRules choose categories with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, BoundFn, Scope, Value};

use crate::ecma402::{
    KeyRequest, OptionsBag, array_from_strings, canonicalize_locale_list, create_data_property,
    create_string_property, is_new_target, prototype_from_constructor, resolve_locale,
    supported_locales,
};
use crate::number::decimal::Numeric;
use crate::number::digits::{DigitOptions, read_digit_options};
use crate::number::format::{compact_steps, compute_exponent};
use crate::{arg, bind};

const ONE_OTHER: [&str; 2] = ["one", "other"];
const OTHER: [&str; 1] = ["other"];
const ONE_MANY_OTHER: [&str; 3] = ["one", "many", "other"];
const ONE_FEW_MANY_OTHER: [&str; 4] = ["one", "few", "many", "other"];
const ONE_FEW_OTHER: [&str; 3] = ["one", "few", "other"];
const ONE_TWO_FEW_OTHER: [&str; 4] = ["one", "two", "few", "other"];
const ONE_TWO_OTHER: [&str; 3] = ["one", "two", "other"];
const ZERO_ONE_OTHER: [&str; 3] = ["zero", "one", "other"];
const ALL_SIX: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];
const NO_ZERO: [&str; 5] = ["one", "two", "few", "many", "other"];

const OTHER_ONLY: [&str; 21] = [
    "ja", "ko", "zh", "th", "vi", "id", "ms", "lo", "my", "km", "yo", "ig", "jv", "su", "bo", "dz",
    "ii", "sah", "yue", "to", "wo",
];
const ONE_IF_INTEGER_ONE: [&str; 17] = [
    "en", "de", "nl", "sv", "fi", "et", "ur", "sw", "fy", "ia", "io", "ji", "lij", "sc", "yi",
    "af", "ast",
];
const ONE_IF_N_ONE: [&str; 21] = [
    "el", "bg", "hu", "tr", "eu", "nb", "nn", "no", "az", "ka", "kk", "ky", "mn", "sq", "ta", "te",
    "uz", "ml", "ne", "or", "ps",
];
const ONE_IF_ZERO_OR_ONE: [&str; 12] = [
    "fa", "hi", "bn", "gu", "kn", "am", "zu", "as", "doi", "mr", "pcm", "kok",
];
const MANY_MILLIONS: [&str; 5] = ["fr", "pt", "es", "it", "ca"];
const EAST_SLAVIC: [&str; 2] = ["ru", "uk"];
const SOUTH_SLAVIC: [&str; 3] = ["hr", "sr", "bs"];

const PLURAL_METHODS: [(&str, BoundFn, u32); 3] = [
    ("select", select, 1),
    ("selectRange", select_range, 2),
    ("resolvedOptions", resolved_options, 0),
];

pub(crate) struct Operands {
    integer: u64,
    integer_is_huge: bool,
    visible: usize,
    fraction: u64,
    fraction_trimmed: u64,
    exponent: i64,
}

fn low_digits(digits: &str) -> u64 {
    let start = digits.len().saturating_sub(18);
    digits[start..].parse().unwrap_or(0)
}

impl Operands {
    pub(crate) fn new(text: &str, exponent: i64) -> Operands {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let mut whole = whole.to_owned();
        let mut fraction = fraction.to_owned();
        if exponent > 0 {
            let shift = exponent as usize;
            let moved: String = fraction.chars().take(shift).collect();
            whole.push_str(&moved);
            whole.extend(std::iter::repeat_n('0', shift.saturating_sub(moved.len())));
            fraction = fraction.chars().skip(shift).collect();
        }
        let whole = whole.trim_start_matches('0');
        let trimmed = fraction.trim_end_matches('0');
        Operands {
            integer: low_digits(whole),
            integer_is_huge: whole.len() > 18,
            visible: fraction.len(),
            fraction: low_digits(&fraction),
            fraction_trimmed: low_digits(trimmed),
            exponent,
        }
    }

    fn is_integer(&self) -> bool {
        self.fraction_trimmed == 0
    }

    fn i_is(&self, value: u64) -> bool {
        !self.integer_is_huge && self.integer == value
    }

    fn n_is(&self, value: u64) -> bool {
        self.is_integer() && self.i_is(value)
    }

    fn n_mod(&self, modulus: u64) -> Option<u64> {
        self.is_integer().then_some(self.integer % modulus)
    }

    fn i_mod(&self, modulus: u64) -> u64 {
        self.integer % modulus
    }

    fn v_zero(&self) -> bool {
        self.visible == 0
    }

    fn many_millions(&self) -> bool {
        let compact = self.exponent != 0 && !(0..=5).contains(&self.exponent);
        let millions = self.exponent == 0
            && (self.integer != 0 || self.integer_is_huge)
            && self.i_mod(1_000_000) == 0
            && self.v_zero();
        compact || millions
    }
}

fn in_range(value: Option<u64>, low: u64, high: u64) -> bool {
    value.is_some_and(|v| (low..=high).contains(&v))
}

fn language(locale: &str) -> &str {
    locale.split('-').next().unwrap_or("en")
}

pub(crate) fn cardinal(locale: &str, o: &Operands) -> &'static str {
    let lang = language(locale);
    if OTHER_ONLY.contains(&lang) {
        return "other";
    }
    if ONE_IF_INTEGER_ONE.contains(&lang) {
        return if o.i_is(1) && o.v_zero() {
            "one"
        } else {
            "other"
        };
    }
    if ONE_IF_N_ONE.contains(&lang) {
        return if o.n_is(1) { "one" } else { "other" };
    }
    if ONE_IF_ZERO_OR_ONE.contains(&lang) {
        return if o.i_is(0) || o.n_is(1) {
            "one"
        } else {
            "other"
        };
    }
    if MANY_MILLIONS.contains(&lang) {
        let one = match lang {
            "fr" => o.i_is(0) || o.i_is(1),
            "pt" if !locale.contains("-PT") => o.i_is(0) || o.i_is(1),
            "es" => o.n_is(1),
            _ => o.i_is(1) && o.v_zero(),
        };
        return if one {
            "one"
        } else if o.many_millions() {
            "many"
        } else {
            "other"
        };
    }
    if EAST_SLAVIC.contains(&lang) {
        let (m10, m100) = (o.i_mod(10), o.i_mod(100));
        return if !o.v_zero() {
            "other"
        } else if m10 == 1 && m100 != 11 {
            "one"
        } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
            "few"
        } else {
            "many"
        };
    }
    if SOUTH_SLAVIC.contains(&lang) {
        let (m10, m100) = (o.i_mod(10), o.i_mod(100));
        let (f10, f100) = (o.fraction % 10, o.fraction % 100);
        return if (o.v_zero() && m10 == 1 && m100 != 11) || (f10 == 1 && f100 != 11) {
            "one"
        } else if (o.v_zero() && (2..=4).contains(&m10) && !(12..=14).contains(&m100))
            || ((2..=4).contains(&f10) && !(12..=14).contains(&f100))
        {
            "few"
        } else {
            "other"
        };
    }
    match lang {
        "da" => {
            if o.n_is(1) || (o.fraction_trimmed != 0 && (o.i_is(0) || o.i_is(1))) {
                "one"
            } else {
                "other"
            }
        }
        "is" => {
            let t = o.fraction_trimmed;
            if (t == 0 && o.i_mod(10) == 1 && o.i_mod(100) != 11) || (t % 10 == 1 && t % 100 != 11)
            {
                "one"
            } else {
                "other"
            }
        }
        "be" => {
            let (m10, m100) = (o.n_mod(10), o.n_mod(100));
            if m10 == Some(1) && m100 != Some(11) {
                "one"
            } else if in_range(m10, 2, 4) && !in_range(m100, 12, 14) {
                "few"
            } else if m10 == Some(0) || in_range(m10, 5, 9) || in_range(m100, 11, 14) {
                "many"
            } else {
                "other"
            }
        }
        "pl" => {
            let (m10, m100) = (o.i_mod(10), o.i_mod(100));
            if o.i_is(1) && o.v_zero() {
                "one"
            } else if o.v_zero() && (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else if o.v_zero()
                && ((!o.i_is(1) && m10 <= 1) || (5..=9).contains(&m10) || (12..=14).contains(&m100))
            {
                "many"
            } else {
                "other"
            }
        }
        "cs" | "sk" => {
            if o.i_is(1) && o.v_zero() {
                "one"
            } else if o.v_zero() && (2..=4).contains(&o.integer) && !o.integer_is_huge {
                "few"
            } else if !o.v_zero() {
                "many"
            } else {
                "other"
            }
        }
        "sl" => {
            let m100 = o.i_mod(100);
            if o.v_zero() && m100 == 1 {
                "one"
            } else if o.v_zero() && m100 == 2 {
                "two"
            } else if (o.v_zero() && (3..=4).contains(&m100)) || !o.v_zero() {
                "few"
            } else {
                "other"
            }
        }
        "lt" => {
            let (m10, m100) = (o.n_mod(10), o.n_mod(100));
            if m10 == Some(1) && !in_range(m100, 11, 19) {
                "one"
            } else if in_range(m10, 2, 9) && !in_range(m100, 11, 19) {
                "few"
            } else if o.fraction != 0 {
                "many"
            } else {
                "other"
            }
        }
        "lv" => {
            let (m10, m100) = (o.n_mod(10), o.n_mod(100));
            let (f10, f100) = (o.fraction % 10, o.fraction % 100);
            if m10 == Some(0)
                || in_range(m100, 11, 19)
                || (o.visible == 2 && (11..=19).contains(&f100))
            {
                "zero"
            } else if (m10 == Some(1) && m100 != Some(11))
                || (o.visible == 2 && f10 == 1 && f100 != 11)
                || (o.visible != 2 && f10 == 1)
            {
                "one"
            } else {
                "other"
            }
        }
        "ro" | "mo" => {
            if o.i_is(1) && o.v_zero() {
                "one"
            } else if !o.v_zero() || o.n_is(0) || (!o.n_is(1) && in_range(o.n_mod(100), 1, 19)) {
                "few"
            } else {
                "other"
            }
        }
        "he" | "iw" => {
            if (o.i_is(1) && o.v_zero()) || (o.i_is(0) && !o.v_zero()) {
                "one"
            } else if o.i_is(2) && o.v_zero() {
                "two"
            } else {
                "other"
            }
        }
        "ar" | "ars" => {
            let m100 = o.n_mod(100);
            if o.n_is(0) {
                "zero"
            } else if o.n_is(1) {
                "one"
            } else if o.n_is(2) {
                "two"
            } else if in_range(m100, 3, 10) {
                "few"
            } else if in_range(m100, 11, 99) {
                "many"
            } else {
                "other"
            }
        }
        "cy" => match (o.is_integer(), o.integer) {
            (true, 0) => "zero",
            (true, 1) => "one",
            (true, 2) => "two",
            (true, 3) => "few",
            (true, 6) => "many",
            _ => "other",
        },
        "ga" => match o.n_mod(u64::MAX) {
            Some(1) => "one",
            Some(2) => "two",
            Some(3..=6) => "few",
            Some(7..=10) => "many",
            _ => "other",
        },
        "gv" => {
            let m10 = o.i_mod(10);
            if o.v_zero() && m10 == 1 {
                "one"
            } else if o.v_zero() && m10 == 2 {
                "two"
            } else if o.v_zero() && [0, 20, 40, 60, 80].contains(&o.i_mod(100)) {
                "few"
            } else if !o.v_zero() {
                "many"
            } else {
                "other"
            }
        }
        "mk" => {
            let f = o.fraction;
            if (o.v_zero() && o.i_mod(10) == 1 && o.i_mod(100) != 11)
                || (f % 10 == 1 && f % 100 != 11)
            {
                "one"
            } else {
                "other"
            }
        }
        "fil" | "tl" => {
            let i10 = o.i_mod(10);
            let f10 = o.fraction % 10;
            if (o.v_zero() && (o.i_is(1) || o.i_is(2) || o.i_is(3)))
                || (o.v_zero() && ![4, 6, 9].contains(&i10))
                || (!o.v_zero() && ![4, 6, 9].contains(&f10))
            {
                "one"
            } else {
                "other"
            }
        }
        _ => {
            if o.i_is(1) && o.v_zero() {
                "one"
            } else {
                "other"
            }
        }
    }
}

pub(crate) fn ordinal(locale: &str, o: &Operands) -> &'static str {
    let (m10, m100) = (o.n_mod(10), o.n_mod(100));
    match language(locale) {
        "en" if m10 == Some(1) && m100 != Some(11) => "one",
        "en" if m10 == Some(2) && m100 != Some(12) => "two",
        "en" if m10 == Some(3) && m100 != Some(13) => "few",
        "fr" | "ms" | "vi" | "hy" | "ro" | "ga" if o.n_is(1) => "one",
        "sv" if matches!(m10, Some(1 | 2)) && !matches!(m100, Some(11 | 12)) => "one",
        "it" if o.n_is(11) || o.n_is(8) || o.n_is(80) || o.n_is(800) => "many",
        _ => "other",
    }
}

pub(crate) fn categories(locale: &str, ordinal: bool) -> &'static [&'static str] {
    let lang = language(locale);
    if ordinal {
        return match lang {
            "en" => &ONE_TWO_FEW_OTHER,
            "fr" | "ms" | "vi" | "hy" | "ro" | "ga" | "sv" => &ONE_OTHER,
            "it" => &["many", "other"],
            _ => &OTHER,
        };
    }
    if OTHER_ONLY.contains(&lang) {
        return &OTHER;
    }
    if MANY_MILLIONS.contains(&lang) {
        return &ONE_MANY_OTHER;
    }
    match lang {
        "ru" | "uk" | "be" | "pl" | "lt" => &ONE_FEW_MANY_OTHER,
        "cs" | "sk" => &ONE_FEW_MANY_OTHER,
        "hr" | "sr" | "bs" | "ro" | "mo" => &ONE_FEW_OTHER,
        "sl" => &ONE_TWO_FEW_OTHER,
        "he" | "iw" => &ONE_TWO_OTHER,
        "lv" => &ZERO_ONE_OTHER,
        "ar" | "ars" | "cy" => &ALL_SIX,
        "ga" | "gv" => &NO_ZERO,
        _ => &ONE_OTHER,
    }
}

pub(crate) fn category_of(locale: &str, ordinal: bool, text: &str, exponent: i64) -> &'static str {
    let operands = Operands::new(text, exponent);
    if ordinal {
        self::ordinal(locale, &operands)
    } else {
        cardinal(locale, &operands)
    }
}

const PLURAL_LANGUAGES: [&str; 32] = [
    "fr", "pt", "es", "it", "ca", "ru", "uk", "be", "pl", "lt", "cs", "sk", "hr", "sr", "bs", "ro",
    "sl", "he", "lv", "ar", "cy", "ga", "gv", "da", "is", "mk", "fil", "tl", "gl", "ast", "lij",
    "mo",
];

fn is_available(tag: &str) -> bool {
    let mut subtags = tag.split('-');
    let lang = subtags.next().unwrap_or("");
    let rest: Vec<&str> = subtags.collect();
    let shape_ok = match rest.as_slice() {
        [] => true,
        [one] => one.len() == 4 || one.len() == 2 || one.len() == 3,
        [script, region] => script.len() == 4 && (region.len() == 2 || region.len() == 3),
        _ => false,
    };
    shape_ok
        && (OTHER_ONLY.contains(&lang)
            || ONE_IF_INTEGER_ONE.contains(&lang)
            || ONE_IF_N_ONE.contains(&lang)
            || ONE_IF_ZERO_OR_ONE.contains(&lang)
            || PLURAL_LANGUAGES.contains(&lang))
}

#[derive(Clone)]
struct PluralRulesData {
    locale: String,
    data_locale: String,
    ordinal: bool,
    notation: &'static str,
    compact_display: &'static str,
    digits: DigitOptions,
}

fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    if !is_new_target(scope, this) {
        return Err(scope.type_error("Intl.PluralRules must be called with new"));
    }
    let intrinsic = arg(data, 0);
    let prototype = prototype_from_constructor(scope, this, &intrinsic, "PluralRules")?;
    let requested = canonicalize_locale_list(scope, &arg(args, 0))?;
    let options = OptionsBag::coerce(scope, &arg(args, 1))?;
    options.string(scope, "localeMatcher", &["lookup", "best fit"], "best fit")?;
    let kind = options.string(scope, "type", &["cardinal", "ordinal"], "cardinal")?;
    let notation = options.string(
        scope,
        "notation",
        &["standard", "scientific", "engineering", "compact"],
        "standard",
    )?;
    let compact_display = options.string(scope, "compactDisplay", &["short", "long"], "short")?;
    let digits = read_digit_options(scope, &options, 0, 3, notation)?;
    let resolution = resolve_locale(&requested, &is_available, &[] as &[KeyRequest<'_>]);
    let slots = PluralRulesData {
        locale: resolution.locale,
        data_locale: resolution.data_locale,
        ordinal: kind == "ordinal",
        notation,
        compact_display,
        digits,
    };
    Ok(scope.new_host_object(Some(&prototype), slots))
}

fn slots(scope: &mut Scope<'_>, this: &Value, method: &str) -> Result<PluralRulesData, Value> {
    scope.host_data::<PluralRulesData>(this).ok_or_else(|| {
        scope.type_error(&format!(
            "Intl.PluralRules.prototype.{method} called on an incompatible receiver"
        ))
    })
}

fn resolve_plural(rules: &PluralRulesData, n: f64) -> (&'static str, String) {
    let Numeric::Finite {
        negative,
        magnitude,
    } = Numeric::from_f64(n)
    else {
        return ("other", String::new());
    };
    let steps = compact_steps(&rules.data_locale, rules.notation, rules.compact_display);
    let (exponent, _) =
        compute_exponent(&rules.digits, rules.notation, steps, negative, &magnitude);
    let rounded = rules.digits.round(negative, &magnitude.scaled(-exponent));
    let category = category_of(&rules.data_locale, rules.ordinal, &rounded.text, exponent);
    let sign = if negative { "-" } else { "" };
    (category, format!("{sign}{}E{exponent}", rounded.text))
}

fn select(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let rules = slots(scope, this, "select")?;
    let n = scope.to_number(&arg(args, 0))?;
    let (category, _) = resolve_plural(&rules, n);
    Ok(scope.string(category))
}

fn select_range(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let rules = slots(scope, this, "selectRange")?;
    let (start, end) = (arg(args, 0), arg(args, 1));
    if start.is_undefined() || end.is_undefined() {
        return Err(scope.type_error("selectRange requires a start and an end"));
    }
    let x = scope.to_number(&start)?;
    let y = scope.to_number(&end)?;
    if x.is_nan() || y.is_nan() {
        return Err(scope.range_error("selectRange cannot select a range with NaN"));
    }
    let (start_category, start_text) = resolve_plural(&rules, x);
    let (end_category, end_text) = resolve_plural(&rules, y);
    let category = if start_text == end_text && x.is_finite() && y.is_finite() {
        start_category
    } else {
        end_category
    };
    Ok(scope.string(category))
}

fn resolved_options(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let rules = slots(scope, this, "resolvedOptions")?;
    let options = scope.new_object();
    create_string_property(scope, &options, "locale", &rules.locale)?;
    let kind = if rules.ordinal { "ordinal" } else { "cardinal" };
    create_string_property(scope, &options, "type", kind)?;
    create_string_property(scope, &options, "notation", rules.notation)?;
    if rules.notation == "compact" {
        create_string_property(scope, &options, "compactDisplay", rules.compact_display)?;
    }
    let digits = &rules.digits;
    create_data_property(
        scope,
        &options,
        "minimumIntegerDigits",
        Value::number(f64::from(digits.minimum_integer_digits)),
    )?;
    if let Some((minimum, maximum)) = digits.fraction_digits {
        create_data_property(
            scope,
            &options,
            "minimumFractionDigits",
            Value::number(f64::from(minimum)),
        )?;
        create_data_property(
            scope,
            &options,
            "maximumFractionDigits",
            Value::number(f64::from(maximum)),
        )?;
    }
    if let Some((minimum, maximum)) = digits.significant_digits {
        create_data_property(
            scope,
            &options,
            "minimumSignificantDigits",
            Value::number(f64::from(minimum)),
        )?;
        create_data_property(
            scope,
            &options,
            "maximumSignificantDigits",
            Value::number(f64::from(maximum)),
        )?;
    }
    let list = array_from_strings(scope, categories(&rules.data_locale, rules.ordinal))?;
    create_data_property(scope, &options, "pluralCategories", list)?;
    create_data_property(
        scope,
        &options,
        "roundingIncrement",
        Value::number(f64::from(digits.rounding_increment)),
    )?;
    create_string_property(scope, &options, "roundingMode", digits.rounding_mode)?;
    create_string_property(
        scope,
        &options,
        "roundingPriority",
        digits.rounding_priority,
    )?;
    create_string_property(
        scope,
        &options,
        "trailingZeroDisplay",
        digits.trailing_zero_display,
    )?;
    Ok(options)
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    supported_locales(scope, &arg(args, 0), &arg(args, 1), &is_available)
}

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) {
    let prototype = scope.new_object();
    let function = scope.bound_constructor(
        "PluralRules",
        0,
        constructor,
        std::slice::from_ref(&prototype),
    );
    for (name, method, arity) in PLURAL_METHODS {
        let method = scope.bound_function(name, arity, method, &[]);
        let _ = scope.define(&prototype, name, method, Attributes::METHOD);
    }
    let _ = scope.define_to_string_tag(&prototype, "Intl.PluralRules");
    let _ = scope.set_constructor(&function, &prototype);
    bind(
        scope,
        &function,
        "supportedLocalesOf",
        supported_locales_of,
        1,
    );
    let _ = scope.define(intl, "PluralRules", function, Attributes::METHOD);
}
