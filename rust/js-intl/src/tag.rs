//! Southstar — Unicode BCP 47 locale identifiers: the UTS #35 grammar, canonicalization and likely subtags.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::cldr_index;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LanguageId {
    pub(crate) language: String,
    pub(crate) script: Option<String>,
    pub(crate) region: Option<String>,
    pub(crate) variants: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Transformed {
    pub(crate) tlang: Option<LanguageId>,
    pub(crate) fields: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocaleId {
    pub(crate) id: LanguageId,
    pub(crate) attributes: Vec<String>,
    pub(crate) keywords: Vec<(String, String)>,
    pub(crate) transformed: Option<Transformed>,
    pub(crate) others: Vec<(char, String)>,
    pub(crate) private_use: Vec<String>,
}

fn all_bytes(text: &str, min: usize, max: usize, test: fn(&u8) -> bool) -> bool {
    (min..=max).contains(&text.len()) && text.bytes().all(|b| test(&b))
}

fn alpha(text: &str, min: usize, max: usize) -> bool {
    all_bytes(text, min, max, u8::is_ascii_alphabetic)
}

fn alphanum(text: &str, min: usize, max: usize) -> bool {
    all_bytes(text, min, max, u8::is_ascii_alphanumeric)
}

pub(crate) fn is_language_subtag(text: &str) -> bool {
    alpha(text, 2, 3) || alpha(text, 5, 8)
}

pub(crate) fn is_script_subtag(text: &str) -> bool {
    alpha(text, 4, 4)
}

pub(crate) fn is_region_subtag(text: &str) -> bool {
    alpha(text, 2, 2) || all_bytes(text, 3, 3, u8::is_ascii_digit)
}

pub(crate) fn is_variant_subtag(text: &str) -> bool {
    alphanum(text, 5, 8) || (alphanum(text, 4, 4) && text.as_bytes()[0].is_ascii_digit())
}

pub(crate) fn is_type_value(text: &str) -> bool {
    text.split('-').all(|part| alphanum(part, 3, 8))
}

fn is_unicode_key(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphanumeric() && bytes[1].is_ascii_alphabetic()
}

fn is_transform_key(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1].is_ascii_digit()
}

pub(crate) fn title_case(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut chars = lower.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}

struct Subtags<'a> {
    parts: Vec<&'a str>,
    at: usize,
}

impl<'a> Subtags<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.parts.get(self.at).copied()
    }

    fn take_if(&mut self, test: fn(&str) -> bool) -> Option<&'a str> {
        let part = self.peek().filter(|part| test(part))?;
        self.at += 1;
        Some(part)
    }

    fn take_values(&mut self, accepts: fn(&str) -> bool) -> String {
        let mut values = Vec::new();
        while let Some(part) = self.take_if(accepts) {
            values.push(part.to_ascii_lowercase());
        }
        values.join("-")
    }

    fn language_id(&mut self) -> Option<LanguageId> {
        let language = self.take_if(is_language_subtag)?.to_ascii_lowercase();
        let script = self.take_if(is_script_subtag).map(title_case);
        let region = self
            .take_if(is_region_subtag)
            .map(|region| region.to_ascii_uppercase());
        let mut variants: Vec<String> = Vec::new();
        while let Some(variant) = self.take_if(is_variant_subtag) {
            let variant = variant.to_ascii_lowercase();
            if variants.contains(&variant) {
                return None;
            }
            variants.push(variant);
        }
        Some(LanguageId {
            language,
            script,
            region,
            variants,
        })
    }

    fn unicode_extension(&mut self, locale: &mut LocaleId) -> Option<()> {
        while let Some(attribute) = self.take_if(|part| alphanum(part, 3, 8)) {
            locale.attributes.push(attribute.to_ascii_lowercase());
        }
        while let Some(key) = self.peek().filter(|part| part.len() == 2) {
            if !is_unicode_key(key) {
                return None;
            }
            self.at += 1;
            let value = self.take_values(|part| alphanum(part, 3, 8));
            locale.keywords.push((key.to_ascii_lowercase(), value));
        }
        (!locale.attributes.is_empty() || !locale.keywords.is_empty()).then_some(())
    }

    fn transformed_extension(&mut self) -> Option<Transformed> {
        let mut transformed = Transformed::default();
        if self.peek().is_some_and(is_language_subtag) {
            transformed.tlang = Some(self.language_id()?);
        }
        while let Some(key) = self.peek().filter(|part| part.len() == 2) {
            if !is_transform_key(key) {
                return None;
            }
            self.at += 1;
            let value = self.take_values(|part| alphanum(part, 3, 8));
            if value.is_empty() {
                return None;
            }
            transformed.fields.push((key.to_ascii_lowercase(), value));
        }
        (transformed.tlang.is_some() || !transformed.fields.is_empty()).then_some(transformed)
    }

    fn other_extension(&mut self) -> Option<String> {
        let value = self.take_values(|part| alphanum(part, 2, 8));
        (!value.is_empty()).then_some(value)
    }
}

