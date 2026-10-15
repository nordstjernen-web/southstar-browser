//! Southstar — Intl.Collator and String.prototype.localeCompare over the in-tree collation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::cmp::Ordering;

use southstar_js_engine::{Attributes, Scope, Trace, Value};

use crate::collation::{self, Settings, Strength};
use crate::negotiate::{self, Options, create_data_property};
use crate::{arg, lang_subtag};

const TAILORED_LANGUAGES: [&str; 14] = [
    "ar", "bn", "de", "es", "fi", "ja", "kn", "ko", "ln", "si", "sv", "vi", "yue", "zh",
];

const RELEVANT_KEYS: [&str; 3] = ["co", "kf", "kn"];

pub(crate) fn collations_for(language: &str) -> Vec<&'static str> {
    match language {
        "ar" => vec!["compat"],
        "bn" | "es" | "fi" | "kn" | "sv" | "vi" => vec!["trad"],
        "de" => vec!["phonebk"],
        "ja" => vec!["unihan"],
        "ko" => vec!["searchjl", "unihan"],
        "ln" => vec!["phonetic"],
        "si" => vec!["dict"],
        "yue" | "zh" => vec!["pinyin", "stroke", "unihan", "zhuyin"],
        _ => Vec::new(),
    }
}

pub(crate) fn available_collations() -> Vec<&'static str> {
    let mut list: Vec<&str> = TAILORED_LANGUAGES
        .iter()
        .filter(|language| negotiate::is_available(language))
        .flat_map(|language| collations_for(language))
        .chain(["emoji", "eor"])
        .collect();
    list.sort_unstable();
    list.dedup();
    list
}

#[derive(Clone)]
struct Resolved {
    locale: String,
    usage: String,
    sensitivity: String,
    ignore_punctuation: bool,
    collation: String,
    numeric: bool,
    case_first: String,
}

struct CollatorSlots {
    resolved: Resolved,
    bound_compare: RefCell<Option<Value>>,
}

impl Trace for CollatorSlots {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(bound) = self.bound_compare.borrow().as_ref() {
            visit(bound);
        }
    }
}

impl Resolved {
    fn settings(&self) -> Settings {
        let language = lang_subtag(self.locale.as_bytes());
        Settings {
            strength: Strength::from_name(&self.sensitivity),
            ignore_punctuation: self.ignore_punctuation,
            numeric: self.numeric,
            upper_first: self.case_first == "upper",
            umlaut_as_e: language == b"de"
                && (self.collation == "phonebk" || self.usage == "search"),
        }
    }

    fn compare(&self, a: &str, b: &str) -> i32 {
        match collation::compare(a, b, &self.settings()) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }
}

fn locale_data(locale: &str, key: &str) -> Vec<Option<String>> {
    let owned = |values: &[&str]| values.iter().map(|v| Some((*v).to_owned())).collect();
    match key {
        "co" => {
            let language = String::from_utf8_lossy(&lang_subtag(locale.as_bytes())).into_owned();
            let mut values = vec![None];
            values.extend(
                collations_for(&language)
                    .into_iter()
                    .chain(["emoji", "eor"])
                    .map(|v| Some(v.to_owned())),
            );
            values
        }
        "kf" => owned(&["false", "lower", "upper"]),
        "kn" => owned(&["false", "true"]),
        _ => Vec::new(),
    }
}

fn resolve(scope: &mut Scope<'_>, locales: &Value, options: &Value) -> Result<Resolved, Value> {
    let requested = negotiate::canonicalize_locale_list(scope, locales)?;
    let options = Options::coerce(scope, options)?;
    let usage = options
        .string(scope, "usage", &["sort", "search"])?
        .unwrap_or_else(|| "sort".to_owned());
    options.locale_matcher(scope)?;
    let collation = options.string(scope, "collation", &[])?;
    if let Some(collation) = &collation
        && !crate::tag::is_type_value(collation)
    {
        return Err(scope.range_error(&format!("Invalid collation: {collation}")));
    }
    let numeric = options
        .boolean(scope, "numeric")?
        .map(|numeric| numeric.to_string());
    let case_first = options.string(scope, "caseFirst", &["upper", "lower", "false"])?;
    let resolution = negotiate::resolve_locale(
        &requested,
        &RELEVANT_KEYS,
        &[("co", collation), ("kn", numeric), ("kf", case_first)],
        &locale_data,
    );
    let sensitivity = options
        .string(scope, "sensitivity", &["base", "accent", "case", "variant"])?
        .unwrap_or_else(|| "variant".to_owned());
    let default_ignore = lang_subtag(resolution.data_locale.as_bytes()) == b"th";
    let ignore_punctuation = options
        .boolean(scope, "ignorePunctuation")?
        .unwrap_or(default_ignore);
    Ok(Resolved {
        locale: resolution.locale.clone(),
        usage,
        sensitivity,
        ignore_punctuation,
        collation: resolution.value("co").unwrap_or("default").to_owned(),
        numeric: resolution.value("kn") == Some("true"),
        case_first: resolution.value("kf").unwrap_or("false").to_owned(),
    })
}

fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let fallback = arg(data, 0);
    let proto = if this.is_undefined() {
        fallback
    } else {
        negotiate::prototype_from_constructor(scope, this, "Collator", &fallback)?
    };
    let resolved = resolve(scope, &arg(args, 0), &arg(args, 1))?;
    let slots = CollatorSlots {
        resolved,
        bound_compare: RefCell::new(None),
    };
    Ok(scope.new_traced_host_object(Some(&proto), slots))
}

fn resolved_of(scope: &mut Scope<'_>, this: &Value, method: &str) -> Result<Resolved, Value> {
    this.with_host(|slots: &CollatorSlots| slots.resolved.clone())
        .ok_or_else(|| {
            scope.type_error(&format!(
                "Intl.Collator.prototype.{method} called on an incompatible receiver"
            ))
        })
}

fn bound_compare(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let resolved = resolved_of(scope, &arg(data, 0), "compare")?;
    let a = scope.to_string(&arg(args, 0))?;
    let b = scope.to_string(&arg(args, 1))?;
    Ok(Value::int(resolved.compare(&a, &b)))
}

fn compare_getter(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    resolved_of(scope, this, "compare")?;
    let cached = this.with_host(|slots: &CollatorSlots| slots.bound_compare.borrow().clone());
    if let Some(Some(bound)) = cached {
        return Ok(bound);
    }
    let bound = scope.bound_function("", 2, bound_compare, std::slice::from_ref(this));
    this.with_host(|slots: &CollatorSlots| slots.bound_compare.replace(Some(bound.clone())));
    Ok(bound)
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let resolved = resolved_of(scope, this, "resolvedOptions")?;
    let options = scope.new_object();
    let texts = [
        ("locale", &resolved.locale),
        ("usage", &resolved.usage),
        ("sensitivity", &resolved.sensitivity),
    ];
    for (key, value) in texts {
        let value = scope.string(value);
        create_data_property(scope, &options, key, value)?;
    }
    let ignore = Value::boolean(resolved.ignore_punctuation);
    create_data_property(scope, &options, "ignorePunctuation", ignore)?;
    let collation = scope.string(&resolved.collation);
    create_data_property(scope, &options, "collation", collation)?;
    create_data_property(scope, &options, "numeric", Value::boolean(resolved.numeric))?;
    let case_first = scope.string(&resolved.case_first);
    create_data_property(scope, &options, "caseFirst", case_first)?;
    Ok(options)
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let requested = negotiate::canonicalize_locale_list(scope, &arg(args, 0))?;
    negotiate::supported_locales(scope, &requested, &arg(args, 1))
}

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) -> Value {
    let proto = scope.new_object();
    let constructor =
        scope.bound_constructor("Collator", 0, constructor, std::slice::from_ref(&proto));
    let _ = scope.set_constructor(&constructor, &proto);
    crate::bind(
        scope,
        &constructor,
        "supportedLocalesOf",
        supported_locales_of,
        1,
    );
    crate::bind(scope, &proto, "resolvedOptions", resolved_options, 0);
    let getter = scope.function("get compare", 0, compare_getter);
    let _ = scope.define_accessor(
        &proto,
        "compare",
        Some(&getter),
        None,
        Attributes::CONFIGURABLE,
    );
    let _ = scope.define_to_string_tag(&proto, "Intl.Collator");
    let _ = scope.define(intl, "Collator", constructor, Attributes::METHOD);
    proto
}

pub(crate) fn string_locale_compare(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if this.is_undefined() || this.is_null() {
        return Err(scope.type_error("String.prototype.localeCompare called on null or undefined"));
    }
    let a = scope.to_string(this)?;
    let b = scope.to_string(&arg(args, 0))?;
    let resolved = resolve(scope, &arg(args, 1), &arg(args, 2))?;
    Ok(Value::int(resolved.compare(&a, &b)))
}
