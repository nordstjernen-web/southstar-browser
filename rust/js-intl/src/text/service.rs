//! Southstar — the ECMA-402 operations the text services share: construction, options, locale negotiation and result objects.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::any::Any;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::{arg, locale};

const LOCALE_MATCHERS: [&str; 2] = ["lookup", "best fit"];

pub(crate) const COMMON_LANGUAGES: &[&str] = &[
    "af", "am", "ar", "az", "be", "bg", "bn", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es",
    "et", "eu", "fa", "fi", "fil", "fr", "ga", "gl", "gu", "he", "hi", "hr", "hu", "hy", "id",
    "is", "it", "ja", "ka", "kk", "km", "kn", "ko", "ky", "lo", "lt", "lv", "mk", "ml", "mn", "mr",
    "ms", "my", "nb", "ne", "nl", "nn", "no", "pa", "pl", "ps", "pt", "ro", "ru", "si", "sk", "sl",
    "sq", "sr", "sv", "sw", "ta", "te", "th", "tr", "uk", "ur", "uz", "vi", "yue", "zh", "zu",
];

pub(crate) fn text_of(scope: &mut Scope<'_>, value: &Value) -> Result<String, Value> {
    let bytes = scope.to_bytes(value)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn require_new_target(
    scope: &mut Scope<'_>,
    new_target: &Value,
    service: &str,
) -> Result<(), Value> {
    if scope.is_constructor(new_target) {
        Ok(())
    } else {
        Err(scope.type_error(&format!("Constructor Intl.{service} requires 'new'")))
    }
}

pub(crate) fn prototype_from(
    scope: &mut Scope<'_>,
    new_target: &Value,
    service: &str,
) -> Result<Value, Value> {
    let prototype = scope.get(new_target, "prototype")?;
    if prototype.is_object() {
        return Ok(prototype);
    }
    let realm_global = scope
        .function_handle(new_target)
        .ok()
        .and_then(|realm| realm.with(|scope| scope.global()));
    let global = realm_global.unwrap_or_else(|| scope.global());
    let intl = scope.get(&global, "Intl")?;
    let constructor = scope.get(&intl, service)?;
    scope.get(&constructor, "prototype")
}

pub(crate) fn receiver<T: Any + Clone>(
    scope: &mut Scope<'_>,
    this: &Value,
    method: &str,
) -> Result<T, Value> {
    match scope.host_data::<T>(this) {
        Some(data) => Ok(data),
        None => Err(scope.type_error(&format!("{method} called on an incompatible receiver"))),
    }
}

pub(crate) struct Options(Option<Value>);

impl Options {
    pub(crate) fn object(scope: &mut Scope<'_>, options: &Value) -> Result<Options, Value> {
        if options.is_undefined() {
            Ok(Options(None))
        } else if options.is_object() {
            Ok(Options(Some(options.clone())))
        } else {
            Err(scope.type_error("Options must be an object"))
        }
    }

    pub(crate) fn coerced(scope: &mut Scope<'_>, options: &Value) -> Result<Options, Value> {
        if options.is_undefined() {
            return Ok(Options(None));
        }
        scope.to_object(options).map(|object| Options(Some(object)))
    }

    fn get(&self, scope: &mut Scope<'_>, key: &str) -> Result<Value, Value> {
        match &self.0 {
            Some(object) => scope.get(object, key),
            None => Ok(Value::undefined()),
        }
    }

    pub(crate) fn string(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        allowed: &[&'static str],
    ) -> Result<Option<String>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        let text = text_of(scope, &value)?;
        if !allowed.is_empty() && !allowed.contains(&text.as_str()) {
            return Err(scope.range_error(&format!("Value {text} out of range for option {key}")));
        }
        Ok(Some(text))
    }

    pub(crate) fn choice(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        allowed: &[&'static str],
    ) -> Result<Option<&'static str>, Value> {
        let text = self.string(scope, key, allowed)?;
        Ok(text.and_then(|text| allowed.iter().copied().find(|&known| known == text)))
    }

    pub(crate) fn choice_or(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        allowed: &[&'static str],
        default: &'static str,
    ) -> Result<&'static str, Value> {
        Ok(self.choice(scope, key, allowed)?.unwrap_or(default))
    }

    pub(crate) fn locale_matcher(&self, scope: &mut Scope<'_>) -> Result<(), Value> {
        self.choice(scope, "localeMatcher", &LOCALE_MATCHERS)
            .map(|_| ())
    }

    pub(crate) fn numbering_system(&self, scope: &mut Scope<'_>) -> Result<Option<String>, Value> {
        let value = self.string(scope, "numberingSystem", &[])?;
        match value {
            Some(value) if !is_unicode_type(&value) => {
                Err(scope.range_error(&format!("Invalid numberingSystem {value}")))
            }
            value => Ok(value),
        }
    }

    pub(crate) fn integer(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        range: core::ops::RangeInclusive<f64>,
    ) -> Result<Option<u32>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        let number = scope.to_number(&value)?;
        if !range.contains(&number) {
            return Err(scope.range_error(&format!("Value of {key} is out of range")));
        }
        Ok(Some(number.floor() as u32))
    }
}

