//! Southstar text — tests for items, itemization, glyph strings, glyph items, shaping and the shape cache's segmentation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{AttrList, attr_foreground_new, attr_weight_new};
use crate::fontmap::FontMap;
use crate::glyph::{GlyphGeometry, GlyphInfo, GlyphString, GlyphVisAttr};
use crate::glyph_item::{GlyphItem, GlyphItemIter};
use crate::item::{Analysis, Item, analysis_flags};
use crate::itemize::{itemize, itemize_post_process_items, itemize_with_font};
use crate::shape::{is_default_ignorable, shape_item, shape_sample};
use crate::shape_cache::shape_cache_split;
use crate::types::{Direction, Weight, shape_flags};
use crate::utf8::{char_at, g_utf8_strlen, next_char, pango_utf8_strlen, pointer_to_offset};

fn is_send_and_sync<T: Send + Sync>() {}

fn glyphs(widths: &[i32], clusters: &[i32]) -> GlyphString {
    GlyphString {
        glyphs: widths
            .iter()
            .enumerate()
            .map(|(i, &width)| GlyphInfo {
                glyph: i as u32 + 1,
                geometry: GlyphGeometry {
                    width,
                    x_offset: 0,
                    y_offset: 0,
                },
                attr: GlyphVisAttr {
                    is_cluster_start: true,
                    is_color: false,
                },
            })
            .collect(),
        log_clusters: clusters.to_vec(),
    }
}

fn item(offset: i32, length: i32, num_chars: i32, level: u8) -> Item {
    let mut item = Item::new();
    item.offset = offset;
    item.length = length;
    item.num_chars = num_chars;
    item.analysis.level = level;
    item
}

#[test]
fn shaped_data_crosses_threads() {
    is_send_and_sync::<Item>();
    is_send_and_sync::<GlyphString>();
    is_send_and_sync::<GlyphItem>();
}

#[test]
fn steps_through_utf8_like_glib() {
    let text = "aé日😀".as_bytes();
    assert_eq!(next_char(text, 0), 1);
    assert_eq!(next_char(text, 1), 3);
    assert_eq!(next_char(text, 2), 3);
    assert_eq!(char_at(text, 3), 0x65E5);
    assert_eq!(char_at(text, 4), u32::MAX);
    assert_eq!(g_utf8_strlen(text, text.len()), 4);
    assert_eq!(g_utf8_strlen(text, 5), 2);
    assert_eq!(pango_utf8_strlen(text, 6), 3);
    assert_eq!(pointer_to_offset(text, 0, 6), 3);
    assert_eq!(g_utf8_strlen(b"a\0b", 3), 1);
    assert_eq!(pango_utf8_strlen(b"a\0b", 3), 3);
}

#[test]
fn splits_and_unsplits_items() {
    let mut whole = item(4, 10, 6, 0);
    whole.char_offset = 2;
    let first = whole.split(3, 2).expect("valid split");
    assert_eq!((first.offset, first.length, first.num_chars), (4, 3, 2));
    assert_eq!(first.char_offset, 2);
    assert_eq!((whole.offset, whole.length, whole.num_chars), (7, 7, 4));
    assert_eq!(whole.char_offset, 4);
    assert!(whole.split(0, 1).is_none());
    assert!(whole.split(7, 1).is_none());
    whole.unsplit(3, 2);
    assert_eq!((whole.offset, whole.length, whole.num_chars), (4, 10, 6));
    assert_eq!(whole.char_offset, 2);
    assert_ne!(whole.analysis.flags & analysis_flags::HAS_CHAR_OFFSET, 0);
}

#[test]
fn applies_attributes_to_items() {
    let mut list = AttrList::new();
    list.insert(attr_foreground_new(1, 2, 3).with_range(0, 3));
    list.insert(attr_weight_new(Weight::BOLD).with_range(2, 8));
    let mut iter = list.iterator();
    let mut first = item(0, 4, 4, 0);
    first.apply_attrs(&mut iter);
    assert_eq!(first.analysis.extra_attrs.len(), 2);
    let mut second = item(4, 4, 4, 0);
    second.apply_attrs(&mut iter);
    assert_eq!(second.analysis.extra_attrs.len(), 1);
}

#[test]
fn maps_between_x_and_index() {
    let analysis = Analysis::default();
    let string = glyphs(&[1000, 1000, 2000], &[0, 1, 2]);
    assert_eq!(string.get_width(), 4000);
    assert_eq!(string.x_to_index("abcd", &analysis, 500), (0, true));
    assert_eq!(string.x_to_index("abcd", &analysis, 1200), (1, false));
    assert_eq!(string.x_to_index("abcd", &analysis, 3500), (3, true));
    assert_eq!(string.index_to_x("abcd", &analysis, 1, false), 1000);
    assert_eq!(string.index_to_x("abcd", &analysis, 3, false), 3000);
    assert_eq!(string.index_to_x("abcd", &analysis, 3, true), 4000);

    let rtl = Analysis {
        level: 1,
        ..Analysis::default()
    };
    let reversed = glyphs(&[2000, 1000, 1000], &[2, 1, 0]);
    assert_eq!(reversed.index_to_x("abcd", &rtl, 0, false), 4000);
    assert_eq!(reversed.index_to_x("abcd", &rtl, 0, true), 3000);
    assert_eq!(reversed.x_to_index("abcd", &rtl, 3500), (0, false));
}

