//! Southstar text — unit tests for language tags, range matching, sample strings and scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::*;

#[test]
fn canonicalizes_and_interns() {
    let a = Language::from_string("en_US.UTF-8");
    let b = Language::from_string("EN-us");
    assert_eq!(a.as_str(), "en-us");
    assert_eq!(a, b);
    assert_eq!(Language::from_string("sr@latin").as_str(), "sr-latin");
    assert_eq!(Language::from_string("").as_str(), "");
}

#[test]
fn matches_ranges() {
    let lang = Language::from_string("de-ch");
    assert!(lang.matches("de"));
    assert!(lang.matches("fr;de"));
    assert!(lang.matches("*"));
    assert!(!lang.matches("d"));
    assert!(!lang.matches("de-at, fr"));
    assert!(language_matches(None, "en;*"));
    assert!(!language_matches(None, "en"));
}

#[test]
fn sample_strings_and_scripts() {
    let pt_br = Language::from_string("pt-br");
    assert!(pt_br.sample_string().starts_with("Vejo galã"));
    assert_eq!(
        Language::from_string("xx").sample_string(),
        "The quick brown fox jumps over the lazy dog."
    );
    assert_eq!(
        Language::from_string("ja").scripts(),
        &[Script::HAN, Script::KATAKANA, Script::HIRAGANA]
    );
    assert!(Language::from_string("ru").includes_script(Script::CYRILLIC));
    assert!(!Language::from_string("ru").includes_script(Script::LATIN));
    assert!(Language::from_string("ru").includes_script(Script::COMMON));
}
