//! Southstar text — text boundaries: the default break pass, script tailoring, attribute customization and per-paragraph log attributes.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

mod cache;
mod default;
mod table;
mod tailor;

use crate::script::{Script, ScriptIter};
use crate::types::LogAttr;
use crate::unicode::{self, BreakType};
use crate::utils::caches_verifying;

pub use cache::break_cache_stats;

#[cfg(test)]
#[path = "tests/breaks.rs"]
mod tests;

fn merge_boundary(attrs: &mut [LogAttr], before: LogAttr) {
    let start = &mut attrs[0];
    start.set_is_line_break(start.is_line_break() || before.is_line_break());
    start.set_is_mandatory_break(start.is_mandatory_break() || before.is_mandatory_break());
    start.set_is_cursor_position(start.is_cursor_position() || before.is_cursor_position());
}

fn enforce_boundary(attrs: &mut [LogAttr], before: LogAttr) {
    attrs[0].set_backspace_deletes_character(before.backspace_deletes_character());
    merge_boundary(attrs, before);
}

pub fn default_break(text: &str, attrs: &mut [LogAttr]) {
    let before = attrs[0];

    if !cache::fill(text, attrs) {
        default::default_break(text, attrs);
        cache::store(text, attrs);
    } else if caches_verifying() {
        let n = unicode::utf8_strlen(text) + 1;
        let mut fresh = vec![LogAttr::default(); n];
        default::default_break(text, &mut fresh);
        if attrs.get(..n) != Some(&fresh[..]) {
            eprintln!("break cache mismatch on '{text}'");
        }
    }

    merge_boundary(attrs, before);
}

pub fn tailor_break(
    paragraph: &str,
    start: usize,
    end: usize,
    script: Script,
    language: Option<&str>,
    attrs: &mut [LogAttr],
) {
    let before = attrs[0];
    if tailor::break_script(paragraph, start, end, script, language, attrs) {
        enforce_boundary(attrs, before);
    }
}

pub fn get_log_attrs(text: &str, language: Option<&str>, attrs: &mut [LogAttr]) {
    default_break(text, attrs);

    let mut chars_broken = 0;
    for (start, end, script) in ScriptIter::new(text) {
        let chars_in_range = unicode::utf8_strlen(&text[start..end]);
        tailor_break(
            text,
            start,
            end,
            script,
            language,
            &mut attrs[chars_broken..=chars_broken + chars_in_range],
        );
        chars_broken += chars_in_range;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrSpan {
    pub start_index: u32,
    pub end_index: u32,
    pub value: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BreakAttrSpans<'a> {
    pub words: &'a [AttrSpan],
    pub sentences: &'a [AttrSpan],
    pub insert_hyphens: &'a [AttrSpan],
    pub allow_breaks: &'a [AttrSpan],
}

struct Positions<'a> {
    text: &'a str,
    offset: i64,
    length: i64,
    attrs_len: i64,
}

impl Positions<'_> {
    fn pointer_to_offset(&self, byte: i64) -> i64 {
        if byte < 0 {
            -1
        } else if byte > self.length {
            unicode::utf8_strlen(self.text) as i64 + 1
        } else {
            unicode::byte_to_char_offset(self.text, byte.unsigned_abs() as usize) as i64
        }
    }

    fn start_pos(&self, start: i64) -> i64 {
        if start < self.offset {
            0
        } else {
            self.pointer_to_offset(start - self.offset)
        }
    }

    fn end_pos(&self, end: i64) -> i64 {
        if end >= self.offset + self.length {
            self.attrs_len
        } else {
            self.pointer_to_offset(end - self.offset)
        }
    }
}

fn span_bounds(span: &AttrSpan) -> (i64, i64) {
    (
        i64::from(span.start_index as i32),
        i64::from(span.end_index as i32),
    )
}

fn attr_at(attrs: &mut [LogAttr], pos: i64) -> Option<&mut LogAttr> {
    usize::try_from(pos)
        .ok()
        .and_then(|index| attrs.get_mut(index))
}

