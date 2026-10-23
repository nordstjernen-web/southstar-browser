//! Southstar text — unit tests for breaking, scripts, emoji runs, bidi levels and paragraph boundaries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::*;
use crate::bidi::{find_base_dir, log2vis_fill_embedding_levels, unichar_direction};
use crate::emoji::EmojiIter;
use crate::types::Direction;
use crate::utils::{find_paragraph_boundary, is_zero_width};

fn log_attrs(text: &str) -> Vec<LogAttr> {
    let mut attrs = vec![LogAttr::default(); text.chars().count() + 1];
    get_log_attrs(text, None, &mut attrs);
    attrs
}

#[test]
fn line_breaks_between_words() {
    let attrs = log_attrs("Hello world");
    assert!(attrs[6].is_line_break());
    assert!(!attrs[3].is_line_break());
    assert!(attrs[11].is_mandatory_break());
    assert!(attrs[0].is_word_start());
    assert!(attrs[5].is_word_end());
}

#[test]
fn empty_text_has_one_attr() {
    let attrs = log_attrs("");
    assert_eq!(attrs.len(), 1);
    assert!(attrs[0].is_cursor_position());
    assert!(attrs[0].is_mandatory_break());
}

#[test]
fn emoji_zwj_sequence_is_one_cluster() {
    let attrs = log_attrs("a\u{1F468}\u{200D}\u{1F469}b");
    assert!(attrs[1].is_cursor_position());
    assert!(!attrs[2].is_cursor_position());
    assert!(!attrs[3].is_cursor_position());
    assert!(attrs[4].is_cursor_position());
}

#[test]
fn script_runs_split_on_script_change() {
    let runs: Vec<_> = ScriptIter::new("abc \u{05D0}\u{05D1} (x)").collect();
    assert_eq!(runs[0], (0, 4, Script::LATIN));
    assert_eq!(runs[1].2, Script::HEBREW);
    assert_eq!(
        runs.last().map(|r| r.1),
        Some("abc \u{05D0}\u{05D1} (x)".len())
    );
}

#[test]
fn emoji_runs() {
    let mut iter = EmojiIter::new("ab\u{1F600}c");
    assert_eq!((iter.start(), iter.end(), iter.is_emoji()), (0, 2, false));
    assert!(iter.next_run());
    assert_eq!((iter.start(), iter.end(), iter.is_emoji()), (2, 6, true));
    assert!(iter.next_run());
    assert!(!iter.next_run());
}

#[test]
fn bidi_levels() {
    let mut dir = Direction::Neutral;
    let mut levels = vec![9; 3];
    log2vis_fill_embedding_levels("abc", &mut levels, &mut dir);
    assert_eq!((levels, dir), (vec![0, 0, 0], Direction::Ltr));

    let mut dir = Direction::Ltr;
    let mut levels = vec![9; 5];
    log2vis_fill_embedding_levels("ab \u{05D0}\u{05D1}", &mut levels, &mut dir);
    assert_eq!(levels, vec![0, 0, 0, 1, 1]);
    assert_eq!(unichar_direction(0x05D0), Direction::Rtl);
    assert_eq!(find_base_dir("12 \u{05D0}"), Direction::Rtl);
}

#[test]
fn paragraph_boundaries() {
    assert_eq!(find_paragraph_boundary("ab\r\ncd"), (2, 4));
    assert_eq!(find_paragraph_boundary("ab"), (2, 2));
    assert_eq!(find_paragraph_boundary("ab\n"), (2, 3));
    assert!(is_zero_width(0x200B));
    assert!(!is_zero_width(0x20));
}

#[test]
fn allow_breaks_false_removes_breaks() {
    let text = "aa bb cc";
    let mut attrs = log_attrs(text);
    let spans = [AttrSpan {
        start_index: 0,
        end_index: 5,
        value: 0,
    }];
    attr_break(
        text,
        &BreakAttrSpans {
            allow_breaks: &spans,
            ..BreakAttrSpans::default()
        },
        0,
        &mut attrs,
    );
    assert!(!attrs[3].is_line_break());
    assert!(attrs[6].is_line_break());
}
