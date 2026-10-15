//! Southstar — Intl.DateTimeFormat's locale lists: language tag validation, canonicalization and locale resolution.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

const LANGUAGE_ALIASES: [(&str, &str); 5] = [
    ("iw", "he"),
    ("in", "id"),
    ("ji", "yi"),
    ("jw", "jv"),
    ("mo", "ro"),
];

const TYPE_ALIASES: [(&str, &str, &str); 3] = [
    ("ca", "islamicc", "islamic-civil"),
    ("ca", "ethiopic-amete-alem", "ethioaa"),
    ("ca", "gregorian", "gregory"),
];

#[derive(Clone, Default)]
pub(crate) struct Tag {
    pub(crate) language: String,
    pub(crate) script: Option<String>,
    pub(crate) region: Option<String>,
    pub(crate) variants: Vec<String>,
    pub(crate) attributes: Vec<String>,
    pub(crate) keywords: Vec<(String, String)>,
    pub(crate) others: Vec<(u8, Vec<String>)>,
    pub(crate) private: Vec<String>,
}

fn all(text: &str, test: fn(&u8) -> bool) -> bool {
    !text.is_empty() && text.as_bytes().iter().all(test)
}

fn is_alpha(text: &str) -> bool {
    all(text, u8::is_ascii_alphabetic)
}

fn is_digit(text: &str) -> bool {
    all(text, u8::is_ascii_digit)
}

fn is_alnum(text: &str) -> bool {
    all(text, u8::is_ascii_alphanumeric)
}

fn is_language(text: &str) -> bool {
    is_alpha(text) && matches!(text.len(), 2 | 3 | 5..=8)
}

fn is_script(text: &str) -> bool {
    text.len() == 4 && is_alpha(text)
}

fn is_region(text: &str) -> bool {
    (text.len() == 2 && is_alpha(text)) || (text.len() == 3 && is_digit(text))
}

fn is_variant(text: &str) -> bool {
    is_alnum(text)
        && ((5..=8).contains(&text.len())
            || (text.len() == 4 && text.as_bytes()[0].is_ascii_digit()))
}

fn is_type_part(text: &str) -> bool {
    is_alnum(text) && (3..=8).contains(&text.len())
}

pub(crate) fn is_type_sequence(text: &str) -> bool {
    text.split('-').all(is_type_part)
}

fn unicode_extension(body: &[&str], tag: &mut Tag) -> Option<()> {
    let mut at = 0;
    while at < body.len() && is_type_part(body[at]) {
        tag.attributes.push(body[at].to_string());
        at += 1;
    }
    while at < body.len() {
        let key = body[at];
        if key.len() != 2 || !key.as_bytes()[1].is_ascii_alphabetic() {
            return None;
        }
        at += 1;
        let start = at;
        while at < body.len() && is_type_part(body[at]) {
            at += 1;
        }
        if !tag.keywords.iter().any(|(known, _)| known == key) {
            tag.keywords
                .push((key.to_string(), body[start..at].join("-")));
        }
    }
    Some(())
}