fn attr_value(attrs: &[LogAttr], pos: i64) -> LogAttr {
    usize::try_from(pos)
        .ok()
        .and_then(|index| attrs.get(index).copied())
        .unwrap_or_default()
}

fn remove_breaks_from_range(
    text: &str,
    start: usize,
    log_attrs: &mut [LogAttr],
    start_pos: i64,
    end_pos: i64,
) {
    let mut after_zws = false;
    let mut after_hyphen = false;
    let mut p = unicode::next_char_index(text, start);
    let mut pos = start_pos + 1;
    while pos < end_pos {
        let ch = unicode::char_at(text, p);
        let bt = default::line_break_class(ch);
        if let Some(attr) = attr_at(log_attrs, pos) {
            if !attr.is_mandatory_break() {
                attr.set_is_line_break(false);
            }
            if after_hyphen {
                attr.set_is_line_break(true);
            }
            if after_zws && bt != BreakType::Space {
                attr.set_is_line_break(true);
            }
        }

        after_hyphen = matches!(
            ch,
            0x00ad
                | 0x05A0
                | 0x2010
                | 0x2012
                | 0x2013
                | 0x0F0B
                | 0x1361
                | 0x17D8
                | 0x17DA
                | 0x2027
                | 0x007C
        );
        after_zws = bt == BreakType::ZeroWidthSpace || (bt == BreakType::Space && after_zws);

        pos += 1;
        p = unicode::next_char_index(text, p);
    }
}

fn handle_allow_breaks(
    positions: &Positions<'_>,
    spans: &[AttrSpan],
    log_attrs: &mut [LogAttr],
) -> bool {
    let mut tailored = false;
    for span in spans {
        if span.value != 0 {
            continue;
        }
        let (start, end) = span_bounds(span);
        let start_pos = positions.start_pos(start);
        let end_pos = positions.end_pos(end);
        for pos in start_pos + 1..end_pos {
            if let Some(attr) = attr_at(log_attrs, pos) {
                attr.set_is_char_break(false);
                if !attr.is_mandatory_break() {
                    attr.set_is_line_break(false);
                }
            }
        }
        tailored = true;
    }
    tailored
}

fn handle_words(positions: &Positions<'_>, spans: &[AttrSpan], log_attrs: &mut [LogAttr]) -> bool {
    let mut tailored = false;
    let log_attrs_len = positions.attrs_len;
    for span in spans {
        let (start, end) = span_bounds(span);
        let start_pos = positions.start_pos(start);
        let end_pos = positions.end_pos(end);

        for pos in start_pos + 1..end_pos {
            if let Some(attr) = attr_at(log_attrs, pos) {
                attr.set_is_word_start(false);
                attr.set_is_word_end(false);
                attr.set_is_word_boundary(false);
            }
        }

        let range_start = (start - positions.offset).max(0).unsigned_abs() as usize;
        remove_breaks_from_range(positions.text, range_start, log_attrs, start_pos, end_pos);

        if start >= positions.offset {
            let mut in_word = false;
            let mut pos = start_pos;
            while pos >= 0 {
                let attr = attr_value(log_attrs, pos);
                if attr.is_word_end() {
                    in_word = pos == start_pos;
                    break;
                }
                if pos < start_pos && attr.is_word_start() {
                    in_word = true;
                    break;
                }
                pos -= 1;
            }
            if let Some(attr) = attr_at(log_attrs, start_pos) {
                attr.set_is_word_start(true);
                attr.set_is_word_end(in_word);
                attr.set_is_word_boundary(true);
                if start_pos > 0 {
                    attr.set_is_line_break(true);
                }
            }
            tailored = true;
        }

        if end < positions.offset + positions.length {
            let mut in_word = false;
            let mut pos = end_pos;
            while pos < log_attrs_len {
                let attr = attr_value(log_attrs, pos);
                if attr.is_word_start() {
                    in_word = pos == end_pos;
                    break;
                }
                if pos > end_pos && attr.is_word_end() {
                    in_word = true;
                    break;
                }
                pos += 1;
            }
            if let Some(attr) = attr_at(log_attrs, end_pos) {
                attr.set_is_word_start(in_word);
                attr.set_is_word_end(true);
                attr.set_is_word_boundary(true);
                if in_word {
                    attr.set_is_line_break(true);
                }
            }
            tailored = true;
        }
    }
    tailored
}

