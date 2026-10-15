//! Southstar — the ECMA-402 locale and option abstract operations every Intl constructor shares.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

use crate::cldr_index;
use crate::tag::{self, LanguageId, LocaleId};

pub(crate) struct Options(Option<Value>);

impl Options {
    pub(crate) fn coerce(scope: &mut Scope<'_>, options: &Value) -> Result<Options, Value> {
        if options.is_undefined() {
            return Ok(Options(None));
        }
        scope.to_object(options).map(|object| Options(Some(object)))
    }

    pub(crate) fn get(&self, scope: &mut Scope<'_>, key: &str) -> Result<Value, Value> {
        match &self.0 {
            Some(object) => scope.get(object, key),
            None => Ok(Value::undefined()),
        }
    }

    pub(crate) fn string(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        allowed: &[&str],
    ) -> Result<Option<String>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        let text = scope.to_string(&value)?;
        if !allowed.is_empty() && !allowed.contains(&text.as_str()) {
            return Err(scope.range_error(&format!("Value {text} out of range for option {key}")));
        }
        Ok(Some(text))
    }

    pub(crate) fn boolean(&self, scope: &mut Scope<'_>, key: &str) -> Result<Option<bool>, Value> {
        let value = self.get(scope, key)?;
        Ok((!value.is_undefined()).then(|| scope.to_bool(&value)))
    }

    pub(crate) fn locale_matcher(&self, scope: &mut Scope<'_>) -> Result<String, Value> {
        let matcher = self.string(scope, "localeMatcher", &["lookup", "best fit"])?;
        Ok(matcher.unwrap_or_else(|| "best fit".to_owned()))
    }
}

pub(crate) fn locale_slot(value: &Value) -> Option<String> {
    value.with_host(|slots: &crate::locale::LocaleSlots| slots.locale.clone())
}

fn to_length(scope: &mut Scope<'_>, value: &Value) -> Result<u64, Value> {
    let number = scope.to_number(value)?;
    if number.is_nan() || number <= 0.0 {
        return Ok(0);
    }
    Ok(number.min(9_007_199_254_740_991.0).trunc() as u64)
}

pub(crate) fn canonicalize_locale_list(
    scope: &mut Scope<'_>,
    locales: &Value,
) -> Result<Vec<String>, Value> {
    let mut seen: Vec<String> = Vec::new();
    if locales.is_undefined() {
        return Ok(seen);
    }
    if locales.is_string() || locale_slot(locales).is_some() {
        add_canonical(scope, &mut seen, locales)?;
        return Ok(seen);
    }
    let list = scope.to_object(locales)?;
    let length = scope.get(&list, "length")?;
    let length = to_length(scope, &length)?;
    let mut index = 0u64;
    while index < length {
        let key = index.to_string();
        index += 1;
        if !scope.has_property(&list, &key)? {
            continue;
        }
        let value = scope.get(&list, &key)?;
        add_canonical(scope, &mut seen, &value)?;
    }
    Ok(seen)
}

fn add_canonical(
    scope: &mut Scope<'_>,
    seen: &mut Vec<String>,
    value: &Value,
) -> Result<(), Value> {
    if !value.is_string() && !value.is_object() {
        return Err(scope.type_error("Locale list elements must be strings or objects"));
    }
    let tag = match locale_slot(value) {
        Some(tag) => tag,
        None => scope.to_string(value)?,
    };
    let Some(canonical) = tag::canonicalize_tag(&tag) else {
        return Err(scope.range_error(&format!("Incorrect locale information provided: {tag}")));
    };
    if !seen.contains(&canonical) {
        seen.push(canonical);
    }
    Ok(())
}

pub(crate) fn is_available(locale: &str) -> bool {
    let Some(id) = LanguageId::parse(locale) else {
        return false;
    };
    if !id.variants.is_empty() || !cldr_index::is_available_language(&id.language) {
        return false;
    }
    match &id.script {
        Some(script) => cldr_index::is_available_script(&format!("{}-{script}", id.language)),
        None => true,
    }
}

pub(crate) fn best_available_locale(locale: &str) -> Option<String> {
    let mut candidate = locale.to_owned();
    loop {
        if is_available(&candidate) {
            return Some(candidate);
        }
        let mut cut = candidate.rfind('-')?;
        if cut >= 2 && candidate.as_bytes()[cut - 2] == b'-' {
            cut -= 2;
        }
        candidate.truncate(cut);
    }
}

fn system_locale() -> Option<String> {
    let from_i18n =
        southstar_i18n::language().map(|tag| String::from_utf8_lossy(tag.to_bytes()).into_owned());
    let names = crate::ffi::language_names()
        .into_iter()
        .map(|name| String::from_utf8_lossy(&name).into_owned());
    from_i18n.into_iter().chain(names).find_map(|name| {
        let name = name
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-");
        let canonical = tag::canonicalize_tag(&name)?;
        let base = LocaleId::parse(&canonical)?.id.to_text();
        best_available_locale(&base)
    })
}

pub(crate) fn default_locale() -> &'static str {
    static DEFAULT: OnceLock<String> = OnceLock::new();
    DEFAULT.get_or_init(|| system_locale().unwrap_or_else(|| "en-US".to_owned()))
}

