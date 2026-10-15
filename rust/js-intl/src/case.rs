//! Southstar — String.prototype.toLocaleUpperCase and toLocaleLowerCase with the language-sensitive SpecialCasing rules.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, BoundFn, Scope, Value};

use crate::arg;
use crate::negotiate;
use crate::tag::LocaleId;

const DOT_ABOVE: char = '\u{307}';
const ABOVE: u8 = 230;

const SOFT_DOTTED: [(char, char); 18] = [
    ('i', 'j'),
    ('\u{12f}', '\u{12f}'),
    ('\u{249}', '\u{249}'),
    ('\u{268}', '\u{268}'),
    ('\u{29d}', '\u{29d}'),
    ('\u{2b2}', '\u{2b2}'),
    ('\u{3f3}', '\u{3f3}'),
    ('\u{456}', '\u{456}'),
    ('\u{458}', '\u{458}'),
    ('\u{1d62}', '\u{1d62}'),
    ('\u{1d96}', '\u{1d96}'),
    ('\u{1da4}', '\u{1da4}'),
    ('\u{1da8}', '\u{1da8}'),
    ('\u{1e2d}', '\u{1e2d}'),
    ('\u{1ecb}', '\u{1ecb}'),
    ('\u{2071}', '\u{2071}'),
    ('\u{2148}', '\u{2149}'),
    ('\u{2c7c}', '\u{2c7c}'),
];

const MATH_SMALL_I: char = '\u{1d422}';
const MATH_SMALL_J_LAST: char = '\u{1d693}';
const MATH_ALPHABET_STRIDE: u32 = 52;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Turkic,
    Lithuanian,
    Other,
}

fn class(c: char) -> u8 {
    southstar_url::combining_class(c)
}

fn is_soft_dotted(c: char) -> bool {
    let math_letter = (MATH_SMALL_I..=MATH_SMALL_J_LAST).contains(&c)
        && (c as u32 - MATH_SMALL_I as u32) % MATH_ALPHABET_STRIDE < 2;
    math_letter
        || SOFT_DOTTED
            .iter()
            .any(|&(first, last)| (first..=last).contains(&c))
}

fn more_above(chars: &[char], at: usize) -> bool {
    for &c in &chars[at + 1..] {
        match class(c) {
            ABOVE => return true,
            0 => return false,
            _ => {}
        }
    }
    false
}

fn followed_by_dot_above(chars: &[char], at: usize) -> bool {
    for &c in &chars[at + 1..] {
        if c == DOT_ABOVE {
            return true;
        }
        if matches!(class(c), 0 | ABOVE) {
            return false;
        }
    }
    false
}

fn preceding_base(chars: &[char], at: usize) -> Option<char> {
    chars[..at]
        .iter()
        .rev()
        .find(|&&c| matches!(class(c), 0 | ABOVE))
        .copied()
}

fn turkic_lower(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        match c {
            '\u{130}' => out.push('i'),
            'I' if followed_by_dot_above(chars, at) => out.push('i'),
            'I' => out.push('\u{131}'),
            DOT_ABOVE if preceding_base(chars, at) == Some('I') => {}
            _ => out.push(c),
        }
    }
    out.to_lowercase()
}

fn lithuanian_lower(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        match c {
            'I' | 'J' | '\u{12e}' if more_above(chars, at) => {
                out.extend(c.to_lowercase());
                out.push(DOT_ABOVE);
            }
            '\u{cc}' => out.push_str("i\u{307}\u{300}"),
            '\u{cd}' => out.push_str("i\u{307}\u{301}"),
            '\u{128}' => out.push_str("i\u{307}\u{303}"),
            _ => out.push(c),
        }
    }
    out.to_lowercase()
}

fn turkic_upper(chars: &[char]) -> String {
    let replaced: String = chars
        .iter()
        .map(|&c| if c == 'i' { '\u{130}' } else { c })
        .collect();
    replaced.to_uppercase()
}

fn lithuanian_upper(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        if c == DOT_ABOVE && preceding_base(chars, at).is_some_and(is_soft_dotted) {
            continue;
        }
        out.push(c);
    }
    out.to_uppercase()
}

fn case_language(scope: &mut Scope<'_>, locales: &Value) -> Result<Language, Value> {
    let requested = negotiate::canonicalize_locale_list(scope, locales)?;
    let locale = requested
        .first()
        .cloned()
        .unwrap_or_else(|| negotiate::default_locale().to_owned());
    let language = LocaleId::parse(&locale).map(|parsed| parsed.id.language);
    Ok(match language.as_deref() {
        Some("tr" | "az") => Language::Turkic,
        Some("lt") => Language::Lithuanian,
        _ => Language::Other,
    })
}

fn transform(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
    upper: bool,
) -> Result<Value, Value> {
    if this.is_undefined() || this.is_null() {
        return Err(
            scope.type_error("String.prototype case conversion called on null or undefined")
        );
    }
    let text = scope.to_string_value(this)?;
    let language = case_language(scope, &arg(args, 0))?;
    if language == Language::Other {
        return scope.call(&arg(data, 0), &text, &[]);
    }
    let chars: Vec<char> = scope.to_string(&text)?.chars().collect();
    let converted = match (language, upper) {
        (Language::Turkic, true) => turkic_upper(&chars),
        (Language::Turkic, false) => turkic_lower(&chars),
        (_, true) => lithuanian_upper(&chars),
        (_, false) => lithuanian_lower(&chars),
    };
    Ok(scope.string(&converted))
}

fn to_locale_upper_case(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    transform(scope, this, args, data, true)
}

fn to_locale_lower_case(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    transform(scope, this, args, data, false)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let Ok(string) = scope.get(global, "String") else {
        return;
    };
    let Ok(proto) = scope.get(&string, "prototype") else {
        return;
    };
    let hooks: [(&str, &str, BoundFn); 2] = [
        ("toLocaleUpperCase", "toUpperCase", to_locale_upper_case),
        ("toLocaleLowerCase", "toLowerCase", to_locale_lower_case),
    ];
    for (name, plain, f) in hooks {
        let Ok(original) = scope.get(&proto, plain) else {
            continue;
        };
        let function = scope.bound_function(name, 0, f, &[original]);
        let _ = scope.define(&proto, name, function, Attributes::METHOD);
    }
}
