//! Southstar text — languages: interned canonical RFC 3066 tags, range matching, sample sentences and the scripts a language uses.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use crate::script::Script;

#[path = "language_table.rs"]
mod table;

#[cfg(test)]
#[path = "tests/language.rs"]
mod tests;

const LANGUAGE_SEPARATORS: &[u8] = b";:, \t";

const DEFAULT_SAMPLE: &str = "The quick brown fox jumps over the lazy dog.";

const fn build_canon_map() -> [u8; 256] {
    let mut map = [0u8; 256];
    map[b'-' as usize] = b'-';
    map[b'@' as usize] = b'-';
    map[b'_' as usize] = b'-';
    let mut c = b'0';
    while c <= b'9' {
        map[c as usize] = c;
        c += 1;
    }
    let mut c = b'a';
    while c <= b'z' {
        map[c as usize] = c;
        map[(c - b'a' + b'A') as usize] = c;
        c += 1;
    }
    map
}

static CANON_MAP: [u8; 256] = build_canon_map();

fn canonicalize(language: &str) -> String {
    language
        .bytes()
        .map(|b| CANON_MAP[b as usize])
        .take_while(|&c| c != 0)
        .map(char::from)
        .collect()
}

struct LanguageEntry {
    tag: Box<str>,
    sample: OnceLock<Option<usize>>,
    scripts: OnceLock<Option<usize>>,
}

#[derive(Clone, Copy)]
pub struct Language(&'static LanguageEntry);

impl PartialEq for Language {
    fn eq(&self, other: &Language) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl Eq for Language {}

impl core::hash::Hash for Language {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::ptr::hash(self.0, state);
    }
}

impl fmt::Debug for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Language({:?})", self.as_str())
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn interned() -> &'static Mutex<HashMap<&'static str, &'static LanguageEntry>> {
    static TABLE: OnceLock<Mutex<HashMap<&'static str, &'static LanguageEntry>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Language {
    pub fn from_string(language: &str) -> Language {
        let tag = canonicalize(language);
        let mut table = interned()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = table.get(tag.as_str()) {
            return Language(entry);
        }
        let entry: &'static LanguageEntry = Box::leak(Box::new(LanguageEntry {
            tag: tag.into_boxed_str(),
            sample: OnceLock::new(),
            scripts: OnceLock::new(),
        }));
        table.insert(&entry.tag, entry);
        Language(entry)
    }

    pub fn as_str(self) -> &'static str {
        &self.0.tag
    }

    pub fn default_language() -> Language {
        static DEFAULT: OnceLock<Language> = OnceLock::new();
        *DEFAULT.get_or_init(|| Language::from_string(&lc_ctype()))
    }

    pub fn matches(self, range_list: &str) -> bool {
        language_matches(Some(self), range_list)
    }

    pub fn sample_string(self) -> &'static str {
        language_get_sample_string(Some(self))
    }

    pub fn scripts(self) -> &'static [Script] {
        language_get_scripts(Some(self))
    }

    pub fn includes_script(self, script: Script) -> bool {
        language_includes_script(Some(self), script)
    }
}

#[cfg(windows)]
fn lc_ctype() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(std::env::var_os)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(crate::ffi::glib_lang::win32_getlocale)
}

#[cfg(not(windows))]
fn lc_ctype() -> String {
    crate::ffi::glib_lang::lc_ctype()
}