pub(crate) struct Match {
    pub(crate) locale: String,
    pub(crate) extension: Option<LocaleId>,
}

pub(crate) fn lookup_matcher(requested: &[String]) -> Match {
    for locale in requested {
        let Some(parsed) = LocaleId::parse(locale) else {
            continue;
        };
        let plain = parsed.without_unicode_extension().to_text();
        if let Some(available) = best_available_locale(&plain) {
            let extension = parsed.has_unicode_extension().then_some(parsed);
            return Match {
                locale: available,
                extension,
            };
        }
    }
    Match {
        locale: default_locale().to_owned(),
        extension: None,
    }
}

pub(crate) struct Resolution {
    pub(crate) locale: String,
    pub(crate) data_locale: String,
    values: Vec<(&'static str, Option<String>)>,
}

impl Resolution {
    pub(crate) fn value(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, value)| value.as_deref())
    }
}

pub(crate) type LocaleData<'a> = &'a dyn Fn(&str, &str) -> Vec<Option<String>>;

pub(crate) fn resolve_locale(
    requested: &[String],
    relevant_keys: &[&'static str],
    options: &[(&str, Option<String>)],
    locale_data: LocaleData<'_>,
) -> Resolution {
    let found = lookup_matcher(requested);
    let mut values = Vec::new();
    let mut supported_keywords: Vec<(String, String)> = Vec::new();
    for &key in relevant_keys {
        let key_data = locale_data(&found.locale, key);
        let mut value = key_data.first().cloned().flatten();
        let mut supported: Option<(String, String)> = None;
        let requested_value = found
            .extension
            .as_ref()
            .and_then(|extension| extension.keyword(key));
        if let Some(requested_value) = requested_value {
            if !requested_value.is_empty() {
                if key_data.contains(&Some(requested_value.to_owned())) {
                    value = Some(requested_value.to_owned());
                    supported = Some((key.to_owned(), requested_value.to_owned()));
                }
            } else if key_data.contains(&Some("true".to_owned())) {
                value = Some("true".to_owned());
                supported = Some((key.to_owned(), String::new()));
            }
        }
        let option = options
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, option)| option.as_deref());
        if let Some(option) = option {
            let mut canonical = tag::canonical_unicode_value(key, option);
            if canonical.is_empty() {
                canonical = "true".to_owned();
            }
            if value.as_deref() != Some(canonical.as_str())
                && key_data.contains(&Some(canonical.clone()))
            {
                value = Some(canonical);
                supported = None;
            }
        }
        supported_keywords.extend(supported);
        values.push((key, value));
    }
    let locale = if supported_keywords.is_empty() {
        found.locale.clone()
    } else {
        insert_unicode_extension(&found.locale, supported_keywords)
    };
    Resolution {
        locale,
        data_locale: found.locale,
        values,
    }
}

pub(crate) fn insert_unicode_extension(locale: &str, keywords: Vec<(String, String)>) -> String {
    let Some(mut parsed) = LocaleId::parse(locale) else {
        return locale.to_owned();
    };
    parsed.keywords = keywords;
    parsed.canonicalize();
    parsed.to_text()
}

pub(crate) fn supported_locales(
    scope: &mut Scope<'_>,
    requested: &[String],
    options: &Value,
) -> Result<Value, Value> {
    let options = Options::coerce(scope, options)?;
    options.locale_matcher(scope)?;
    let array = scope.new_array();
    let mut count = 0;
    for locale in requested {
        let plain = LocaleId::parse(locale)
            .map(|parsed| parsed.without_unicode_extension().to_text())
            .unwrap_or_default();
        if best_available_locale(&plain).is_some() {
            let value = scope.string(locale);
            scope.define_entry(&array, &Value::int(count), value)?;
            count += 1;
        }
    }
    Ok(array)
}

pub(crate) fn string_array(scope: &mut Scope<'_>, items: &[&str]) -> Result<Value, Value> {
    let array = scope.new_array();
    for (index, item) in items.iter().enumerate() {
        let value = scope.string(item);
        scope.define_entry(&array, &Value::int(index as i32), value)?;
    }
    Ok(array)
}

pub(crate) fn create_data_property(
    scope: &mut Scope<'_>,
    object: &Value,
    key: &str,
    value: Value,
) -> Result<(), Value> {
    let key = scope.string(key);
    scope.define_entry(object, &key, value)
}

fn realm_prototype(scope: &mut Scope<'_>, new_target: &Value, service: &str) -> Option<Value> {
    let global = scope.function_realm_global(new_target).ok()?;
    let intl = scope.get(&global, "Intl").ok()?;
    let constructor = scope.get(&intl, service).ok()?;
    scope
        .get(&constructor, "prototype")
        .ok()
        .filter(Value::is_object)
}

pub(crate) fn prototype_from_constructor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    service: &str,
    fallback: &Value,
) -> Result<Value, Value> {
    let proto = scope.get(new_target, "prototype")?;
    if proto.is_object() {
        return Ok(proto);
    }
    Ok(realm_prototype(scope, new_target, service).unwrap_or_else(|| fallback.clone()))
}
