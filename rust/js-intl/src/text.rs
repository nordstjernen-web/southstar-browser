//! Southstar — Intl.PluralRules, ListFormat, RelativeTimeFormat, DisplayNames, DurationFormat and Segmenter.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{
    Method, Parts, arg, arg_locale, bind, c_string, ffi, hget_str, hide_text, join_parts,
    lang_subtag, new_instance, number_arg, opt_str, printf, set, set_text, text,
};

pub(crate) const PLURAL_METHODS: [Method; 3] = [
    ("select", plural_select, 1),
    ("selectRange", plural_select_range, 2),
    ("resolvedOptions", plural_resolved, 0),
];
pub(crate) const LIST_METHODS: [Method; 3] = [
    ("format", list_format, 1),
    ("formatToParts", list_format_to_parts, 1),
    ("resolvedOptions", list_resolved, 0),
];
pub(crate) const RELATIVE_METHODS: [Method; 3] = [
    ("format", relative_format, 2),
    ("formatToParts", relative_format_to_parts, 2),
    ("resolvedOptions", relative_resolved, 0),
];
pub(crate) const DISPLAY_METHODS: [Method; 2] = [
    ("of", display_of, 1),
    ("resolvedOptions", display_resolved, 0),
];
pub(crate) const DURATION_METHODS: [Method; 3] = [
    ("format", duration_format, 1),
    ("formatToParts", duration_format_to_parts, 1),
    ("resolvedOptions", duration_resolved, 0),
];
pub(crate) const SEGMENTER_METHODS: [Method; 1] = [("segment", segment, 1)];

const NO_CATEGORIES: [&[u8]; 11] = [
    b"ja", b"zh", b"ko", b"th", b"vi", b"id", b"ms", b"lo", b"my", b"km", b"fa",
];
const FRENCH: [&[u8]; 3] = [b"fr", b"ff", b"kab"];
const SLAVIC: [&[u8]; 3] = [b"ru", b"uk", b"be"];
const WEST_SLAVIC: [&[u8]; 2] = [b"cs", b"sk"];
const PLURAL_SAMPLES: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 5.0, 11.0, 21.0, 100.0];
const RELATIVE_WORDS: [(&[u8], i32, &[u8]); 16] = [
    (b"second", 0, b"now"),
    (b"day", -1, b"yesterday"),
    (b"day", 0, b"today"),
    (b"day", 1, b"tomorrow"),
    (b"week", -1, b"last week"),
    (b"week", 0, b"this week"),
    (b"week", 1, b"next week"),
    (b"month", -1, b"last month"),
    (b"month", 0, b"this month"),
    (b"month", 1, b"next month"),
    (b"quarter", -1, b"last quarter"),
    (b"quarter", 0, b"this quarter"),
    (b"quarter", 1, b"next quarter"),
    (b"year", -1, b"last year"),
    (b"year", 0, b"this year"),
    (b"year", 1, b"next year"),
];
const REGIONS: [(&[u8], &[u8]); 31] = [
    (b"US", b"United States"),
    (b"GB", b"United Kingdom"),
    (b"FR", b"France"),
    (b"DE", b"Germany"),
    (b"ES", b"Spain"),
    (b"IT", b"Italy"),
    (b"NL", b"Netherlands"),
    (b"NO", b"Norway"),
    (b"SE", b"Sweden"),
    (b"DK", b"Denmark"),
    (b"FI", b"Finland"),
    (b"JP", b"Japan"),
    (b"CN", b"China"),
    (b"KR", b"South Korea"),
    (b"IN", b"India"),
    (b"BR", b"Brazil"),
    (b"CA", b"Canada"),
    (b"AU", b"Australia"),
    (b"RU", b"Russia"),
    (b"MX", b"Mexico"),
    (b"PT", b"Portugal"),
    (b"PL", b"Poland"),
    (b"CH", b"Switzerland"),
    (b"BE", b"Belgium"),
    (b"AT", b"Austria"),
    (b"IE", b"Ireland"),
    (b"NZ", b"New Zealand"),
    (b"ZA", b"South Africa"),
    (b"AR", b"Argentina"),
    (b"GR", b"Greece"),
    (b"TR", b"Turkey"),
];
const LANGUAGES: [(&[u8], &str); 28] = [
    (b"en", "English"),
    (b"fr", "French"),
    (b"de", "German"),
    (b"es", "Spanish"),
    (b"it", "Italian"),
    (b"nl", "Dutch"),
    (b"no", "Norwegian"),
    (b"nb", "Norwegian Bokmål"),
    (b"nn", "Norwegian Nynorsk"),
    (b"sv", "Swedish"),
    (b"da", "Danish"),
    (b"fi", "Finnish"),
    (b"ja", "Japanese"),
    (b"zh", "Chinese"),
    (b"ko", "Korean"),
    (b"ru", "Russian"),
    (b"pt", "Portuguese"),
    (b"pl", "Polish"),
    (b"ar", "Arabic"),
    (b"hi", "Hindi"),
    (b"tr", "Turkish"),
    (b"el", "Greek"),
    (b"cs", "Czech"),
    (b"uk", "Ukrainian"),
    (b"he", "Hebrew"),
    (b"th", "Thai"),
    (b"vi", "Vietnamese"),
    (b"id", "Indonesian"),
];
const SCRIPTS: [(&[u8], &[u8]); 11] = [
    (b"Latn", b"Latin"),
    (b"Cyrl", b"Cyrillic"),
    (b"Grek", b"Greek"),
    (b"Arab", b"Arabic"),
    (b"Hans", b"Simplified Han"),
    (b"Hant", b"Traditional Han"),
    (b"Jpan", b"Japanese"),
    (b"Kore", b"Korean"),
    (b"Hebr", b"Hebrew"),
    (b"Deva", b"Devanagari"),
    (b"Thai", b"Thai"),
];
const DURATION_UNITS: [(&str, &str, &str, &str); 10] = [
    ("years", "year", "y", "yr"),
    ("months", "month", "mo", "mth"),
    ("weeks", "week", "w", "wk"),
    ("days", "day", "d", "day"),
    ("hours", "hour", "h", "hr"),
    ("minutes", "minute", "m", "min"),
    ("seconds", "second", "s", "sec"),
    ("milliseconds", "millisecond", "ms", "ms"),
    ("microseconds", "microsecond", "\u{b5}s", "\u{b5}s"),
    ("nanoseconds", "nanosecond", "ns", "ns"),
];
const LIST_LIMIT: u32 = 1 << 20;