fn handle_sentences(
    positions: &Positions<'_>,
    spans: &[AttrSpan],
    log_attrs: &mut [LogAttr],
) -> bool {
    let mut tailored = false;
    let log_attrs_len = positions.attrs_len;
    for span in spans {
        let (start, end) = span_bounds(span);
        let start_pos = positions.start_pos(start);
        let end_pos = positions.end_pos(end);

        for pos in start_pos + 1..end_pos {
            if let Some(attr) = attr_at(log_attrs, pos) {
                attr.set_is_sentence_start(false);
                attr.set_is_sentence_end(false);
                attr.set_is_sentence_boundary(false);
            }
            tailored = true;
        }

        if start >= positions.offset {
            let mut in_sentence = false;
            let mut pos = start_pos - 1;
            while pos >= 0 {
                let attr = attr_value(log_attrs, pos);
                if attr.is_sentence_end() {
                    break;
                }
                if attr.is_sentence_start() {
                    in_sentence = true;
                    break;
                }
                pos -= 1;
            }
            if let Some(attr) = attr_at(log_attrs, start_pos) {
                attr.set_is_sentence_start(true);
                attr.set_is_sentence_end(in_sentence);
                attr.set_is_sentence_boundary(true);
            }
            tailored = true;
        }

        if end < positions.offset + positions.length {
            let mut in_sentence = false;
            let mut pos = end_pos + 1;
            while pos < log_attrs_len {
                let attr = attr_value(log_attrs, pos);
                if attr.is_sentence_start() {
                    break;
                }
                if attr.is_sentence_end() {
                    in_sentence = true;
                    break;
                }
                pos += 1;
            }
            if let Some(attr) = attr_at(log_attrs, end_pos) {
                attr.set_is_sentence_start(in_sentence);
                attr.set_is_sentence_end(true);
                attr.set_is_sentence_boundary(true);
            }
            tailored = true;
        }
    }
    tailored
}

fn handle_hyphens(
    positions: &Positions<'_>,
    spans: &[AttrSpan],
    log_attrs: &mut [LogAttr],
) -> bool {
    let mut tailored = false;
    for span in spans {
        if span.value != 0 {
            continue;
        }
        let (start, end) = span_bounds(span);
        let start_pos = positions.start_pos(start);
        let end_pos = positions.end_pos(end);
        for pos in start_pos + 1..end_pos {
            if let Some(attr) = attr_at(log_attrs, pos)
                && !attr.break_removes_preceding()
            {
                attr.set_break_inserts_hyphen(false);
                tailored = true;
            }
        }
    }
    tailored
}

fn break_attrs(
    text: &str,
    spans: &BreakAttrSpans<'_>,
    offset: i32,
    log_attrs: &mut [LogAttr],
) -> bool {
    let positions = Positions {
        text,
        offset: i64::from(offset),
        length: text.len() as i64,
        attrs_len: log_attrs.len() as i64,
    };
    let mut tailored = false;
    tailored |= handle_words(&positions, spans.words, log_attrs);
    tailored |= handle_sentences(&positions, spans.sentences, log_attrs);
    tailored |= handle_hyphens(&positions, spans.insert_hyphens, log_attrs);
    tailored |= handle_allow_breaks(&positions, spans.allow_breaks, log_attrs);
    tailored
}

pub fn attr_break(text: &str, spans: &BreakAttrSpans<'_>, offset: i32, attrs: &mut [LogAttr]) {
    let before = attrs[0];
    if break_attrs(text, spans, offset, attrs) {
        enforce_boundary(attrs, before);
    }
}