impl LanguageId {
    pub(crate) fn parse(text: &str) -> Option<LanguageId> {
        let locale = LocaleId::parse(text)?;
        let plain = LocaleId {
            id: locale.id.clone(),
            ..LocaleId::default()
        };
        (locale == plain).then_some(locale.id)
    }

    fn write(&self, out: &mut String) {
        out.push_str(&self.language);
        for part in [&self.script, &self.region].into_iter().flatten() {
            out.push('-');
            out.push_str(part);
        }
        for variant in &self.variants {
            out.push('-');
            out.push_str(variant);
        }
    }

    pub(crate) fn to_text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    pub(crate) fn matches_rule(&self, rule: &LanguageId) -> bool {
        (rule.language == "und" || rule.language == self.language)
            && (rule.script.is_none() || rule.script == self.script)
            && (rule.region.is_none() || rule.region == self.region)
            && rule.variants.iter().all(|v| self.variants.contains(v))
    }

    fn apply(&mut self, rule: &LanguageId, replacement: &LanguageId) {
        if rule.language != "und" || (self.language == "und" && replacement.language != "und") {
            self.language.clone_from(&replacement.language);
        }
        if rule.script.is_some() || self.script.is_none() {
            self.script.clone_from(&replacement.script);
        }
        if rule.region.is_some() || self.region.is_none() {
            self.region.clone_from(&replacement.region);
        }
        self.variants.retain(|v| !rule.variants.contains(v));
        for variant in &replacement.variants {
            if !self.variants.contains(variant) {
                self.variants.push(variant.clone());
            }
        }
        self.variants.sort();
    }

    fn replace_region(&mut self) -> bool {
        let Some(regions) = self.region.as_deref().and_then(cldr_index::territory_alias) else {
            return false;
        };
        let likely = LanguageId {
            language: self.language.clone(),
            script: self.script.clone(),
            ..LanguageId::default()
        };
        let likely_region = maximize(&likely).and_then(|max| max.region);
        let chosen = regions
            .iter()
            .find(|region| Some(region.as_str()) == likely_region.as_deref())
            .or_else(|| regions.first());
        self.region = chosen.cloned();
        true
    }

    fn replace_aliases(&mut self) {
        for _ in 0..32 {
            if let Some((rule, replacement)) = cldr_index::language_rule(self) {
                self.apply(rule, replacement);
            } else if !self.replace_region() {
                return;
            }
        }
    }

    pub(crate) fn canonicalize(&mut self) {
        self.variants.sort();
        self.replace_aliases();
    }

    pub(crate) fn rule_specificity(&self) -> (usize, bool, bool, bool) {
        (
            self.variants.len(),
            self.language != "und",
            self.region.is_some(),
            self.script.is_some(),
        )
    }
}

impl LocaleId {
    pub(crate) fn parse(text: &str) -> Option<LocaleId> {
        if text.is_empty() || !text.is_ascii() {
            return None;
        }
        let parts: Vec<&str> = text.split('-').collect();
        if parts.iter().any(|part| !alphanum(part, 1, 8)) {
            return None;
        }
        let mut subtags = Subtags { parts, at: 0 };
        let mut locale = LocaleId {
            id: subtags.language_id()?,
            ..LocaleId::default()
        };
        let mut seen: Vec<char> = Vec::new();
        while let Some(singleton) = subtags.peek() {
            if singleton.len() != 1 {
                return None;
            }
            subtags.at += 1;
            let singleton = singleton.to_ascii_lowercase().chars().next()?;
            if seen.contains(&singleton) {
                return None;
            }
            seen.push(singleton);
            match singleton {
                'x' => {
                    let rest = &subtags.parts[subtags.at..];
                    if rest.is_empty() {
                        return None;
                    }
                    locale.private_use = rest.iter().map(|p| p.to_ascii_lowercase()).collect();
                    break;
                }
                'u' => subtags.unicode_extension(&mut locale)?,
                't' => locale.transformed = Some(subtags.transformed_extension()?),
                other => {
                    let value = subtags.other_extension()?;
                    locale.others.push((other, value));
                }
            }
        }
        Some(locale)
    }

    pub(crate) fn keyword(&self, key: &str) -> Option<&str> {
        self.keywords
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn has_unicode_extension(&self) -> bool {
        !self.attributes.is_empty() || !self.keywords.is_empty()
    }

    pub(crate) fn without_unicode_extension(&self) -> LocaleId {
        LocaleId {
            attributes: Vec::new(),
            keywords: Vec::new(),
            ..self.clone()
        }
    }

    pub(crate) fn canonicalize(&mut self) {
        self.id.canonicalize();
        self.attributes.sort();
        self.attributes.dedup();
        self.keywords.sort_by(|a, b| a.0.cmp(&b.0));
        self.keywords
            .dedup_by(|later, earlier| later.0 == earlier.0);
        for (key, value) in &mut self.keywords {
            *value = canonical_unicode_value(key, value);
        }
        if let Some(transformed) = &mut self.transformed {
            if let Some(tlang) = &mut transformed.tlang {
                tlang.canonicalize();
            }
            transformed.fields.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, value) in &mut transformed.fields {
                if let Some(alias) = cldr_index::transform_value_alias(key, value) {
                    *value = alias.to_owned();
                }
            }
        }
        self.others.sort_by_key(|(singleton, _)| *singleton);
    }