fn simple_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    service: &str,
    settings: &[(&str, &str, &[u8])],
) -> Value {
    let locale = arg_locale(scope, &arg(args, 0));
    let options = arg(args, 1);
    let instance = new_instance(scope, this, service);
    let values: Vec<Option<Vec<u8>>> = settings
        .iter()
        .map(|&(option, _, _)| opt_str(scope, &options, option))
        .collect();
    hide_text(scope, &instance, "_locale", &locale);
    for (&(_, slot, default), value) in settings.iter().zip(&values) {
        hide_text(scope, &instance, slot, value.as_deref().unwrap_or(default));
    }
    instance
}

fn resolved_options(
    scope: &mut Scope<'_>,
    this: &Value,
    settings: &[(&str, &str, &[u8])],
    numbering: bool,
) -> Value {
    resolved_with_locale(scope, this, settings, numbering).0
}

fn resolved_with_locale(
    scope: &mut Scope<'_>,
    this: &Value,
    settings: &[(&str, &str, &[u8])],
    numbering: bool,
) -> (Value, Option<Vec<u8>>) {
    let options = scope.new_object();
    let locale = hget_str(scope, this, "_locale");
    let values: Vec<Option<Vec<u8>>> = settings
        .iter()
        .map(|&(_, slot, _)| hget_str(scope, this, slot))
        .collect();
    set_text(
        scope,
        &options,
        "locale",
        locale.as_deref().unwrap_or(b"en-US"),
    );
    for (&(option, _, default), value) in settings.iter().zip(&values) {
        set_text(scope, &options, option, value.as_deref().unwrap_or(default));
    }
    if numbering {
        set_text(scope, &options, "numberingSystem", b"latn");
    }
    (options, locale)
}

const PLURAL_SETTINGS: [(&str, &str, &[u8]); 1] = [("type", "_type", b"cardinal")];
const LIST_SETTINGS: [(&str, &str, &[u8]); 2] = [
    ("type", "_type", b"conjunction"),
    ("style", "_style", b"long"),
];
const RELATIVE_SETTINGS: [(&str, &str, &[u8]); 2] = [
    ("numeric", "_numeric", b"always"),
    ("style", "_style", b"long"),
];
const DISPLAY_SETTINGS: [(&str, &str, &[u8]); 3] = [
    ("type", "_type", b"language"),
    ("style", "_style", b"long"),
    ("fallback", "_fallback", b"code"),
];
const DURATION_SETTINGS: [(&str, &str, &[u8]); 1] = [("style", "_style", b"short")];
const SEGMENTER_SETTINGS: [(&str, &str, &[u8]); 1] = [("granularity", "_granularity", b"grapheme")];