pub(crate) fn is_unicode_type(value: &str) -> bool {
    value.split('-').all(|part| {
        (3..=8).contains(&part.len()) && part.bytes().all(|c| c.is_ascii_alphanumeric())
    })
}

pub(crate) fn requested_locales(
    scope: &mut Scope<'_>,
    locales: &Value,
) -> Result<Vec<String>, Value> {
    let list =
        locale::get_canonical_locales(scope, &Value::undefined(), core::slice::from_ref(locales))?;
    let count = crate::length(scope, &list);
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let entry = scope.get_index(&list, index)?;
        out.push(text_of(scope, &entry)?);
    }
    Ok(out)
}

fn default_locale() -> String {
    crate::negotiate::default_locale().to_owned()
}

pub(crate) fn language_of(locale: &str) -> &str {
    locale.split('-').next().unwrap_or(locale)
}

fn is_script(subtag: &str) -> bool {
    subtag.len() == 4 && subtag.bytes().all(|c| c.is_ascii_alphabetic())
}

fn is_region(subtag: &str) -> bool {
    (subtag.len() == 2 && subtag.bytes().all(|c| c.is_ascii_alphabetic()))
        || (subtag.len() == 3 && subtag.bytes().all(|c| c.is_ascii_digit()))
}

fn is_available(languages: &[&str], default_language: &str, tag: &str) -> bool {
    let mut subtags = tag.split('-');
    let language = subtags.next().unwrap_or_default();
    let known = languages.contains(&language) || language == default_language;
    let rest: Vec<&str> = subtags.collect();
    known
        && match rest.as_slice() {
            [] => true,
            [only] => is_script(only) || is_region(only),
            [script, region] => is_script(script) && is_region(region),
            _ => false,
        }
}

fn best_available(languages: &[&str], default_language: &str, locale: &str) -> Option<String> {
    let mut candidate = locale.to_owned();
    loop {
        if is_available(languages, default_language, &candidate) {
            return Some(candidate);
        }
        let mut position = candidate.rfind('-')?;
        if position >= 2 && candidate.as_bytes()[position - 2] == b'-' {
            position -= 2;
        }
        candidate.truncate(position);
    }
}

fn split_unicode_extension(tag: &str) -> (String, Option<String>) {
    let subtags: Vec<&str> = tag.split('-').collect();
    let mut base: Vec<&str> = Vec::with_capacity(subtags.len());
    let mut extension = None;
    let mut index = 0;
    while index < subtags.len() {
        let subtag = subtags[index];
        if index == 0 || subtag.len() != 1 {
            base.push(subtag);
            index += 1;
            continue;
        }
        if subtag.eq_ignore_ascii_case("x") {
            base.extend_from_slice(&subtags[index..]);
            break;
        }
        let end = (index + 1..subtags.len())
            .find(|&next| subtags[next].len() == 1)
            .unwrap_or(subtags.len());
        if !subtag.eq_ignore_ascii_case("u") {
            base.extend_from_slice(&subtags[index..end]);
        } else if extension.is_none() {
            extension = Some(subtags[index..end].join("-"));
        }
        index = end;
    }
    (base.join("-"), extension)
}