fn until_nul(s: &str) -> &[u8] {
    let bytes = s.as_bytes();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

pub fn language_matches(language: Option<Language>, range_list: &str) -> bool {
    let lang = language.map(|l| l.as_str().as_bytes());
    let ranges = until_nul(range_list);
    let mut p = 0;
    loop {
        let rest = &ranges[p..];
        let (end, done) = match rest.iter().position(|b| LANGUAGE_SEPARATORS.contains(b)) {
            Some(offset) => (p + offset, false),
            None => (ranges.len(), true),
        };
        let range = &ranges[p..end];
        if rest.first() == Some(&b'*') {
            return true;
        }
        if let Some(lang) = lang {
            let n = range.len();
            if lang.len() >= n && &lang[..n] == range && (lang.len() == n || lang[n] == b'-') {
                return true;
            }
        }
        if done {
            return false;
        }
        p = end + 1;
    }
}

fn first_component_len(s: &[u8]) -> usize {
    s.iter().position(|&b| b == b'-').unwrap_or(s.len())
}

fn compare_first_component(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    let n = first_component_len(a).max(first_component_len(b));
    for i in 0..n {
        let ca = a.get(i).copied().unwrap_or(0);
        let cb = b.get(i).copied().unwrap_or(0);
        if ca != cb {
            return ca.cmp(&cb);
        }
        if ca == 0 {
            break;
        }
    }
    core::cmp::Ordering::Equal
}

fn bsearch_first_component(lang: &[u8], records: &[&str]) -> Option<usize> {
    let mut low = 0;
    let mut high = records.len();
    while low < high {
        let index = (low + high) / 2;
        match compare_first_component(lang, records[index].as_bytes()) {
            core::cmp::Ordering::Less => high = index,
            core::cmp::Ordering::Greater => low = index + 1,
            core::cmp::Ordering::Equal => return Some(index),
        }
    }
    None
}

fn find_best_lang_match(language: Language, records: &[&str]) -> Option<usize> {
    let lang = language.as_str().as_bytes();
    let mut record = bsearch_first_component(lang, records)?;
    while record + 1 < records.len()
        && compare_first_component(lang, records[record + 1].as_bytes()).is_eq()
    {
        record += 1;
    }
    loop {
        if !compare_first_component(lang, records[record].as_bytes()).is_eq() {
            return None;
        }
        if language.matches(records[record]) {
            return Some(record);
        }
        record = record.checked_sub(1)?;
    }
}

fn sample_index(language: Language) -> Option<usize> {
    *language.0.sample.get_or_init(|| {
        let records: Vec<&str> = table::SAMPLE_TEXTS.iter().map(|(lang, _)| *lang).collect();
        find_best_lang_match(language, &records)
    })
}

fn scripts_index(language: Language) -> Option<usize> {
    *language.0.scripts.get_or_init(|| {
        let records: Vec<&str> = table::SCRIPTS_FOR_LANG
            .iter()
            .map(|(lang, _)| *lang)
            .collect();
        find_best_lang_match(language, &records)
    })
}

pub fn language_get_sample_string(language: Option<Language>) -> &'static str {
    let language = language.unwrap_or_else(Language::default_language);
    match sample_index(language) {
        Some(index) => table::SAMPLE_TEXTS[index].1,
        None => DEFAULT_SAMPLE,
    }
}

pub fn language_get_scripts(language: Option<Language>) -> &'static [Script] {
    language
        .and_then(scripts_index)
        .map_or(&[], |index| table::SCRIPTS_FOR_LANG[index].1)
}

pub fn language_includes_script(language: Option<Language>, script: Script) -> bool {
    let real_script = script.0 > Script::INHERITED.0 && script != Script::UNKNOWN;
    if !real_script {
        return true;
    }
    let scripts = language_get_scripts(language);
    scripts.is_empty() || scripts.contains(&script)
}

fn parse_default_languages(value: &str) -> Vec<Language> {
    value
        .split(|c: char| c.is_ascii() && LANGUAGE_SEPARATORS.contains(&(c as u8)))
        .filter(|item| !item.is_empty() && *item != "C")
        .map(Language::from_string)
        .collect()
}

fn default_languages() -> Option<&'static [Language]> {
    static LANGUAGES: OnceLock<Option<Vec<Language>>> = OnceLock::new();
    LANGUAGES
        .get_or_init(|| {
            let value =
                std::env::var_os("NS_PANGO_LANGUAGE").or_else(|| std::env::var_os("LANGUAGE"))?;
            Some(parse_default_languages(&value.to_string_lossy()))
        })
        .as_deref()
}

static SAMPLE_LANGUAGES: [&str; 88] = [
    "", "", "ar", "hy", "bn", "", "chr", "cop", "ru", "", "hi", "am", "ka", "", "el", "gu", "pa",
    "", "ko", "he", "ja", "kn", "ja", "km", "lo", "en", "ml", "mn", "my", "", "", "or", "", "si",
    "syr", "ta", "te", "dv", "th", "bo", "iu", "", "tl", "hnn", "bku", "tbw", "", "", "", "", "",
    "", "", "uga", "", "bug", "", "", "syl", "peo", "", "", "", "", "", "", "nqo", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
];

pub fn script_get_sample_language(script: Script) -> Option<Language> {
    let index = usize::try_from(script.0).ok()?;
    let sample_language = *SAMPLE_LANGUAGES.get(index)?;

    let preferred = default_languages().and_then(|languages| {
        languages
            .iter()
            .copied()
            .find(|&language| language.includes_script(script))
    });
    if preferred.is_some() {
        return preferred;
    }

    (!sample_language.is_empty()).then(|| Language::from_string(sample_language))
}