pub(crate) fn plural_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "PluralRules",
        &PLURAL_SETTINGS,
    ))
}

fn plural_category(locale: &[u8], n: f64) -> &'static [u8] {
    let lang = lang_subtag(locale);
    let lang = &lang[..];
    let i = n.abs().floor();
    let integer = crate::c_long_trunc(i);
    let (mod10, mod100) = (integer % 10, integer % 100);
    let teens = (12..=14).contains(&mod100);
    if NO_CATEGORIES.contains(&lang) {
        return b"other";
    }
    if FRENCH.contains(&lang) {
        return if i == 0.0 || i == 1.0 {
            b"one"
        } else {
            b"other"
        };
    }
    if lang == b"ar" {
        return if n == 0.0 {
            b"zero"
        } else if n == 1.0 {
            b"one"
        } else if n == 2.0 {
            b"two"
        } else if (3..=10).contains(&mod100) {
            b"few"
        } else if (11..=99).contains(&mod100) {
            b"many"
        } else {
            b"other"
        };
    }
    if lang == b"pl" {
        return if i == 1.0 {
            b"one"
        } else if (2..=4).contains(&mod10) && !teens {
            b"few"
        } else if mod10 <= 1 || (5..=9).contains(&mod10) || teens {
            b"many"
        } else {
            b"other"
        };
    }
    if SLAVIC.contains(&lang) {
        return if mod10 == 1 && mod100 != 11 {
            b"one"
        } else if (2..=4).contains(&mod10) && !teens {
            b"few"
        } else if mod10 == 0 || (5..=9).contains(&mod10) || (11..=14).contains(&mod100) {
            b"many"
        } else {
            b"other"
        };
    }
    if WEST_SLAVIC.contains(&lang) {
        return if i == 1.0 {
            b"one"
        } else if (2.0..=4.0).contains(&i) {
            b"few"
        } else {
            b"other"
        };
    }
    if n == 1.0 { b"one" } else { b"other" }
}

fn plural_select(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let n = number_arg(scope, args, 0, 0.0);
    let locale = hget_str(scope, this, "_locale");
    let category = plural_category(locale.as_deref().unwrap_or(b"en"), n);
    Ok(text(scope, category))
}

fn plural_select_range(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let n = number_arg(scope, args, 1, 1.0);
    let locale = hget_str(scope, this, "_locale");
    let category = plural_category(locale.as_deref().unwrap_or(b"en"), n);
    Ok(text(scope, category))
}

fn plural_resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let (options, locale) = resolved_with_locale(scope, this, &PLURAL_SETTINGS, false);
    let probe = locale.as_deref().unwrap_or(b"en");
    let mut categories: Vec<&[u8]> = Vec::new();
    for sample in PLURAL_SAMPLES {
        let category = plural_category(probe, sample);
        if !categories.contains(&category) && categories.len() < 6 {
            categories.push(category);
        }
    }
    let mut list = Parts::new(scope);
    for category in categories {
        let value = text(scope, category);
        list.push_value(scope, value);
    }
    set(scope, &options, "pluralCategories", list.array);
    Ok(options)
}

pub(crate) fn list_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "ListFormat",
        &LIST_SETTINGS,
    ))
}

fn list_parts(scope: &mut Scope<'_>, this: &Value, list: &Value) -> Value {
    let mut out = Parts::new(scope);
    let kind = hget_str(scope, this, "_type");
    let count = crate::length(scope, list).min(LIST_LIMIT);
    let items: Vec<Vec<u8>> = (0..count)
        .map(|i| {
            scope
                .get_index(list, i)
                .and_then(|item| c_string(scope, &item))
                .unwrap_or_default()
        })
        .collect();
    let is_unit = kind.as_deref() == Some(b"unit");
    let word: &[u8] = if kind.as_deref() == Some(b"disjunction") {
        b"or"
    } else {
        b"and"
    };
    for (i, item) in items.iter().enumerate() {
        out.push(scope, b"element", item);
        if i + 1 < items.len() {
            if is_unit || i + 2 != items.len() {
                out.push(scope, b"literal", b", ");
            } else {
                let lead: &[u8] = if items.len() == 2 { b" " } else { b", " };
                let literal = [lead, word, b" "].concat();
                out.push(scope, b"literal", &literal);
            }
        }
    }
    out.array
}

