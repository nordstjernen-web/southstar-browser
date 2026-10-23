//! Southstar text — tests for fonts, the font map, fontsets and the context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::context::Context;
use crate::font::{get_ignorable_size, parse_variations, utf8_strwidth};
use crate::font_description::FontDescription;
use crate::fontmap::{FontMap, combine_features, features_get_variant};
use crate::language::Language;
use crate::types::{GLYPH_EMPTY, Gravity, Variant, get_unknown_glyph};

fn is_send_and_sync<T: Send + Sync>() {}

fn is_send<T: Send>() {}

#[test]
fn shares_fonts_between_threads() {
    is_send_and_sync::<crate::font::Font>();
    is_send_and_sync::<FontMap>();
    is_send_and_sync::<crate::fontset::Fontset>();
    is_send::<Context>();
}

#[test]
fn combines_variant_features() {
    assert_eq!(combine_features(Variant::Normal, None), "");
    assert_eq!(combine_features(Variant::SmallCaps, None), "smcp=1");
    assert_eq!(
        combine_features(Variant::AllSmallCaps, Some("liga=0")),
        "smcp=1,c2sc=1,liga=0"
    );
    assert_eq!(
        combine_features(Variant::AllPetiteCaps, Some("pcap=0")),
        "c2pc=1,pcap=0"
    );
    assert_eq!(combine_features(Variant::TitleCaps, Some("")), "titl=1,");
}

#[test]
fn reads_variant_back_from_features() {
    assert_eq!(features_get_variant(None), Variant::Normal);
    assert_eq!(features_get_variant(Some("smcp=1")), Variant::SmallCaps);
    assert_eq!(
        features_get_variant(Some("smcp=1,c2sc=1")),
        Variant::AllSmallCaps
    );
    assert_eq!(
        features_get_variant(Some("c2sc=1,smcp=1")),
        Variant::AllSmallCaps
    );
    assert_eq!(
        features_get_variant(Some("c2pc=1,pcap=1")),
        Variant::AllPetiteCaps
    );
    assert_eq!(
        features_get_variant(Some("kern=0,unic=1")),
        Variant::Unicase
    );
}

#[test]
fn sizes_ignorable_hex_boxes() {
    assert_eq!(get_ignorable_size(0x00ad), Some(("SHY", 1, 3)));
    assert_eq!(get_ignorable_size(0x200c), Some(("ZWNJ", 2, 2)));
    assert_eq!(get_ignorable_size(0xfeff), Some(("ZWNBS", 2, 3)));
    assert_eq!(get_ignorable_size(0x0041), None);
}

#[test]
fn measures_string_width_in_cells() {
    assert_eq!(utf8_strwidth("ab"), 2);
    assert_eq!(utf8_strwidth("a\u{4e2d}"), 3);
    assert_eq!(utf8_strwidth("a\u{200b}"), 1);
}

#[test]
fn parses_variations_onto_axes() {
    let axes = [
        (u32::from_be_bytes(*b"wght"), 1),
        (u32::from_be_bytes(*b"wdth"), 0),
    ];
    let mut coords = [100.0f32, 400.0];
    parse_variations("wght=700,bogus,wdth=75.5,opsz=12", &axes, &mut coords);
    assert_eq!(coords, [75.5, 700.0]);
}

#[test]
fn context_tracks_changes_through_its_serial() {
    let map = FontMap::new();
    let mut context = map.create_context();
    let serial = context.serial();
    context.set_language(Some(Language::from_string("fr")));
    assert_ne!(context.serial(), serial);
    let serial = context.serial();
    context.set_language(Some(Language::from_string("fr")));
    assert_eq!(context.serial(), serial);
    map.changed();
    assert_ne!(context.serial(), serial);
    assert_eq!(Context::new().font_description().family(), Some("serif"));
    assert_eq!(context.gravity(), Gravity::South);
}

#[test]
fn default_font_map_is_per_thread() {
    let here = FontMap::default_for_thread();
    assert!(here.ptr_eq(&FontMap::default_for_thread()));
    let there = std::thread::spawn(|| FontMap::default_for_thread().serial())
        .join()
        .unwrap();
    assert_eq!(there, here.serial());
}

#[test]
fn loads_fonts_and_their_extents() {
    let map = FontMap::new();
    let context = map.create_context();
    let desc = FontDescription::from_string("Sans 12");
    let Some(font) = context.load_font(&desc) else {
        return;
    };
    assert!(
        map.load_font(Some(&context), &desc)
            .is_some_and(|again| again == font)
    );
    let (ink, logical) = font.glyph_extents(GLYPH_EMPTY);
    assert_eq!(ink.width, 0);
    assert!(logical.height > 0);
    let (_, missing) = font.glyph_extents(get_unknown_glyph(0x4e2d));
    assert!(missing.width > 0);
    let metrics = font.metrics(None);
    assert!(metrics.ascent > 0);
    let fontset = context.load_fontset(&desc, None).expect("fontset");
    assert!(fontset.get_font(u32::from('A')).is_some());
    let serial = map.serial();
    map.config_changed();
    assert_ne!(map.serial(), serial);
}
