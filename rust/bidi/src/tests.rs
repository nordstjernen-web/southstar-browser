//! Southstar bidi — unit tests, plus the Unicode conformance suites when SOUTHSTAR_UCD_DIR names a UCD directory.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{BidiClass, ParagraphDirection, bracket, class, mirror, resolve_paragraph};

fn levels_of(text: &str, direction: ParagraphDirection) -> (Vec<u8>, u8) {
    let code_points: Vec<u32> = text.chars().map(u32::from).collect();
    let classes: Vec<BidiClass> = code_points.iter().map(|&ch| class(ch)).collect();
    let brackets: Vec<_> = code_points.iter().map(|&ch| bracket(ch)).collect();
    let mut levels = vec![0; classes.len()];
    let resolved = resolve_paragraph(&classes, &brackets, direction, &mut levels);
    (levels, resolved.base_level)
}

#[test]
fn classes_come_from_the_ucd() {
    assert_eq!(class(u32::from('a')), BidiClass::L);
    assert_eq!(class(0x05d0), BidiClass::R);
    assert_eq!(class(0x0627), BidiClass::AL);
    assert_eq!(class(u32::from('1')), BidiClass::EN);
    assert_eq!(class(0x0661), BidiClass::AN);
    assert_eq!(class(u32::from(' ')), BidiClass::WS);
    assert_eq!(class(u32::from('\t')), BidiClass::S);
    assert_eq!(class(0x2029), BidiClass::B);
    assert_eq!(class(0x200d), BidiClass::BN);
    assert_eq!(class(0x2067), BidiClass::RLI);
    assert_eq!(class(0x05ff), BidiClass::R);
    assert_eq!(class(0x10ffff), BidiClass::BN);
}

#[test]
fn brackets_pair_through_canonical_equivalents() {
    let open = bracket(0x2329).unwrap();
    let close = bracket(0x3009).unwrap();
    assert!(open.opening);
    assert!(!close.opening);
    assert_eq!(open.pair, close.pair);
    assert_eq!(bracket(u32::from('a')), None);
    assert_eq!(mirror(u32::from('(')), Some(u32::from(')')));
}

#[test]
fn mixed_paragraphs_resolve() {
    let (levels, base) = levels_of("abc \u{5d0}\u{5d1}", ParagraphDirection::WeakLtr);
    assert_eq!(base, 0);
    assert_eq!(levels, [0, 0, 0, 0, 1, 1]);
    let (levels, base) = levels_of("\u{5d0} 12 ab", ParagraphDirection::WeakLtr);
    assert_eq!(base, 1);
    assert_eq!(levels, [1, 1, 2, 2, 1, 2, 2]);
    let (levels, _) = levels_of("\u{5d0}(a)\u{5d1}", ParagraphDirection::Rtl);
    assert_eq!(levels, [1, 1, 2, 1, 1]);
    let (levels, _) = levels_of("a \u{2067}\u{5d0}\u{2069} ", ParagraphDirection::Ltr);
    assert_eq!(levels, [0, 0, 0, 1, 0, 0]);
}

#[test]
fn removed_characters_take_the_previous_level() {
    let (levels, _) = levels_of("\u{202b}\u{5d0}\u{202c}a", ParagraphDirection::Ltr);
    assert_eq!(levels, [0, 1, 1, 0]);
}

fn parse_class(name: &str) -> BidiClass {
    match name {
        "L" => BidiClass::L,
        "R" => BidiClass::R,
        "AL" => BidiClass::AL,
        "EN" => BidiClass::EN,
        "ES" => BidiClass::ES,
        "ET" => BidiClass::ET,
        "AN" => BidiClass::AN,
        "CS" => BidiClass::CS,
        "NSM" => BidiClass::NSM,
        "BN" => BidiClass::BN,
        "B" => BidiClass::B,
        "S" => BidiClass::S,
        "WS" => BidiClass::WS,
        "ON" => BidiClass::ON,
        "LRE" => BidiClass::LRE,
        "LRO" => BidiClass::LRO,
        "RLE" => BidiClass::RLE,
        "RLO" => BidiClass::RLO,
        "PDF" => BidiClass::PDF,
        "LRI" => BidiClass::LRI,
        "RLI" => BidiClass::RLI,
        "FSI" => BidiClass::FSI,
        "PDI" => BidiClass::PDI,
        other => panic!("unknown bidi class {other}"),
    }
}