fn transformed_extension(body: &[&str]) -> Option<()> {
    let mut at = 0;
    if is_language(body[0]) {
        at = 1;
        if at < body.len() && is_script(body[at]) {
            at += 1;
        }
        if at < body.len() && is_region(body[at]) {
            at += 1;
        }
        let mut variants: Vec<&str> = Vec::new();
        while at < body.len() && is_variant(body[at]) {
            if variants.contains(&body[at]) {
                return None;
            }
            variants.push(body[at]);
            at += 1;
        }
    }
    while at < body.len() {
        let key = body[at].as_bytes();
        if key.len() != 2 || !key[0].is_ascii_alphabetic() || !key[1].is_ascii_digit() {
            return None;
        }
        at += 1;
        let start = at;
        while at < body.len() && is_type_part(body[at]) {
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    Some(())
}

pub(crate) fn parse(text: &str) -> Option<Tag> {
    if text.is_empty() || !text.is_ascii() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('-').collect();
    if parts.iter().any(|part| !is_alnum(part) || part.len() > 8) {
        return None;
    }
    if !is_language(parts[0]) {
        return None;
    }
    let mut tag = Tag {
        language: parts[0].to_string(),
        ..Tag::default()
    };
    let mut at = 1;
    if at < parts.len() && is_script(parts[at]) {
        tag.script = Some(parts[at].to_string());
        at += 1;
    }
    if at < parts.len() && is_region(parts[at]) {
        tag.region = Some(parts[at].to_string());
        at += 1;
    }
    while at < parts.len() && is_variant(parts[at]) {
        if tag.variants.iter().any(|known| known == parts[at]) {
            return None;
        }
        tag.variants.push(parts[at].to_string());
        at += 1;
    }
    let mut singletons: Vec<u8> = Vec::new();
    while at < parts.len() {
        if parts[at].len() != 1 {
            return None;
        }
        let singleton = parts[at].as_bytes()[0];
        at += 1;
        if singleton == b'x' {
            if at == parts.len() {
                return None;
            }
            tag.private = parts[at..].iter().map(|part| part.to_string()).collect();
            return Some(tag);
        }
        if singletons.contains(&singleton) {
            return None;
        }
        singletons.push(singleton);
        let start = at;
        while at < parts.len() && parts[at].len() != 1 {
            at += 1;
        }
        let body = &parts[start..at];
        if body.is_empty() {
            return None;
        }
        match singleton {
            b'u' => unicode_extension(body, &mut tag)?,
            b't' => transformed_extension(body)?,
            _ if body.iter().any(|part| part.len() < 2) => return None,
            _ => {}
        }
        if singleton != b'u' {
            tag.others.push((
                singleton,
                body.iter().map(|part| part.to_string()).collect(),
            ));
        }
    }
    Some(tag)
}

pub(crate) fn canonical_type(key: &str, value: &str) -> String {
    let value = value.to_ascii_lowercase();
    TYPE_ALIASES
        .iter()
        .find(|(alias_key, alias, _)| *alias_key == key && *alias == value)
        .map_or(value, |(_, _, canonical)| canonical.to_string())
}

impl Tag {
    fn canonicalize(&mut self) {
        if let Some((_, canonical)) = LANGUAGE_ALIASES
            .iter()
            .find(|(alias, _)| *alias == self.language)
        {
            self.language = canonical.to_string();
        }
        self.variants.sort();
        self.attributes.sort();
        self.attributes.dedup();
        for (key, value) in &mut self.keywords {
            *value = canonical_type(key, value);
            if value == "true" {
                value.clear();
            }
        }
        self.keywords.sort_by(|a, b| a.0.cmp(&b.0));
        self.others.sort_by_key(|(singleton, _)| *singleton);
    }

    pub(crate) fn base_name(&self) -> String {
        let mut out = self.language.clone();
        if let Some(script) = &self.script {
            out.push('-');
            out.push_str(&script[..1].to_ascii_uppercase());
            out.push_str(&script[1..]);
        }
        if let Some(region) = &self.region {
            out.push('-');
            out.push_str(&region.to_ascii_uppercase());
        }
        for variant in &self.variants {
            out.push('-');
            out.push_str(variant);
        }
        out
    }

    pub(crate) fn keyword(&self, key: &str) -> Option<&str> {
        self.keywords
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.as_str())
    }

    fn unicode_body(&self) -> Vec<String> {
        let mut body = self.attributes.clone();
        for (key, value) in &self.keywords {
            body.push(key.clone());
            if !value.is_empty() {
                body.push(value.clone());
            }
        }
        body
    }

    pub(crate) fn render(&self) -> String {
        let mut out = self.base_name();
        let mut extensions: Vec<(u8, Vec<String>)> = self.others.clone();
        let unicode = self.unicode_body();
        if !unicode.is_empty() {
            extensions.push((b'u', unicode));
        }
        extensions.sort_by_key(|(singleton, _)| *singleton);
        for (singleton, body) in extensions {
            out.push('-');
            out.push(char::from(singleton));
            for part in body {
                out.push('-');
                out.push_str(&part);
            }
        }
        if !self.private.is_empty() {
            out.push_str("-x");
            for part in &self.private {
                out.push('-');
                out.push_str(part);
            }
        }
        out
    }

    pub(crate) fn without_unicode(&self) -> Tag {
        Tag {
            attributes: Vec::new(),
            keywords: Vec::new(),
            ..self.clone()
        }
    }
}

pub(crate) fn canonical_tag(text: &str) -> Option<Tag> {
    let mut tag = parse(text)?;
    tag.canonicalize();
    Some(tag)
}

fn to_length(scope: &mut Scope<'_>, value: &Value) -> Result<u64, Value> {
    let number = scope.to_number(value)?;
    if number.is_nan() || number <= 0.0 {
        return Ok(0);
    }
    Ok(number.min(9_007_199_254_740_991.0).floor() as u64)
}

fn is_locale_object(scope: &mut Scope<'_>, value: &Value) -> bool {
    let global = scope.global();
    let locale = scope
        .get(&global, "Intl")
        .and_then(|intl| scope.get(&intl, "Locale"));
    match locale {
        Ok(locale) if scope.is_function(&locale) => scope.instance_of(value, &locale),
        _ => false,
    }
}

pub(crate) fn canonicalize_locale_list(
    scope: &mut Scope<'_>,
    locales: &Value,
) -> Result<Vec<String>, Value> {
    if locales.is_undefined() {
        return Ok(Vec::new());
    }
    let single = locales.is_string() || is_locale_object(scope, locales);
    let entries: Vec<Value> = if single {
        vec![locales.clone()]
    } else {
        let object = scope.to_object(locales)?;
        let length_value = scope.get(&object, "length")?;
        let length = to_length(scope, &length_value)?;
        let mut entries = Vec::new();
        for index in 0..length {
            let key = index.to_string();
            if scope.has_property(&object, &key)? {
                entries.push(scope.get(&object, &key)?);
            }
        }
        entries
    };
    let mut seen: Vec<String> = Vec::new();
    for entry in entries {
        if !entry.is_string() && !entry.is_object() {
            return Err(scope.type_error("locale must be a string or an object"));
        }
        let text = scope.to_string(&entry)?;
        let Some(tag) = canonical_tag(&text) else {
            return Err(scope.range_error(&format!("invalid language tag: {text}")));
        };
        let canonical = tag.render();
        if !seen.contains(&canonical) {
            seen.push(canonical);
        }
    }
    Ok(seen)
}

pub(crate) enum Requested {
    Absent,
    Null,
    Text(String),
}

pub(crate) struct Keyword<'a> {
    pub(crate) key: &'static str,
    pub(crate) requested: Requested,
    pub(crate) supported: &'a dyn Fn(&str) -> bool,
}

