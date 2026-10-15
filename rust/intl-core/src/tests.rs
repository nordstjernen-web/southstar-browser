//! Southstar — unit tests for locale identifier validation, canonicalization, likely subtags, collation and casing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use crate::casing::{self, CaseRules};
use crate::collation::{self, Settings, Strength};
use crate::tag::{self, LanguageId, LocaleId};

fn canonical(tag: &str) -> Option<String> {
    tag::canonicalize_tag(tag)
}

fn id(tag: &str) -> LanguageId {
    LanguageId::parse(tag).expect("language id parses")
}

fn maximal(tag: &str) -> String {
    tag::maximize(&id(tag)).map_or_else(|| tag.to_owned(), |max| max.to_text())
}

fn minimal(tag: &str) -> String {
    tag::minimize(&id(tag)).to_text()
}

fn settings(strength: Strength) -> Settings {
    Settings {
        strength,
        ignore_punctuation: false,
        numeric: false,
        upper_first: false,
        umlaut_as_e: false,
    }
}

fn sorted(words: &[&str], settings: &Settings) -> Vec<String> {
    let mut list: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    list.sort_by(|a, b| collation::compare(a, b, settings));
    list
}

#[test]
fn rejects_structurally_invalid_tags() {
    let invalid = [
        "",
        "i",
        "x",
        "419",
        "u-ca-gregory",
        "hans-cmn-cn",
        "cmn-hans-cn-u-u",
        "de-gregory-gregory",
        "de-1996-1996",
        "es-Latn-latn",
        "pl-PL-pl",
        "de_DE",
        "en\u{0}",
        " en",
        "en-ß",
        "de-u",
        "de-u-ca-",
        "x-private",
        "i-klingon",
        "zh-min-nan",
        "no-bok",
        "en-t",
        "en-t-root",
        "en-t-en-latn-latn",
        "en-t-d0",
        "en-u-c0",
        "en-u-00",
    ];
    for tag in invalid {
        assert_eq!(canonical(tag), None, "{tag:?} must be rejected");
    }
}

#[test]
fn canonicalizes_case_and_order() {
    let cases = [
        ("DE-de", "de-DE"),
        ("IT-LATN-iT", "it-Latn-IT"),
        ("sl-rozaj-biske-1994", "sl-1994-biske-rozaj"),
        ("en-u-baz-a-bar-x-u-foo", "en-a-bar-u-baz-x-u-foo"),
        ("it-u-nu-latn-ca-gregory", "it-u-ca-gregory-nu-latn"),
        ("en-u-foo-bar-foo", "en-u-bar-foo"),
        ("en-u-ca-gregory-ca-buddhist", "en-u-ca-gregory"),
        ("DE-T-M0-DIN-K0-QWERTZ", "de-t-k0-qwertz-m0-din"),
        ("en-u-0c", "en-u-0c"),
    ];
    for (input, expected) in cases {
        assert_eq!(canonical(input).as_deref(), Some(expected), "{input}");
    }
}