    pub(crate) fn to_text(&self) -> String {
        let mut out = self.id.to_text();
        let mut extensions: Vec<(char, String)> = self.others.clone();
        if let Some(transformed) = &self.transformed {
            let mut parts = Vec::new();
            if let Some(tlang) = &transformed.tlang {
                parts.push(tlang.to_text().to_ascii_lowercase());
            }
            for (key, value) in &transformed.fields {
                parts.push(format!("{key}-{value}"));
            }
            extensions.push(('t', parts.join("-")));
        }
        if self.has_unicode_extension() {
            let mut parts = self.attributes.clone();
            for (key, value) in &self.keywords {
                parts.push(key.clone());
                if !value.is_empty() {
                    parts.push(value.clone());
                }
            }
            extensions.push(('u', parts.join("-")));
        }
        extensions.sort_by_key(|(singleton, _)| *singleton);
        for (singleton, body) in extensions {
            out.push('-');
            out.push(singleton);
            out.push('-');
            out.push_str(&body);
        }
        if !self.private_use.is_empty() {
            out.push_str("-x-");
            out.push_str(&self.private_use.join("-"));
        }
        out
    }
}

pub(crate) fn canonical_unicode_value(key: &str, value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let aliased = match key {
        "rg" | "sd" => cldr_index::subdivision_alias(&lower),
        _ => cldr_index::unicode_value_alias(key, &lower),
    };
    let value = aliased.map_or(lower, str::to_owned);
    if value == "true" {
        String::new()
    } else {
        value
    }
}

pub(crate) fn canonicalize_tag(text: &str) -> Option<String> {
    let mut locale = LocaleId::parse(text)?;
    locale.canonicalize();
    Some(locale.to_text())
}

fn clean(id: &LanguageId) -> LanguageId {
    LanguageId {
        language: id.language.clone(),
        script: id.script.clone().filter(|script| script != "Zzzz"),
        region: id.region.clone().filter(|region| region != "ZZ"),
        variants: Vec::new(),
    }
}

fn lookup_keys(id: &LanguageId) -> Vec<String> {
    let language = &id.language;
    let mut keys = Vec::new();
    if let (Some(script), Some(region)) = (&id.script, &id.region) {
        keys.push(format!("{language}-{script}-{region}"));
    }
    if let Some(script) = &id.script {
        keys.push(format!("{language}-{script}"));
    }
    if let Some(region) = &id.region {
        keys.push(format!("{language}-{region}"));
    }
    keys.push(language.clone());
    keys
}

fn fill(id: &LanguageId, key: &str, missing: &str) -> LanguageId {
    let key_id = LanguageId::parse(key).unwrap_or_default();
    let mut values = missing.split('-');
    let language = if key_id.language == "und" {
        values.next().unwrap_or("und").to_owned()
    } else {
        key_id.language.clone()
    };
    let script = if key_id.script.is_none() {
        values.next().map(str::to_owned)
    } else {
        key_id.script.clone()
    };
    let region = if key_id.region.is_none() {
        values.next().map(str::to_owned)
    } else {
        key_id.region.clone()
    };
    LanguageId {
        language: if id.language == "und" {
            language
        } else {
            id.language.clone()
        },
        script: id.script.clone().or(script),
        region: id.region.clone().or(region),
        variants: id.variants.clone(),
    }
}

pub(crate) fn maximize(id: &LanguageId) -> Option<LanguageId> {
    let cleaned = clean(id);
    for key in lookup_keys(&cleaned) {
        if let Some(missing) = cldr_index::likely_subtags(&key) {
            let mut maximal = fill(&cleaned, &key, missing);
            maximal.variants = id.variants.clone();
            return Some(maximal);
        }
    }
    None
}

fn same_subtags(a: &LanguageId, b: &LanguageId) -> bool {
    a.language == b.language && a.script == b.script && a.region == b.region
}

pub(crate) fn minimize(id: &LanguageId) -> LanguageId {
    let Some(maximal) = maximize(id) else {
        return id.clone();
    };
    let trial = |script: Option<&String>, region: Option<&String>| LanguageId {
        language: maximal.language.clone(),
        script: script.cloned(),
        region: region.cloned(),
        variants: Vec::new(),
    };
    let trials = [
        trial(None, None),
        trial(None, maximal.region.as_ref()),
        trial(maximal.script.as_ref(), None),
    ];
    for candidate in trials {
        if maximize(&candidate).is_some_and(|max| same_subtags(&max, &maximal)) {
            return LanguageId {
                variants: id.variants.clone(),
                ..candidate
            };
        }
    }
    maximal
}