fn keyword_value(extension: &str, key: &str) -> Option<String> {
    let subtags: Vec<&str> = extension.split('-').skip(1).collect();
    let mut index = 0;
    while index < subtags.len() {
        if subtags[index].len() != 2 {
            index += 1;
            continue;
        }
        let end = (index + 1..subtags.len())
            .find(|&next| subtags[next].len() == 2)
            .unwrap_or(subtags.len());
        if subtags[index].eq_ignore_ascii_case(key) {
            return Some(subtags[index + 1..end].join("-").to_ascii_lowercase());
        }
        index = end;
    }
    None
}

fn lookup(requested: &[String], languages: &[&str]) -> (String, Option<String>) {
    let default = default_locale();
    for locale in requested {
        let (base, extension) = split_unicode_extension(locale);
        if let Some(found) = best_available(languages, language_of(&default), &base) {
            return (found, extension);
        }
    }
    (default, None)
}

pub(crate) fn resolve(requested: &[String], languages: &[&str]) -> String {
    lookup(requested, languages).0
}

pub(crate) fn supported_values(scope: &mut Scope<'_>, key: &str) -> Vec<String> {
    let key = scope.string(key);
    let Ok(list) = locale::supported_values_of(scope, &Value::undefined(), &[key]) else {
        return Vec::new();
    };
    let count = crate::length(scope, &list);
    let mut values = Vec::with_capacity(count as usize);
    for index in 0..count {
        if let Ok(value) = scope.get_index(&list, index)
            && let Ok(value) = text_of(scope, &value)
        {
            values.push(value);
        }
    }
    values
}

pub(crate) fn resolve_numbering(
    scope: &mut Scope<'_>,
    requested: &[String],
    languages: &[&str],
    option: Option<&str>,
) -> (String, String) {
    let supported = supported_values(scope, "numberingSystem");
    let is_supported = |value: &str| supported.iter().any(|known| known == value);
    let (found, extension) = lookup(requested, languages);
    let mut numbering = String::from("latn");
    let mut keyword = None;
    if let Some(value) = extension.and_then(|extension| keyword_value(&extension, "nu"))
        && is_supported(&value)
    {
        numbering.clone_from(&value);
        keyword = Some(value);
    }
    if let Some(value) = option.map(str::to_ascii_lowercase)
        && is_supported(&value)
        && value != numbering
    {
        numbering = value;
        keyword = None;
    }
    let locale = match keyword {
        Some(value) => format!("{found}-u-nu-{value}"),
        None => found,
    };
    (locale, numbering)
}

pub(crate) fn supported_locales(
    scope: &mut Scope<'_>,
    args: &[Value],
    languages: &[&str],
) -> Result<Value, Value> {
    let requested = requested_locales(scope, &arg(args, 0))?;
    let options = Options::coerced(scope, &arg(args, 1))?;
    options.locale_matcher(scope)?;
    let default = default_locale();
    let mut supported = Vec::new();
    for locale in requested {
        let base = split_unicode_extension(&locale).0;
        if best_available(languages, language_of(&default), &base).is_some() {
            supported.push(scope.string(&locale));
        }
    }
    Ok(array_of(scope, supported))
}

pub(crate) fn install_supported_locales(
    scope: &mut Scope<'_>,
    intl: &Value,
    service: &str,
    f: NativeFn,
) {
    if let Ok(constructor) = scope.get(intl, service)
        && constructor.is_object()
    {
        let function = scope.function("supportedLocalesOf", 1, f);
        let _ = scope.define(
            &constructor,
            "supportedLocalesOf",
            function,
            Attributes::METHOD,
        );
    }
}

pub(crate) fn data_property(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let key = scope.string(key);
    let _ = scope.define_entry(object, &key, value);
}

pub(crate) fn text_property(scope: &mut Scope<'_>, object: &Value, key: &str, value: &str) {
    let value = scope.string(value);
    data_property(scope, object, key, value);
}

pub(crate) fn array_of(scope: &mut Scope<'_>, values: Vec<Value>) -> Value {
    let array = scope.new_array();
    for (index, value) in values.into_iter().enumerate() {
        let _ = scope.define_entry(&array, &Value::int(index as i32), value);
    }
    array
}

pub(crate) fn plain_object(scope: &mut Scope<'_>, entries: &[(&str, Value)]) -> Value {
    let object = scope.new_object_with_proto(&Value::null());
    for (key, value) in entries {
        data_property(scope, &object, key, value.clone());
    }
    object
}

