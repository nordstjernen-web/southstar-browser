//! Southstar — Intl.PluralRules, ListFormat, RelativeTimeFormat, DisplayNames, DurationFormat and Segmenter.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod display;
mod duration;
mod list;
mod relative;
mod segmenter;
mod service;

use southstar_js_engine::{Scope, Value};

pub(crate) use display::{METHODS as DISPLAY_METHODS, constructor as display_constructor};
pub(crate) use duration::{METHODS as DURATION_METHODS, constructor as duration_constructor};
pub(crate) use list::{METHODS as LIST_METHODS, constructor as list_constructor};
pub(crate) use relative::{METHODS as RELATIVE_METHODS, constructor as relative_constructor};
pub(crate) use segmenter::{METHODS as SEGMENTER_METHODS, constructor as segmenter_constructor};

use crate::{
    Method, Parts, arg, arg_locale, hget_str, hide_text, lang_subtag, new_instance, number_arg,
    opt_str, set, set_text, text,
};

pub(crate) const PLURAL_METHODS: [Method; 3] = [
    ("select", plural_select, 1),
    ("selectRange", plural_select_range, 2),
    ("resolvedOptions", plural_resolved, 0),
];

const NO_CATEGORIES: [&[u8]; 11] = [
    b"ja", b"zh", b"ko", b"th", b"vi", b"id", b"ms", b"lo", b"my", b"km", b"fa",
];
const FRENCH: [&[u8]; 3] = [b"fr", b"ff", b"kab"];
const SLAVIC: [&[u8]; 3] = [b"ru", b"uk", b"be"];
const WEST_SLAVIC: [&[u8]; 2] = [b"cs", b"sk"];
const PLURAL_SAMPLES: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 5.0, 11.0, 21.0, 100.0];

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

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) {
    service::install_supported_locales(scope, intl, "ListFormat", list::supported_locales_of);
    service::install_supported_locales(
        scope,
        intl,
        "RelativeTimeFormat",
        relative::supported_locales_of,
    );
    service::install_supported_locales(scope, intl, "DisplayNames", display::supported_locales_of);
    service::install_supported_locales(
        scope,
        intl,
        "DurationFormat",
        duration::supported_locales_of,
    );
    service::install_supported_locales(scope, intl, "Segmenter", segmenter::supported_locales_of);
    let _ = segmenter::install(scope, intl);
}