fn list_format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let list = arg(args, 0);
    if !list.is_object() {
        return Ok(scope.string(""));
    }
    let parts = list_parts(scope, this, &list);
    Ok(join_parts(scope, &parts))
}

fn list_format_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let list = arg(args, 0);
    if !list.is_object() {
        return Ok(scope.new_array());
    }
    Ok(list_parts(scope, this, &list))
}

fn list_resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(resolved_options(scope, this, &LIST_SETTINGS, false))
}

pub(crate) fn relative_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "RelativeTimeFormat",
        &RELATIVE_SETTINGS,
    ))
}

fn relative_unit(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    let mut unit = c_string(scope, value).unwrap_or_default();
    unit.make_ascii_lowercase();
    if unit.last() == Some(&b's') {
        unit.pop();
    }
    unit
}

fn relative_word(unit: &[u8], value: i32) -> Option<&'static [u8]> {
    RELATIVE_WORDS
        .iter()
        .find(|&&(known, at, _)| known == unit && at == value)
        .map(|&(_, _, word)| word)
}

fn relative_setup(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> (f64, Vec<u8>, Option<&'static [u8]>) {
    let value = number_arg(scope, args, 0, 0.0);
    let unit = relative_unit(scope, &arg(args, 1));
    let numeric = hget_str(scope, this, "_numeric");
    let word = if numeric.as_deref() == Some(b"auto") {
        relative_word(&unit, crate::c_trunc_i32(value))
    } else {
        None
    };
    (value, unit, word)
}

fn relative_tail(unit: &[u8], value: f64) -> Vec<u8> {
    let plural: &[u8] = if value.abs() != 1.0 { b"s" } else { b"" };
    let ago: &[u8] = if value < 0.0 { b" ago" } else { b"" };
    [b" ", unit, plural, ago].concat()
}

fn relative_format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (value, unit, word) = relative_setup(scope, this, args);
    if let Some(word) = word {
        return Ok(text(scope, word));
    }
    let lead: &[u8] = if value >= 0.0 { b"in " } else { b"" };
    let amount = printf::general(value.abs());
    let phrase = [lead, amount.as_bytes(), &relative_tail(&unit, value)].concat();
    Ok(text(scope, &phrase))
}

fn relative_format_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (value, unit, word) = relative_setup(scope, this, args);
    let mut out = Parts::new(scope);
    if let Some(word) = word {
        out.push(scope, b"literal", word);
        return Ok(out.array);
    }
    if value >= 0.0 {
        out.push(scope, b"literal", b"in ");
    }
    let amount = printf::general(value.abs());
    let integer = crate::part(scope, b"integer", amount.as_bytes());
    set_text(scope, &integer, "unit", &unit);
    out.push_value(scope, integer);
    out.push(scope, b"literal", &relative_tail(&unit, value));
    Ok(out.array)
}

fn relative_resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(resolved_options(scope, this, &RELATIVE_SETTINGS, true))
}

pub(crate) fn display_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "DisplayNames",
        &DISPLAY_SETTINGS,
    ))
}

fn display_of(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(code) = args.first() else {
        return Ok(Value::undefined());
    };
    let Ok(code) = c_string(scope, code) else {
        return Ok(Value::undefined());
    };
    let kind = hget_str(scope, this, "_type");
    let fallback = hget_str(scope, this, "_fallback");
    let name: Option<&[u8]> = match kind.as_deref() {
        Some(b"region") => REGIONS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(&code))
            .map(|&(_, name)| name),
        Some(b"script") => SCRIPTS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(&code))
            .map(|&(_, name)| name),
        Some(b"currency") => None,
        _ => {
            let base = lang_subtag(&code);
            LANGUAGES
                .iter()
                .find(|(known, _)| *known == base)
                .map(|&(_, name)| name.as_bytes())
        }
    };
    Ok(match name {
        Some(name) => text(scope, name),
        None if fallback.as_deref() == Some(b"none") => Value::undefined(),
        None => text(scope, &code),
    })
}

fn display_resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(resolved_options(scope, this, &DISPLAY_SETTINGS, false))
}