pub(crate) fn iter_result(scope: &mut Scope<'_>, value: Value, done: bool) -> Value {
    let result = scope.new_object();
    data_property(scope, &result, "value", value);
    data_property(scope, &result, "done", Value::boolean(done));
    result
}

#[derive(Clone)]
pub(crate) struct Part {
    pub(crate) kind: String,
    pub(crate) value: String,
    pub(crate) unit: Option<&'static str>,
}

impl Part {
    pub(crate) fn new(kind: &str, value: impl Into<String>) -> Part {
        Part {
            kind: kind.to_owned(),
            value: value.into(),
            unit: None,
        }
    }

    pub(crate) fn literal(value: impl Into<String>) -> Part {
        Part::new("literal", value)
    }

    pub(crate) fn with_unit(mut self, unit: &'static str) -> Part {
        self.unit = Some(unit);
        self
    }
}

pub(crate) fn parts_array(scope: &mut Scope<'_>, parts: &[Part]) -> Value {
    let mut values = Vec::with_capacity(parts.len());
    for part in parts {
        let object = scope.new_object();
        text_property(scope, &object, "type", &part.kind);
        text_property(scope, &object, "value", &part.value);
        if let Some(unit) = part.unit {
            text_property(scope, &object, "unit", unit);
        }
        values.push(object);
    }
    array_of(scope, values)
}

pub(crate) fn joined(scope: &mut Scope<'_>, parts: &[Part]) -> Value {
    let text: String = parts.iter().map(|part| part.value.as_str()).collect();
    scope.string(&text)
}

pub(crate) fn construct_intl(
    scope: &mut Scope<'_>,
    service: &str,
    args: &[Value],
) -> Result<Value, Value> {
    let constructor = crate::global_constructor(scope, service)?;
    scope.construct(&constructor, args)
}

pub(crate) fn number_parts(
    scope: &mut Scope<'_>,
    format: &Value,
    value: &Value,
) -> Result<Vec<Part>, Value> {
    let method = scope.get(format, "formatToParts")?;
    let parts = scope.call(&method, format, core::slice::from_ref(value))?;
    let count = crate::length(scope, &parts);
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let part = scope.get_index(&parts, index)?;
        let kind = scope.get(&part, "type")?;
        let value = scope.get(&part, "value")?;
        let kind = text_of(scope, &kind)?;
        out.push(Part::new(&kind, text_of(scope, &value)?));
    }
    Ok(out)
}

pub(crate) fn well_known_symbol(scope: &mut Scope<'_>, name: &str) -> Result<Value, Value> {
    let global = scope.global();
    let symbol = scope.get(&global, "Symbol")?;
    scope.get(&symbol, name)
}

pub(crate) struct Iteration {
    iterator: Value,
    next: Value,
}

impl Iteration {
    pub(crate) fn open(scope: &mut Scope<'_>, iterable: &Value) -> Result<Iteration, Value> {
        let symbol = well_known_symbol(scope, "iterator")?;
        let method = scope.get_key(iterable, &symbol)?;
        if !scope.is_function(&method) {
            return Err(scope.type_error("Value is not iterable"));
        }
        let iterator = scope.call(&method, iterable, &[])?;
        if !iterator.is_object() {
            return Err(scope.type_error("Result of the Symbol.iterator method is not an object"));
        }
        let next = scope.get(&iterator, "next")?;
        Ok(Iteration { iterator, next })
    }

    pub(crate) fn step(&self, scope: &mut Scope<'_>) -> Result<Option<Value>, Value> {
        let result = scope.call(&self.next, &self.iterator, &[])?;
        if !result.is_object() {
            return Err(scope.type_error("Iterator result is not an object"));
        }
        let done = scope.get(&result, "done")?;
        if scope.to_bool(&done) {
            return Ok(None);
        }
        scope.get(&result, "value").map(Some)
    }

    pub(crate) fn close(&self, scope: &mut Scope<'_>) {
        if let Ok(method) = scope.get(&self.iterator, "return")
            && !method.is_undefined()
            && !method.is_null()
        {
            let _ = scope.call(&method, &self.iterator, &[]);
        }
    }
}
