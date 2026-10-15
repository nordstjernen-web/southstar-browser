//! Southstar — Intl.ListFormat: conjunction, disjunction and unit lists joined with CLDR list patterns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use super::service::{self, Iteration, Options, Part};
use crate::{Method, arg};

pub(crate) const METHODS: [Method; 3] = [
    ("format", format, 1),
    ("formatToParts", format_to_parts, 1),
    ("resolvedOptions", resolved_options, 0),
];

const LANGUAGES: [&str; 14] = [
    "da", "de", "en", "es", "fi", "fr", "it", "nb", "nl", "nn", "no", "pl", "pt", "sv",
];

const CONNECTIVES: [(&str, &str, &str); 12] = [
    ("da", " og ", " eller "),
    ("de", " und ", " oder "),
    ("fi", " ja ", " tai "),
    ("fr", " et ", " ou "),
    ("it", " e ", " o "),
    ("nb", " og ", " eller "),
    ("nl", " en ", " of "),
    ("nn", " og ", " eller "),
    ("no", " og ", " eller "),
    ("pl", " i ", " lub "),
    ("pt", " e ", " ou "),
    ("sv", " och ", " eller "),
];

const TYPES: [&str; 3] = ["conjunction", "disjunction", "unit"];
const STYLES: [&str; 3] = ["long", "short", "narrow"];

#[derive(Clone)]
struct ListFormat {
    locale: String,
    kind: &'static str,
    style: &'static str,
}

struct Separators {
    pair: &'static str,
    start: &'static str,
    middle: &'static str,
    end: &'static str,
}

const fn uniform(separator: &'static str) -> Separators {
    Separators {
        pair: separator,
        start: separator,
        middle: separator,
        end: separator,
    }
}

const fn joined_by(word: &'static str, end: &'static str) -> Separators {
    Separators {
        pair: word,
        start: ", ",
        middle: ", ",
        end,
    }
}

fn connective_separators(
    and: &'static str,
    or: &'static str,
    kind: &str,
    style: &str,
) -> Separators {
    match (kind, style) {
        ("conjunction", _) | ("unit", "long") => joined_by(and, and),
        ("disjunction", _) => joined_by(or, or),
        ("unit", "narrow") => uniform(" "),
        _ => uniform(", "),
    }
}

fn separators(language: &str, kind: &str, style: &str) -> Separators {
    if let Some(&(_, and, or)) = CONNECTIVES.iter().find(|&&(known, _, _)| known == language) {
        return connective_separators(and, or, kind, style);
    }
    match (language, kind, style) {
        ("es", "unit", "short") => joined_by(" y ", ", "),
        ("es", _, _) => connective_separators(" y ", " o ", kind, style),
        (_, "unit", "narrow") => uniform(" "),
        (_, "conjunction", "long") => joined_by(" and ", ", and "),
        (_, "conjunction", "short") => joined_by(" & ", ", & "),
        (_, "disjunction", _) => joined_by(" or ", ", or "),
        _ => uniform(", "),
    }
}

fn starts_with_sound(word: &str, sounds: &[&str]) -> bool {
    let lower = word.to_lowercase();
    sounds.iter().any(|sound| lower.starts_with(sound))
}

fn spanish_separator(separator: &'static str, next: &str) -> &'static str {
    let lower = next.to_lowercase();
    let diphthong = ["hia", "hie", "hio", "hiu"];
    match separator {
        " y "
            if starts_with_sound(next, &["i", "í", "hi", "hí"])
                && !starts_with_sound(next, &diphthong) =>
        {
            " e "
        }
        " o "
            if starts_with_sound(next, &["o", "ó", "ho", "hó", "8"])
                || (lower.starts_with("11")
                    && !lower[2..].starts_with(|c: char| c.is_ascii_digit())) =>
        {
            " u "
        }
        _ => separator,
    }
}

fn list_parts(locale: &str, kind: &str, style: &str, items: &[String]) -> Vec<Part> {
    let language = service::language_of(locale);
    let separators = separators(language, kind, style);
    let count = items.len();
    let mut parts = Vec::with_capacity(count * 2);
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            let separator = if count == 2 {
                separators.pair
            } else if index == 1 {
                separators.start
            } else if index == count - 1 {
                separators.end
            } else {
                separators.middle
            };
            let separator = if language == "es" {
                spanish_separator(separator, item)
            } else {
                separator
            };
            parts.push(Part::literal(separator));
        }
        parts.push(Part::new("element", item.clone()));
    }
    parts
}

pub(crate) fn unit_list_parts(locale: &str, style: &str, items: &[String]) -> Vec<Part> {
    list_parts(locale, "unit", style, items)
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::require_new_target(scope, this, "ListFormat")?;
    let prototype = service::prototype_from(scope, this, "ListFormat")?;
    let requested = service::requested_locales(scope, &arg(args, 0))?;
    let options = Options::object(scope, &arg(args, 1))?;
    options.locale_matcher(scope)?;
    let locale = service::resolve(&requested, &LANGUAGES);
    let kind = options.choice_or(scope, "type", &TYPES, "conjunction")?;
    let style = options.choice_or(scope, "style", &STYLES, "long")?;
    let format = ListFormat {
        locale,
        kind,
        style,
    };
    Ok(scope.new_host_object(Some(&prototype), format))
}

pub(crate) fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::supported_locales(scope, args, &LANGUAGES)
}

fn string_list(scope: &mut Scope<'_>, iterable: &Value) -> Result<Vec<String>, Value> {
    if iterable.is_undefined() {
        return Ok(Vec::new());
    }
    let iteration = Iteration::open(scope, iterable)?;
    let mut items = Vec::new();
    while let Some(item) = iteration.step(scope)? {
        if !item.is_string() {
            let error = scope.type_error("Intl.ListFormat lists may only contain strings");
            iteration.close(scope);
            return Err(error);
        }
        items.push(service::text_of(scope, &item)?);
    }
    Ok(items)
}

fn partition(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> Result<Vec<Part>, Value> {
    let format: ListFormat = service::receiver(scope, this, method)?;
    let items = string_list(scope, &arg(args, 0))?;
    Ok(list_parts(
        &format.locale,
        format.kind,
        format.style,
        &items,
    ))
}

fn format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = partition(scope, this, args, "Intl.ListFormat.prototype.format")?;
    Ok(service::joined(scope, &parts))
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = partition(scope, this, args, "Intl.ListFormat.prototype.formatToParts")?;
    Ok(service::parts_array(scope, &parts))
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let format: ListFormat =
        service::receiver(scope, this, "Intl.ListFormat.prototype.resolvedOptions")?;
    let options = scope.new_object();
    service::text_property(scope, &options, "locale", &format.locale);
    service::text_property(scope, &options, "type", format.kind);
    service::text_property(scope, &options, "style", format.style);
    Ok(options)
}