pub(crate) struct Resolution {
    pub(crate) locale: String,
    pub(crate) data_tag: Tag,
    pub(crate) values: Vec<Option<String>>,
}

fn truncate(tag: &str) -> Option<String> {
    let mut end = tag.rfind('-')?;
    if end >= 2 && tag.as_bytes()[end - 2] == b'-' {
        end -= 2;
    }
    Some(tag[..end].to_string())
}

pub(crate) fn lookup(
    requested: &[String],
    available: &dyn Fn(&Tag) -> bool,
) -> Option<(Tag, Option<Tag>)> {
    for locale in requested {
        let Some(tag) = canonical_tag(locale) else {
            continue;
        };
        let mut candidate = tag.without_unicode().render();
        loop {
            if let Some(found) = canonical_tag(&candidate)
                && available(&found)
            {
                return Some((found, Some(tag)));
            }
            match truncate(&candidate) {
                Some(shorter) => candidate = shorter,
                None => break,
            }
        }
    }
    None
}

pub(crate) fn resolve(
    requested: &[String],
    available: &dyn Fn(&Tag) -> bool,
    fallback: &Tag,
    keywords: &[Keyword<'_>],
    defaults: &dyn Fn(&Tag, &str) -> Option<String>,
) -> Resolution {
    let (found, source) = lookup(requested, available).unwrap_or_else(|| (fallback.clone(), None));
    let mut supported_keywords: Vec<(String, String)> = Vec::new();
    let mut values = Vec::new();
    for keyword in keywords {
        let mut value = defaults(&found, keyword.key);
        let mut supported_keyword: Option<(String, String)> = None;
        if let Some(requested) = source.as_ref().and_then(|tag| tag.keyword(keyword.key))
            && !requested.is_empty()
            && (keyword.supported)(requested)
        {
            value = Some(requested.to_string());
            supported_keyword = Some((keyword.key.to_string(), requested.to_string()));
        }
        let option = match &keyword.requested {
            Requested::Absent => None,
            Requested::Null => Some(None),
            Requested::Text(text) => {
                let canonical = canonical_type(keyword.key, text);
                Some(Some(if canonical.is_empty() {
                    "true".to_string()
                } else {
                    canonical
                }))
            }
        };
        if let Some(option) = option
            && option != value
        {
            let supported = match &option {
                Some(text) => (keyword.supported)(text),
                None => true,
            };
            if supported {
                value = option;
                supported_keyword = None;
            }
        }
        if let Some(entry) = supported_keyword {
            supported_keywords.push(entry);
        }
        values.push(value);
    }
    let mut locale_tag = found.clone();
    locale_tag.keywords = supported_keywords;
    locale_tag.canonicalize();
    Resolution {
        locale: locale_tag.render(),
        data_tag: found,
        values,
    }
}