#[test]
fn replaces_aliases() {
    let cases = [
        ("cmn-hans-cn", "zh-Hans-CN"),
        ("sgn-GR", "gss"),
        ("ji", "yi"),
        ("de-DD", "de-DE"),
        ("sh", "sr-Latn"),
        ("sh-Cyrl", "sr-Cyrl"),
        ("cnr-BA", "sr-BA"),
        ("art-lojban", "jbo"),
        ("art-lojban-fonipa", "jbo-fonipa"),
        ("ja-latn-hepburn-heploc", "ja-Latn-alalc97"),
        ("hy-arevela", "hy"),
        ("hy-arevmda", "hyw"),
        ("ru-SU", "ru-RU"),
        ("hy-SU", "hy-AM"),
        ("und-Armn-SU", "und-Armn-AM"),
        ("sr-CS", "sr-RS"),
        ("az-NT", "az-SA"),
        ("en-t-iw", "en-t-he"),
        (
            "und-Latn-t-und-hani-m0-names",
            "und-Latn-t-und-hani-m0-prprname",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(canonical(input).as_deref(), Some(expected), "{input}");
    }
}

#[test]
fn canonicalizes_unicode_extension_values() {
    let cases = [
        ("und-u-ca-ethiopic-amete-alem", "und-u-ca-ethioaa"),
        ("und-u-ca-islamicc", "und-u-ca-islamic-civil"),
        ("und-u-ks-primary", "und-u-ks-level1"),
        ("und-u-ms-imperial", "und-u-ms-uksystem"),
        ("und-u-tz-cnckg", "und-u-tz-cnsha"),
        ("und-u-tz-eire", "und-u-tz-iedub"),
        ("und-u-rg-no23", "und-u-rg-no50"),
        ("und-NO-u-sd-no23", "und-NO-u-sd-no50"),
        ("und-u-kb-yes", "und-u-kb"),
        ("und-u-kf-yes", "und-u-kf-yes"),
        ("en-u-ca-true", "en-u-ca"),
        ("en-t-m0-true", "en-t-m0-true"),
    ];
    for (input, expected) in cases {
        assert_eq!(canonical(input).as_deref(), Some(expected), "{input}");
    }
}

#[test]
fn reads_extension_keywords() {
    let locale = LocaleId::parse("de-DE-u-co-phonebk-kn-x-private").expect("parses");
    assert_eq!(locale.keyword("co"), Some("phonebk"));
    assert_eq!(locale.keyword("kn"), Some(""));
    assert_eq!(locale.keyword("nu"), None);
    assert_eq!(
        locale.without_unicode_extension().to_text(),
        "de-DE-x-private"
    );
}

#[test]
fn adds_likely_subtags() {
    let cases = [
        ("en", "en-Latn-US"),
        ("en-Shaw", "en-Shaw-GB"),
        ("en-Arab", "en-Arab-US"),
        ("en-FR", "en-Latn-FR"),
        ("it-Kana-CA", "it-Kana-CA"),
        ("und", "en-Latn-US"),
        ("und-Thai", "th-Thai-TH"),
        ("und-419", "es-Latn-419"),
        ("und-150", "en-Latn-150"),
        ("und-AT", "de-Latn-AT"),
        ("und-Cyrl-RO", "bg-Cyrl-RO"),
        ("und-AQ", "en-Latn-AQ"),
        ("zh-TW", "zh-Hant-TW"),
        ("sr-ME", "sr-Latn-ME"),
        ("jbo", "jbo-Latn-001"),
        ("xtg", "xtg"),
        ("abcdefgh", "abcdefgh"),
    ];
    for (input, expected) in cases {
        assert_eq!(maximal(input), expected, "{input}");
    }
}

#[test]
fn removes_likely_subtags() {
    let cases = [
        ("en-Latn-US", "en"),
        ("en-Shaw-GB", "en-Shaw"),
        ("en-Latn-GB", "en-GB"),
        ("th-Thai-TH", "th"),
        ("es-Latn-419", "es-419"),
        ("bg-Cyrl-RO", "bg-RO"),
        ("und-Latn-AQ", "en-AQ"),
        ("zh-Hant", "zh-TW"),
        ("zh-Hani", "zh-Hani"),
        ("aae-Thai-CO", "aae-Thai-CO"),
        ("und-CW", "pap"),
    ];
    for (input, expected) in cases {
        assert_eq!(minimal(input), expected, "{input}");
    }
}

#[test]
fn collates_in_root_order() {
    let words = [
        "L", "X", "C", "k", "Z", "H", "d", "m", "w", "A", "i", "f", "y", "E", "N", "V", "g",
    ];
    let expected = [
        "A", "C", "d", "E", "f", "g", "H", "i", "k", "L", "m", "N", "V", "w", "X", "y", "Z",
    ];
    assert_eq!(sorted(&words, &settings(Strength::Variant)), expected);
    let mixed = ["b", "ä", "a", "A", "ß", "ss", "-", "1", "á", "à"];
    assert_eq!(
        sorted(&mixed, &settings(Strength::Variant)),
        ["-", "1", "a", "A", "á", "à", "ä", "b", "ss", "ß"]
    );
}

#[test]
fn compares_at_each_strength() {
    let equal = |strength, a: &str, b: &str| {
        collation::compare(a, b, &settings(strength)) == Ordering::Equal
    };
    assert!(equal(Strength::Base, "Aa", "áÁ"));
    assert!(!equal(Strength::Accent, "Aa", "áÁ"));
    assert!(equal(Strength::Accent, "Aa", "aA"));
    assert!(equal(Strength::Case, "Aa", "Aã"));
    assert!(!equal(Strength::Case, "Aa", "aA"));
    assert!(equal(Strength::Variant, "o\u{308}", "ö"));
    assert!(equal(Strength::Variant, "\u{1111}\u{1171}\u{11b6}", "퓛"));
}

#[test]
fn collates_numbers_punctuation_and_phonebook() {
    let numeric = Settings {
        numeric: true,
        ..settings(Strength::Variant)
    };
    assert_eq!(
        sorted(&["item10", "item9", "item010", "item1"], &numeric),
        ["item1", "item9", "item10", "item010"]
    );
    let ignoring = Settings {
        ignore_punctuation: true,
        ..settings(Strength::Variant)
    };
    assert_eq!(collation::compare("", " ", &ignoring), Ordering::Equal);
    assert_eq!(collation::compare("", "*", &ignoring), Ordering::Equal);
    let phonebook = Settings {
        umlaut_as_e: true,
        ..settings(Strength::Variant)
    };
    assert_eq!(
        sorted(&["off", "Ä", "A", "ö", "Af", "b", "od", "Ab"], &phonebook),
        ["A", "Ab", "Ä", "Af", "b", "od", "ö", "off"]
    );
}

#[test]
fn applies_turkic_and_lithuanian_casing() {
    assert_eq!(casing::to_lower("I\u{307}", CaseRules::Turkic), "i");
    assert_eq!(
        casing::to_lower("IA\u{307}", CaseRules::Turkic),
        "\u{131}a\u{307}"
    );
    assert_eq!(casing::to_upper("i", CaseRules::Turkic), "\u{130}");
    assert_eq!(
        casing::to_lower("I\u{300}", CaseRules::Lithuanian),
        "i\u{307}\u{300}"
    );
    assert_eq!(
        casing::to_lower("\u{cc}", CaseRules::Lithuanian),
        "i\u{307}\u{300}"
    );
    assert_eq!(casing::to_upper("i\u{307}", CaseRules::Lithuanian), "I");
    assert_eq!(casing::to_upper("i\u{307}", CaseRules::Default), "I\u{307}");
    assert_eq!(CaseRules::for_language("az"), CaseRules::Turkic);
}
