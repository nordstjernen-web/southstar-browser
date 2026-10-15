//! Southstar — the ECMA-402 abstract operations Intl's constructors share: options, locale lists, locale negotiation and prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

const MAX_SAFE_LENGTH: f64 = 9_007_199_254_740_991.0;

pub(crate) struct OptionsBag(Option<Value>);

impl OptionsBag {
    pub(crate) fn coerce(scope: &mut Scope<'_>, options: &Value) -> Result<OptionsBag, Value> {
        if options.is_undefined() {
            return Ok(OptionsBag(None));
        }
        scope
            .to_object(options)
            .map(|object| OptionsBag(Some(object)))
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
        allowed: &[&'static str],
        fallback: &'static str,
    ) -> Result<&'static str, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(fallback);
        }
        let text = scope.to_string(&value)?;
        allowed
            .iter()
            .find(|candidate| **candidate == text)
            .copied()
            .ok_or_else(|| scope.range_error(&format!("{text} is not a valid value for {key}")))
    }

    pub(crate) fn free_string(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
    ) -> Result<Option<String>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        scope.to_string(&value).map(Some)
    }

    pub(crate) fn number(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        minimum: u32,
        maximum: u32,
        fallback: u32,
    ) -> Result<u32, Value> {
        let value = self.get(scope, key)?;
        let number = default_number_option(scope, &value, minimum, maximum, key)?;
        Ok(number.unwrap_or(fallback))
    }
}

pub(crate) fn default_number_option(
    scope: &mut Scope<'_>,
    value: &Value,
    minimum: u32,
    maximum: u32,
    key: &str,
) -> Result<Option<u32>, Value> {
    if value.is_undefined() {
        return Ok(None);
    }
    let number = scope.to_number(value)?;
    if !number.is_finite() || number < f64::from(minimum) || number > f64::from(maximum) {
        return Err(scope.range_error(&format!("{key} is out of range")));
    }
    Ok(Some(number.floor() as u32))
}

fn to_length(scope: &mut Scope<'_>, value: &Value) -> Result<u64, Value> {
    let number = scope.to_number(value)?;
    if number.is_nan() || number <= 0.0 {
        return Ok(0);
    }
    Ok(number.floor().min(MAX_SAFE_LENGTH) as u64)
}

pub(crate) fn canonicalize_locale_list(
    scope: &mut Scope<'_>,
    locales: &Value,
) -> Result<Vec<String>, Value> {
    let mut seen: Vec<String> = Vec::new();
    if locales.is_undefined() {
        return Ok(seen);
    }
    if locales.is_string() {
        let tag = scope.to_string(locales)?;
        add_canonical_tag(scope, &mut seen, &tag)?;
        return Ok(seen);
    }
    let object = scope.to_object(locales)?;
    let length = scope.get(&object, "length")?;
    let length = to_length(scope, &length)?;
    for index in 0..length {
        let key = index.to_string();
        if !scope.has_property(&object, &key)? {
            continue;
        }
        let value = scope.get(&object, &key)?;
        if !value.is_string() && !value.is_object() {
            return Err(scope.type_error("locale list entries must be strings or objects"));
        }
        let tag = scope.to_string(&value)?;
        add_canonical_tag(scope, &mut seen, &tag)?;
    }
    Ok(seen)
}

fn add_canonical_tag(
    scope: &mut Scope<'_>,
    seen: &mut Vec<String>,
    tag: &str,
) -> Result<(), Value> {
    if !is_structurally_valid_language_tag(tag) {
        return Err(scope.range_error(&format!("{tag} is not a valid language tag")));
    }
    let canonical = canonicalize_language_tag(tag);
    if !seen.contains(&canonical) {
        seen.push(canonical);
    }
    Ok(())
}

fn is_alpha(subtag: &str) -> bool {
    subtag.bytes().all(|c| c.is_ascii_alphabetic())
}

