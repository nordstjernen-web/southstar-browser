//! Southstar text — tests for paragraph layout: line breaking, extents, positions, iterators and ellipsization.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::fontmap::FontMap;
use crate::layout::{Layout, SharedContext, shared_context};
use crate::types::{EllipsizeMode, SCALE, WrapMode};

fn is_send<T: Send>() {}

fn context() -> SharedContext {
    shared_context(FontMap::default_for_thread().create_context())
}

#[test]
fn layout_is_send() {
    is_send::<Layout>();
    is_send::<SharedContext>();
}

#[test]
fn empty_layout_has_one_line() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    assert_eq!(layout.line_count(), 1);
    assert_eq!(layout.log_attrs().len(), 1);
    let line = layout.line(0).expect("first line");
    assert_eq!(line.start_index, 0);
    assert_eq!(line.length, 0);
    assert!(line.runs.is_empty());
}

#[test]
fn paragraphs_become_lines() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    layout.set_text("one\ntwo\u{2029}three");
    let starts: Vec<i32> = layout.lines().iter().map(|line| line.start_index).collect();
    assert_eq!(starts, vec![0, 4, 10]);
    assert_eq!(layout.text(), "one\ntwo\u{2029}three");
}

#[test]
fn wrapping_breaks_between_words() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    layout.set_text("hello world hello world hello world");
    let (full_width, _) = layout.size();
    layout.set_width(full_width / 2);
    layout.set_wrap(WrapMode::Word);
    assert!(layout.line_count() > 1);
    assert!(layout.is_wrapped());
    for line in layout.lines() {
        let start = line.start_index as usize;
        let text = &layout_text(&line)[start..start + line.length as usize];
        assert!(!text.starts_with(' '), "line starts with a space: {text:?}");
    }
}

fn layout_text<'a>(line: &crate::layout::LineRef<'a>) -> &'a str {
    line.layout().text()
}

#[test]
fn ellipsize_shortens_the_line() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    layout.set_text("a fairly long line of text that will not fit");
    layout.set_width(60 * SCALE);
    layout.set_ellipsize(EllipsizeMode::End);
    assert_eq!(layout.line_count(), 1);
    assert!(layout.is_ellipsized());
    assert!(layout.size().0 <= 60 * SCALE);
}

#[test]
fn index_and_position_round_trip() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    layout.set_text("abc def");
    let pos = layout.index_to_pos(4);
    let (inside, index, trailing) = layout.xy_to_index(pos.x + 1, pos.y + 1);
    assert!(inside);
    assert_eq!((index, trailing), (4, 0));
}

#[test]
fn iterator_walks_every_line() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    layout.set_text("a\nb\nc");
    let mut iter = layout.iter();
    let mut baselines = vec![iter.baseline()];
    while iter.next_line() {
        baselines.push(iter.baseline());
    }
    assert_eq!(baselines.len(), 3);
    assert!(baselines.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn user_data_slot_keeps_values() {
    let ctx = context();
    let mut layout = Layout::new(&ctx);
    assert_eq!(layout.css_line_height("lh"), None);
    layout.set_css_line_height("lh", 18.5);
    layout.set_text("x");
    assert_eq!(layout.css_line_height("lh"), Some(18.5));
}