pub(crate) fn duration_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "DurationFormat",
        &DURATION_SETTINGS,
    ))
}

fn duration_parts(scope: &mut Scope<'_>, this: &Value, duration: &Value) -> Value {
    let mut out = Parts::new(scope);
    let style = hget_str(scope, this, "_style");
    let narrow = style.as_deref() == Some(b"narrow");
    let digital = style.as_deref() == Some(b"digital");
    let long = style.as_deref() == Some(b"long");
    let mut segments: Vec<String> = Vec::new();
    for (field, unit, narrow_label, short_label) in DURATION_UNITS {
        let amount = match scope.get(duration, field) {
            Ok(value) if value.is_undefined() => continue,
            Ok(value) => scope.to_number(&value).unwrap_or(f64::NAN),
            Err(_) => f64::NAN,
        };
        if amount == 0.0 {
            continue;
        }
        let number = printf::general(amount);
        segments.push(if narrow {
            format!("{number}{narrow_label}")
        } else if long {
            let plural = if amount != 1.0 { "s" } else { "" };
            format!("{number} {unit}{plural}")
        } else {
            format!("{number} {short_label}")
        });
    }
    let separator: &[u8] = if narrow || digital { b" " } else { b", " };
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            out.push(scope, b"literal", separator);
        }
        out.push(scope, b"element", segment.as_bytes());
    }
    if segments.is_empty() {
        out.push(scope, b"element", b"0 sec");
    }
    out.array
}

fn duration_format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let duration = arg(args, 0);
    if !duration.is_object() {
        return Ok(scope.string(""));
    }
    let parts = duration_parts(scope, this, &duration);
    Ok(join_parts(scope, &parts))
}

fn duration_format_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let duration = arg(args, 0);
    if !duration.is_object() {
        return Ok(scope.new_array());
    }
    Ok(duration_parts(scope, this, &duration))
}

fn duration_resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(resolved_options(scope, this, &DURATION_SETTINGS, true))
}

pub(crate) fn segmenter_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(simple_constructor(
        scope,
        this,
        args,
        "Segmenter",
        &SEGMENTER_SETTINGS,
    ))
}

fn containing(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let index = match args.first() {
        Some(index) => scope.to_int32(index).unwrap_or(0),
        None => 0,
    };
    for i in 0..crate::length(scope, this) {
        let Ok(entry) = scope.get_index(this, i) else {
            continue;
        };
        let start = scope.get(&entry, "index");
        let segment = scope.get(&entry, "segment");
        let start = start.and_then(|start| scope.to_int32(&start)).unwrap_or(0);
        let chars = segment
            .and_then(|segment| c_string(scope, &segment))
            .map_or(0, |segment| ffi::utf8_strlen(&segment) as i32);
        if index >= start && index < start.wrapping_add(chars) {
            return Ok(entry);
        }
    }
    Ok(Value::undefined())
}

fn segment(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let input = c_string(scope, &arg(args, 0))?;
    let granularity = hget_str(scope, this, "_granularity");
    let words = granularity.as_deref() == Some(b"word");
    let sentences = granularity.as_deref() == Some(b"sentence");
    let locale = hget_str(scope, this, "_locale");
    let language = locale.as_deref().and_then(ffi::pango_language);
    let input_c = ffi::c_text(&input);
    let chars = ffi::utf8_strlen(&input);
    let mut out = Parts::new(scope);
    if chars > 0 {
        let attrs = ffi::log_attrs(&input_c, chars, language.as_ref());
        let mut start = 0;
        let mut segment_start = 0;
        let mut at = 0;
        for i in 1..=chars {
            at = (at + ffi::utf8_skip(input[at])).min(input.len());
            let boundary = if i == chars {
                true
            } else if words {
                attrs.word_boundary(i)
            } else if sentences {
                attrs.sentence_boundary(i)
            } else {
                attrs.cursor_position(i)
            };
            if !boundary {
                continue;
            }
            let entry = scope.new_object();
            set_text(scope, &entry, "segment", &input[start..at]);
            set(scope, &entry, "index", Value::int(segment_start as i32));
            set_text(scope, &entry, "input", &input);
            if words {
                let word_like = ffi::is_alnum_at(&input_c, start);
                set(scope, &entry, "isWordLike", Value::boolean(word_like));
            }
            out.push_value(scope, entry);
            start = at;
            segment_start = i;
        }
    }
    bind(scope, &out.array, "containing", containing, 1);
    Ok(out.array)
}