fn is_digit(subtag: &str) -> bool {
    subtag.bytes().all(|c| c.is_ascii_digit())
}

fn is_alphanum(subtag: &str) -> bool {
    subtag.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn is_language_subtag(subtag: &str) -> bool {
    is_alpha(subtag) && matches!(subtag.len(), 2 | 3 | 5..=8)
}

fn is_script_subtag(subtag: &str) -> bool {
    subtag.len() == 4 && is_alpha(subtag)
}

fn is_region_subtag(subtag: &str) -> bool {
    (subtag.len() == 2 && is_alpha(subtag)) || (subtag.len() == 3 && is_digit(subtag))
}

fn is_variant_subtag(subtag: &str) -> bool {
    is_alphanum(subtag)
        && ((5..=8).contains(&subtag.len())
            || (subtag.len() == 4 && subtag.as_bytes()[0].is_ascii_digit()))
}

fn sized_alphanum(subtag: &str, shortest: usize, longest: usize) -> bool {
    is_alphanum(subtag) && (shortest..=longest).contains(&subtag.len())
}

fn language_id_length(subtags: &[&str]) -> Option<usize> {
    let mut index = 0;
    if !subtags.first().is_some_and(|s| is_language_subtag(s)) {
        return None;
    }
    index += 1;
    if subtags.get(index).is_some_and(|s| is_script_subtag(s)) {
        index += 1;
    }
    if subtags.get(index).is_some_and(|s| is_region_subtag(s)) {
        index += 1;
    }
    let mut variants: Vec<String> = Vec::new();
    while let Some(subtag) = subtags.get(index).filter(|s| is_variant_subtag(s)) {
        let lower = subtag.to_ascii_lowercase();
        if variants.contains(&lower) {
            return None;
        }
        variants.push(lower);
        index += 1;
    }
    Some(index)
}

fn unicode_extension_valid(subtags: &[&str]) -> bool {
    let mut index = 0;
    while subtags.get(index).is_some_and(|s| sized_alphanum(s, 3, 8)) {
        index += 1;
    }
    let mut any = index > 0;
    while let Some(key) = subtags.get(index) {
        let bytes = key.as_bytes();
        if key.len() != 2 || !bytes[0].is_ascii_alphanumeric() || !bytes[1].is_ascii_alphabetic() {
            return false;
        }
        any = true;
        index += 1;
        while subtags.get(index).is_some_and(|s| sized_alphanum(s, 3, 8)) {
            index += 1;
        }
    }
    any
}

fn transformed_extension_valid(subtags: &[&str]) -> bool {
    let mut index = 0;
    if subtags.first().is_some_and(|s| is_language_subtag(s)) {
        match language_id_length(subtags) {
            Some(length) => index = length,
            None => return false,
        }
    }
    let mut any = index > 0;
    while let Some(key) = subtags.get(index) {
        let bytes = key.as_bytes();
        if key.len() != 2 || !bytes[0].is_ascii_alphabetic() || !bytes[1].is_ascii_digit() {
            return false;
        }
        index += 1;
        let start = index;
        while subtags.get(index).is_some_and(|s| sized_alphanum(s, 3, 8)) {
            index += 1;
        }
        if index == start {
            return false;
        }
        any = true;
    }
    any
}

pub(crate) fn is_structurally_valid_language_tag(tag: &str) -> bool {
    if tag.is_empty() || !tag.is_ascii() {
        return false;
    }
    let subtags: Vec<&str> = tag.split('-').collect();
    if subtags.iter().any(|s| s.is_empty()) {
        return false;
    }
    let Some(mut index) = language_id_length(&subtags) else {
        return false;
    };
    let mut singletons: Vec<u8> = Vec::new();
    while index < subtags.len() {
        let singleton = subtags[index];
        if singleton.len() != 1 || !is_alphanum(singleton) {
            return false;
        }
        let letter = singleton.as_bytes()[0].to_ascii_lowercase();
        if letter == b'x' {
            let rest = &subtags[index + 1..];
            return !rest.is_empty() && rest.iter().all(|s| sized_alphanum(s, 1, 8));
        }
        if singletons.contains(&letter) {
            return false;
        }
        singletons.push(letter);
        let end = subtags[index + 1..]
            .iter()
            .position(|s| s.len() == 1)
            .map_or(subtags.len(), |offset| index + 1 + offset);
        let body = &subtags[index + 1..end];
        let valid = match letter {
            b'u' => unicode_extension_valid(body),
            b't' => transformed_extension_valid(body),
            _ => !body.is_empty() && body.iter().all(|s| sized_alphanum(s, 2, 8)),
        };
        if !valid {
            return false;
        }
        index = end;
    }
    true
}

fn canonical_case(index: usize, subtag: &str) -> String {
    if index > 0 && is_script_subtag(subtag) {
        let lower = subtag.to_ascii_lowercase();
        return lower[..1].to_ascii_uppercase() + &lower[1..];
    }
    if index > 0 && subtag.len() == 2 && is_alpha(subtag) {
        return subtag.to_ascii_uppercase();
    }
    subtag.to_ascii_lowercase()
}

fn canonical_unicode_extension(body: &[&str]) -> Vec<String> {
    let mut index = 0;
    let mut attributes: Vec<String> = Vec::new();
    while body.get(index).is_some_and(|s| s.len() != 2) {
        let attribute = body[index].to_ascii_lowercase();
        if !attributes.contains(&attribute) {
            attributes.push(attribute);
        }
        index += 1;
    }
    attributes.sort();
    let mut keywords: Vec<(String, Vec<String>)> = Vec::new();
    while let Some(key) = body.get(index) {
        let key = key.to_ascii_lowercase();
        index += 1;
        let mut types = Vec::new();
        while body.get(index).is_some_and(|s| s.len() != 2) {
            types.push(body[index].to_ascii_lowercase());
            index += 1;
        }
        if types.len() == 1 && types[0] == "true" {
            types.clear();
        }
        if !keywords.iter().any(|(known, _)| *known == key) {
            keywords.push((key, types));
        }
    }
    keywords.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = attributes;
    for (key, types) in keywords {
        out.push(key);
        out.extend(types);
    }
    out
}

pub(crate) fn canonicalize_language_tag(tag: &str) -> String {
    let subtags: Vec<&str> = tag.split('-').collect();
    let base_length = language_id_length(&subtags).unwrap_or(subtags.len());
    let mut base: Vec<String> = subtags[..base_length]
        .iter()
        .enumerate()
        .map(|(i, s)| canonical_case(i, s))
        .collect();
    let variant_start = base
        .iter()
        .skip(1)
        .position(|s| is_variant_subtag(s))
        .map_or(base.len(), |offset| offset + 1);
    base[variant_start..].sort();
    let mut extensions: Vec<(u8, Vec<String>)> = Vec::new();
    let mut private: Vec<String> = Vec::new();
    let mut index = base_length;
    while index < subtags.len() {
        let letter = subtags[index].as_bytes()[0].to_ascii_lowercase();
        if letter == b'x' {
            private = subtags[index..]
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect();
            break;
        }
        let end = subtags[index + 1..]
            .iter()
            .position(|s| s.len() == 1)
            .map_or(subtags.len(), |offset| index + 1 + offset);
        let body = &subtags[index + 1..end];
        let canonical = match letter {
            b'u' => canonical_unicode_extension(body),
            _ => body.iter().map(|s| s.to_ascii_lowercase()).collect(),
        };
        if letter != b'u' || !canonical.is_empty() {
            extensions.push((letter, canonical));
        }
        index = end;
    }
    extensions.sort_by_key(|(letter, _)| *letter);
    let mut out = base;
    for (letter, body) in extensions {
        out.push(char::from(letter).to_string());
        out.extend(body);
    }
    out.extend(private);
    out.join("-")
}

pub(crate) fn split_unicode_extension(tag: &str) -> (String, Option<String>) {
    let subtags: Vec<&str> = tag.split('-').collect();
    let mut base: Vec<&str> = Vec::new();
    let mut extension: Option<String> = None;
    let mut index = 0;
    while index < subtags.len() {
        let subtag = subtags[index];
        if subtag.len() == 1 && subtag.eq_ignore_ascii_case("x") {
            base.extend_from_slice(&subtags[index..]);
            break;
        }
        if subtag.len() == 1 && index > 0 {
            let end = subtags[index + 1..]
                .iter()
                .position(|s| s.len() == 1)
                .map_or(subtags.len(), |offset| index + 1 + offset);
            if subtag.eq_ignore_ascii_case("u") {
                extension = Some(subtags[index..end].join("-"));
            } else {
                base.extend_from_slice(&subtags[index..end]);
            }
            index = end;
            continue;
        }
        base.push(subtag);
        index += 1;
    }
    (base.join("-"), extension)
}

fn unicode_keywords(extension: &str) -> Vec<(String, String)> {
    let subtags: Vec<&str> = extension.split('-').skip(1).collect();
    let mut keywords: Vec<(String, String)> = Vec::new();
    let mut index = 0;
    while subtags.get(index).is_some_and(|s| s.len() != 2) {
        index += 1;
    }
    while let Some(key) = subtags.get(index) {
        index += 1;
        let start = index;
        while subtags.get(index).is_some_and(|s| s.len() != 2) {
            index += 1;
        }
        let value = subtags[start..index].join("-");
        if !keywords.iter().any(|(known, _)| known == key) {
            keywords.push(((*key).to_owned(), value));
        }
    }
    keywords
}

fn lookup_prefix(available: &dyn Fn(&str) -> bool, locale: &str) -> Option<String> {
    let mut prefix = locale.to_owned();
    loop {
        if available(&prefix) {
            return Some(prefix);
        }
        let cut = prefix.rfind('-')?;
        prefix.truncate(cut);
        if prefix.len() >= 2 && prefix.as_bytes()[prefix.len() - 2] == b'-' {
            prefix.truncate(prefix.len() - 2);
        }
    }
}

pub(crate) fn default_locale(available: &dyn Fn(&str) -> bool) -> String {
    let raw = crate::negotiate::default_locale().to_owned();
    if is_structurally_valid_language_tag(&raw) {
        let (base, _) = split_unicode_extension(&canonicalize_language_tag(&raw));
        if let Some(found) = lookup_prefix(available, &base) {
            return found;
        }
    }
    "en-US".to_owned()
}

pub(crate) struct KeyRequest<'a> {
    pub key: &'static str,
    pub option: Option<String>,
    pub supported: &'a dyn Fn(&str, &str) -> bool,
    pub default: &'a dyn Fn(&str) -> String,
}

