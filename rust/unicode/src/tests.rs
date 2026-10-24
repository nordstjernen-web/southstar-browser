//! Southstar unicode — unit tests for the property lookups and case mappings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{
    Category, category, combining_class, decompose, fold_string, is_alpha, is_control, is_print,
    is_space, is_wide, is_zero_width, lower_string, script, script_tag, to_lower, to_title,
    to_upper, upper_string,
};

#[test]
fn categories() {
    assert_eq!(category(u32::from('a')), Category::LowercaseLetter);
    assert_eq!(category(u32::from('A')), Category::UppercaseLetter);
    assert_eq!(category(0x0378), Category::Unassigned);
    assert_eq!(category(0xe000), Category::PrivateUse);
    assert_eq!(category(0xd800), Category::Surrogate);
    assert_eq!(category(0x4e00), Category::OtherLetter);
}

#[test]
fn widths_and_spaces() {
    assert!(is_wide(0x4e00));
    assert!(is_wide(0x3400 + 0x10));
    assert!(!is_wide(u32::from('a')));
    assert!(is_zero_width(0x0301));
    assert!(is_zero_width(0x200b));
    assert!(!is_zero_width(0x00ad));
    assert!(is_space(u32::from('\t')));
    assert!(is_space(0x3000));
    assert!(!is_space(0x200b));
}

#[test]
fn scripts_and_classes() {
    assert_eq!(
        script_tag(script(u32::from('a'))),
        u32::from_be_bytes(*b"Latn")
    );
    assert_eq!(script_tag(script(0x0627)), u32::from_be_bytes(*b"Arab"));
    assert_eq!(
        script_tag(script(u32::from(' '))),
        u32::from_be_bytes(*b"Zyyy")
    );
    assert_eq!(combining_class(0x0301), 230);
    assert_eq!(decompose(0x00e9), Some((u32::from('e'), 0x0301)));
    assert_eq!(decompose(0xac01), Some((0xac00, 0x11a8)));
    assert_eq!(decompose(0xac00), Some((0x1100, 0x1161)));
}

#[test]
fn case_mappings() {
    assert_eq!(to_upper(u32::from('a')), u32::from('A'));
    assert_eq!(to_lower(u32::from('A')), u32::from('a'));
    assert_eq!(to_title(0x01c6), 0x01c5);
    assert_eq!(to_title(0x01c4), 0x01c5);
    assert_eq!(upper_string("straße"), "STRASSE");
    assert_eq!(lower_string("ΣΑΣ"), "σας");
    assert_eq!(lower_string("\u{130}"), "i\u{307}");
    assert_eq!(fold_string("Straße µ ﬁ ΣΑΣ"), "strasse μ fi σασ");
    assert_eq!(
        upper_string("\u{3ac}\u{345}\u{301}"),
        "\u{386}\u{301}\u{399}"
    );
}

#[test]
fn classes_of_characters() {
    assert!(is_alpha(u32::from('é')));
    assert!(!is_alpha(u32::from('1')));
    assert!(is_control(0x07));
    assert!(!is_print(0x200b));
    assert!(is_print(u32::from(' ')));
    assert!(!is_print(0x0378));
}