fn parse_levels(text: &str) -> Vec<Option<u8>> {
    text.split_whitespace()
        .map(|level| level.parse().ok())
        .collect()
}

fn levels_match(expected: &[Option<u8>], actual: &[u8]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(want, got)| want.is_none_or(|want| want == *got))
}

fn ucd_file(name: &str) -> Option<String> {
    let dir = std::env::var("SOUTHSTAR_UCD_DIR").ok()?;
    std::fs::read_to_string(std::path::Path::new(&dir).join(name)).ok()
}

#[test]
fn bidi_test_conformance() {
    let Some(data) = ucd_file("BidiTest.txt") else {
        return;
    };
    let mut expected = Vec::new();
    let (mut passed, mut failed) = (0usize, 0usize);
    for line in data.lines() {
        if let Some(rest) = line.strip_prefix("@Levels:") {
            expected = parse_levels(rest);
            continue;
        }
        if line.starts_with('#') || line.starts_with('@') || line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split(';');
        let classes: Vec<BidiClass> = fields
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .map(parse_class)
            .collect();
        let bitset: u32 = fields.next().unwrap_or_default().trim().parse().unwrap();
        let brackets = vec![None; classes.len()];
        for (bit, direction) in [
            (1, ParagraphDirection::WeakLtr),
            (2, ParagraphDirection::Ltr),
            (4, ParagraphDirection::Rtl),
        ] {
            if bitset & bit == 0 {
                continue;
            }
            let mut levels = vec![0; classes.len()];
            resolve_paragraph(&classes, &brackets, direction, &mut levels);
            if levels_match(&expected, &levels) {
                passed += 1;
            } else {
                failed += 1;
                if failed <= 10 {
                    eprintln!("BidiTest {line} ({direction:?}): want {expected:?}, got {levels:?}");
                }
            }
        }
    }
    eprintln!("BidiTest: {passed} passed, {failed} failed");
    assert_eq!(failed, 0);
}

#[test]
fn bidi_character_test_conformance() {
    let Some(data) = ucd_file("BidiCharacterTest.txt") else {
        return;
    };
    let (mut passed, mut failed) = (0usize, 0usize);
    for line in data.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        let code_points: Vec<u32> = fields[0]
            .split_whitespace()
            .map(|hex| u32::from_str_radix(hex, 16).unwrap())
            .collect();
        let direction = match fields[1].trim() {
            "0" => ParagraphDirection::Ltr,
            "1" => ParagraphDirection::Rtl,
            _ => ParagraphDirection::WeakLtr,
        };
        let want_base: u8 = fields[2].trim().parse().unwrap();
        let expected = parse_levels(fields[3]);
        let classes: Vec<BidiClass> = code_points.iter().map(|&ch| class(ch)).collect();
        let brackets: Vec<_> = code_points.iter().map(|&ch| bracket(ch)).collect();
        let mut levels = vec![0; classes.len()];
        let resolved = resolve_paragraph(&classes, &brackets, direction, &mut levels);
        if resolved.base_level == want_base && levels_match(&expected, &levels) {
            passed += 1;
        } else {
            failed += 1;
            if failed <= 10 {
                eprintln!(
                    "BidiCharacterTest {line}: want {want_base} {expected:?}, got {} {levels:?}",
                    resolved.base_level
                );
            }
        }
    }
    eprintln!("BidiCharacterTest: {passed} passed, {failed} failed");
    assert_eq!(failed, 0);
}