pub(crate) struct Resolution {
    pub locale: String,
    pub data_locale: String,
    pub values: Vec<String>,
}

pub(crate) fn resolve_locale(
    requested: &[String],
    available: &dyn Fn(&str) -> bool,
    keys: &[KeyRequest<'_>],
) -> Resolution {
    let mut found: Option<(String, Option<String>)> = None;
    for locale in requested {
        let (base, extension) = split_unicode_extension(locale);
        if let Some(prefix) = lookup_prefix(available, &base) {
            found = Some((prefix, extension));
            break;
        }
    }
    let (data_locale, extension) = found.unwrap_or_else(|| (default_locale(available), None));
    let keywords = extension
        .as_deref()
        .map(unicode_keywords)
        .unwrap_or_default();
    let mut supported_keywords: Vec<(String, String)> = Vec::new();
    let mut values = Vec::new();
    for request in keys {
        let mut value = (request.default)(&data_locale);
        let mut keyword: Option<(String, String)> = None;
        if let Some((_, requested_value)) = keywords.iter().find(|(key, _)| key == request.key) {
            if !requested_value.is_empty() {
                if (request.supported)(&data_locale, requested_value) {
                    value.clone_from(requested_value);
                    keyword = Some((request.key.to_owned(), value.clone()));
                }
            } else if (request.supported)(&data_locale, "true") {
                value = "true".to_owned();
                keyword = Some((request.key.to_owned(), String::new()));
            }
        }
        if let Some(option) = &request.option {
            let option = option.to_ascii_lowercase();
            let option = if option.is_empty() {
                "true".to_owned()
            } else {
                option
            };
            if option != value && (request.supported)(&data_locale, &option) {
                value = option;
                keyword = None;
            }
        }
        if let Some(keyword) = keyword {
            supported_keywords.push(keyword);
        }
        values.push(value);
    }
    let locale = if supported_keywords.is_empty() {
        data_locale.clone()
    } else {
        supported_keywords.sort();
        let mut tag = data_locale.clone();
        tag.push_str("-u");
        for (key, value) in &supported_keywords {
            tag.push('-');
            tag.push_str(key);
            if !value.is_empty() {
                tag.push('-');
                tag.push_str(value);
            }
        }
        canonicalize_language_tag(&tag)
    };
    Resolution {
        locale,
        data_locale,
        values,
    }
}

pub(crate) fn supported_locales(
    scope: &mut Scope<'_>,
    locales: &Value,
    options: &Value,
    available: &dyn Fn(&str) -> bool,
) -> Result<Value, Value> {
    let requested = canonicalize_locale_list(scope, locales)?;
    let options = OptionsBag::coerce(scope, options)?;
    options.string(scope, "localeMatcher", &["lookup", "best fit"], "best fit")?;
    let array = scope.new_array();
    let mut count = 0;
    for locale in requested {
        let (base, _) = split_unicode_extension(&locale);
        if lookup_prefix(available, &base).is_some() {
            let value = scope.string(&locale);
            scope.define_entry(&array, &Value::int(count), value)?;
            count += 1;
        }
    }
    Ok(array)
}

pub(crate) fn is_new_target(scope: &mut Scope<'_>, this: &Value) -> bool {
    this.is_object() && scope.is_constructor(this)
}

pub(crate) fn prototype_from_constructor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    intrinsic: &Value,
    service: &str,
) -> Result<Value, Value> {
    let prototype = scope.get(new_target, "prototype")?;
    if prototype.is_object() {
        return Ok(prototype);
    }
    let realm_prototype = scope.function_handle(new_target).ok().and_then(|handle| {
        handle.with(|other| {
            let global = other.global();
            other
                .get(&global, "Intl")
                .and_then(|intl| other.get(&intl, service))
                .and_then(|constructor| other.get(&constructor, "prototype"))
                .ok()
                .filter(Value::is_object)
        })
    });
    Ok(realm_prototype
        .flatten()
        .unwrap_or_else(|| intrinsic.clone()))
}

pub(crate) fn ordinary_has_instance(
    scope: &mut Scope<'_>,
    prototype: &Value,
    object: &Value,
) -> Result<bool, Value> {
    if !object.is_object() {
        return Ok(false);
    }
    let mut current = object.clone();
    loop {
        current = scope.get_prototype_of(&current)?;
        if !current.is_object() {
            return Ok(false);
        }
        if current.same_object(prototype) {
            return Ok(true);
        }
    }
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

pub(crate) fn create_string_property(
    scope: &mut Scope<'_>,
    object: &Value,
    key: &str,
    value: &str,
) -> Result<(), Value> {
    let value = scope.string(value);
    create_data_property(scope, object, key, value)
}

pub(crate) fn array_from_strings(scope: &mut Scope<'_>, values: &[&str]) -> Result<Value, Value> {
    let array = scope.new_array();
    for (index, value) in values.iter().enumerate() {
        let value = scope.string(value);
        scope.define_entry(&array, &Value::int(index as i32), value)?;
    }
    Ok(array)
}
