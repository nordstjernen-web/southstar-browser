//! Southstar text — unit tests for font description parsing, printing, merging and hashing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::*;

#[test]
fn round_trips_common_strings() {
    for (input, output) in [
        ("Sans Bold Italic 12", "Sans Bold Italic 12"),
        ("Cantarell 11", "Cantarell 11"),
        ("Serif,Monospace Bold 10.5px", "Serif,Monospace Bold 10.5px"),
        ("  DejaVu Sans ,  Arial , 9", "DejaVu Sans,Arial 9"),
        ("", "Normal"),
        ("Foo weight=450 12", "Foo weight=450 12"),
        ("Bar Bold,", "Bar Bold,"),
        ("Sans 12 @wght=200", "Sans 12 @wght=200"),
        ("Sans 12 #smcp", "Sans 12 #smcp"),
    ] {
        assert_eq!(
            FontDescription::from_string(input).to_string(),
            output,
            "{input}"
        );
    }
}

#[test]
fn merge_and_unset() {
    let mut desc = FontDescription::from_string("Sans 12");
    let other = FontDescription::from_string("Serif Bold 20");
    desc.merge(Some(&other), false);
    assert_eq!(desc.to_string(), "Sans 12");
    desc.merge(Some(&other), true);
    assert_eq!(desc.to_string(), "Serif Bold 20");
    desc.unset_fields(font_mask::FAMILY | font_mask::SIZE);
    assert_eq!(desc.to_string(), "Bold");
    assert_eq!(desc.family(), None);
}

#[test]
fn equality_ignores_family_case() {
    let a = FontDescription::from_string("SANS 12");
    let b = FontDescription::from_string("sans 12");
    assert!(a.equal(&b));
    assert_eq!(a.hash(), b.hash());
}
