//! Southstar — lookups over the generated CLDR tables, each indexed on first use.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::cldr;
use crate::tag::LanguageId;

type Index = HashMap<&'static str, &'static str>;

struct AliasRule {
    from: LanguageId,
    to: LanguageId,
}

fn pairs(source: &'static str) -> Index {
    source
        .split_ascii_whitespace()
        .filter_map(|entry| entry.split_once('='))
        .collect()
}

fn grouped(source: &'static str) -> Index {
    let mut index = HashMap::new();
    for entry in source.split_ascii_whitespace() {
        if let Some((value, keys)) = entry.split_once(':') {
            for key in keys.split(',') {
                index.insert(key, value);
            }
        }
    }
    index
}

fn words(source: &'static str) -> HashSet<&'static str> {
    source.split_ascii_whitespace().collect()
}

fn alias_rules() -> &'static HashMap<String, Vec<AliasRule>> {
    static RULES: OnceLock<HashMap<String, Vec<AliasRule>>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut rules: HashMap<String, Vec<AliasRule>> = HashMap::new();
        for (from, to) in pairs(cldr::LANGUAGE_ALIASES) {
            if let (Some(from), Some(to)) = (LanguageId::parse(from), LanguageId::parse(to)) {
                rules
                    .entry(from.language.clone())
                    .or_default()
                    .push(AliasRule { from, to });
            }
        }
        rules
    })
}

pub(crate) fn language_rule(id: &LanguageId) -> Option<(&'static LanguageId, &'static LanguageId)> {
    let rules = alias_rules();
    let candidates = [id.language.as_str(), "und"]
        .into_iter()
        .filter_map(|language| rules.get(language))
        .flatten();
    candidates
        .filter(|rule| id.matches_rule(&rule.from))
        .max_by_key(|rule| rule.from.rule_specificity())
        .map(|rule| (&rule.from, &rule.to))
}

pub(crate) fn territory_alias(region: &str) -> Option<Vec<String>> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    let regions = INDEX
        .get_or_init(|| pairs(cldr::TERRITORY_ALIASES))
        .get(region)?;
    Some(regions.split(',').map(str::to_owned).collect())
}

pub(crate) fn subdivision_alias(code: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| pairs(cldr::SUBDIVISION_ALIASES))
        .get(code)
        .copied()
}

pub(crate) fn unicode_value_alias(key: &str, value: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| pairs(cldr::UNICODE_VALUE_ALIASES))
        .get(format!("{key}:{value}").as_str())
        .copied()
}

pub(crate) fn transform_value_alias(key: &str, value: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| pairs(cldr::TRANSFORM_VALUE_ALIASES))
        .get(format!("{key}:{value}").as_str())
        .copied()
}

pub(crate) fn likely_subtags(key: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| pairs(cldr::LIKELY_SUBTAGS))
        .get(key)
        .copied()
}

pub(crate) fn calendars(region: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| grouped(cldr::CALENDAR_PREFERENCES))
        .get(region)
        .copied()
}

pub(crate) fn hour_cycle(key: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| grouped(cldr::HOUR_CYCLES))
        .get(key)
        .copied()
}

pub(crate) fn first_day(region: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| grouped(cldr::FIRST_DAYS))
        .get(region)
        .copied()
}

pub(crate) fn weekend(region: &str) -> Option<&'static str> {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX
        .get_or_init(|| grouped(cldr::WEEKENDS))
        .get(region)
        .copied()
}

fn zones_by_region() -> &'static HashMap<&'static str, Vec<&'static str>> {
    static INDEX: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        cldr::TIME_ZONES
            .split_ascii_whitespace()
            .filter_map(|entry| entry.split_once(':'))
            .map(|(region, zones)| (region, zones.split(',').collect()))
            .collect()
    })
}

pub(crate) fn time_zones(region: &str) -> Option<&'static [&'static str]> {
    zones_by_region().get(region).map(Vec::as_slice)
}

pub(crate) fn all_time_zones() -> Vec<&'static str> {
    let mut zones: Vec<&'static str> = zones_by_region().values().flatten().copied().collect();
    zones.sort_unstable();
    zones.dedup();
    zones
}

pub(crate) fn is_rtl_script(script: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| words(cldr::RTL_SCRIPTS))
        .contains(script)
}

pub(crate) fn is_ltr_script(script: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| words(cldr::LTR_SCRIPTS))
        .contains(script)
}

pub(crate) fn is_available_language(language: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| words(cldr::AVAILABLE_LANGUAGES))
        .contains(language)
}

pub(crate) fn is_available_script(language_script: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| words(cldr::AVAILABLE_SCRIPTS))
        .contains(language_script)
}