#[test]
fn walks_and_splits_glyph_items() {
    let text = "abcd";
    let mut glyph_item = GlyphItem::new(item(0, 4, 4, 0), glyphs(&[10, 10, 10, 10], &[0, 0, 2, 3]));
    let mut iter = GlyphItemIter::default();
    let mut clusters = Vec::new();
    let mut have = iter.init_start(&glyph_item, text);
    while have {
        clusters.push((
            iter.start_index,
            iter.end_index,
            iter.start_char,
            iter.end_char,
        ));
        have = iter.next_cluster(&glyph_item, text);
    }
    assert_eq!(clusters, vec![(0, 2, 0, 2), (2, 3, 2, 3), (3, 4, 3, 4)]);

    let mut widths = [0; 4];
    glyph_item.get_logical_widths(text, &mut widths);
    assert_eq!(widths, [10, 10, 10, 10]);

    let first = glyph_item
        .split(text, 1)
        .expect("splits at the next cluster");
    assert_eq!((first.item.offset, first.item.length), (0, 2));
    assert_eq!(first.glyphs.glyphs.len(), 2);
    assert_eq!((glyph_item.item.offset, glyph_item.item.length), (2, 2));
    assert_eq!(glyph_item.glyphs.log_clusters, vec![0, 1]);
}

#[test]
fn letter_and_word_space_glyph_items() {
    let text = "a b";
    let log_attrs: Vec<crate::types::LogAttr> = (0..4)
        .map(|_| {
            let mut attr = crate::types::LogAttr::default();
            attr.set_is_cursor_position(true);
            attr
        })
        .collect();
    let mut glyph_item = GlyphItem::new(item(0, 3, 3, 0), glyphs(&[10, 5, 10], &[0, 1, 2]));
    glyph_item.letter_space(text, &log_attrs, 2);
    let widths: Vec<i32> = glyph_item
        .glyphs
        .glyphs
        .iter()
        .map(|g| g.geometry.width)
        .collect();
    assert_eq!(widths, vec![12, 7, 12]);
    glyph_item.word_space(text, 100);
    assert_eq!(glyph_item.glyphs.glyphs[1].geometry.width, 107);
}

#[test]
fn cuts_items_after_spaces() {
    let text = "one two  three";
    assert_eq!(
        shape_cache_split(text.as_bytes(), 0, text.len(), 512),
        vec![0, 4, 9]
    );
    assert_eq!(shape_cache_split(text.as_bytes(), 4, 5, 512), vec![0]);
    assert!(shape_cache_split(b"a b c", 0, 5, 2).is_empty());
    assert!(is_default_ignorable(0x200B));
    assert!(!is_default_ignorable(u32::from('a')));
}

#[test]
fn itemizes_and_shapes_with_installed_fonts() {
    let map = FontMap::new();
    let mut context = map.create_context();
    let text = "Hello שלום 日本語";
    let items = itemize(&mut context, text, 0, text.len(), None, None);
    assert!(!items.is_empty());
    let mut covered = 0;
    let mut chars = 0;
    for item in &items {
        assert_eq!(item.offset, covered);
        assert_eq!(item.char_offset, chars);
        covered += item.length;
        chars += item.num_chars;
    }
    assert_eq!(covered as usize, text.len());
    assert!(items.iter().any(|item| item.analysis.level & 1 == 1));

    let again = itemize(&mut context, text, 0, text.len(), None, None);
    assert_eq!(again.len(), items.len());

    let mut with_font = itemize_with_font(
        &mut context,
        Direction::Ltr,
        text,
        0,
        text.len(),
        None,
        None,
        None,
    );
    itemize_post_process_items(&context, text, None, &mut with_font);
    assert_eq!(with_font.len(), items.len());

    for item in &items {
        if item.analysis.font.is_none() {
            continue;
        }
        let mut glyphs = GlyphString::new();
        shape_item(item, text, None, &mut glyphs, shape_flags::ROUND_POSITIONS);
        assert!(!glyphs.glyphs.is_empty());
        assert!(glyphs.glyphs[0].attr.is_cluster_start);
        for &cluster in &glyphs.log_clusters {
            assert!(cluster >= 0 && cluster < item.length);
        }
    }

    let desc = context.font_description().clone();
    let sample = shape_sample(&mut context, &desc, "abc");
    assert!(!sample.is_empty());
}
