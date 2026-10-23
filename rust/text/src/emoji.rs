//! Southstar text — emoji properties and the emoji presentation run iterator, a port of Chromium's Ragel scanner.
//! Copyright 2015 The Chromium Authors; 2019 Google LLC; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[path = "emoji_table.rs"]
mod table;

fn bsearch_interval(ch: u32, table: &[(u32, u32)]) -> bool {
    let mut lower: usize = 0;
    let mut upper: isize = table.len() as isize - 1;
    while lower as isize <= upper {
        let mid = (lower + upper.unsigned_abs()) / 2;
        let (start, end) = table[mid];
        if ch < start {
            upper = mid as isize - 1;
        } else if ch > end {
            lower = mid + 1;
        } else {
            return true;
        }
    }
    false
}

fn in_table(ch: u32, table: &[(u32, u32)]) -> bool {
    ch >= table[0].0 && bsearch_interval(ch, table)
}

fn is_emoji(ch: u32) -> bool {
    in_table(ch, table::EMOJI)
}

fn is_emoji_presentation(ch: u32) -> bool {
    in_table(ch, table::EMOJI_PRESENTATION)
}

fn is_emoji_modifier(ch: u32) -> bool {
    in_table(ch, table::EMOJI_MODIFIER)
}

fn is_emoji_modifier_base(ch: u32) -> bool {
    in_table(ch, table::EMOJI_MODIFIER_BASE)
}

pub fn is_emoji_base_character(ch: u32) -> bool {
    is_emoji(ch)
}

pub fn is_emoji_extended_pictographic(ch: u32) -> bool {
    in_table(ch, table::EXTENDED_PICTOGRAPHIC)
}

fn is_emoji_keycap_base(ch: u32) -> bool {
    (u32::from(b'0')..=u32::from(b'9')).contains(&ch)
        || ch == u32::from(b'#')
        || ch == u32::from(b'*')
}

fn is_regional_indicator(ch: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&ch)
}

const COMBINING_ENCLOSING_CIRCLE_BACKSLASH_CHARACTER: u32 = 0x20E0;
const COMBINING_ENCLOSING_KEYCAP_CHARACTER: u32 = 0x20E3;
const VARIATION_SELECTOR_15_CHARACTER: u32 = 0xFE0E;
const VARIATION_SELECTOR_16_CHARACTER: u32 = 0xFE0F;
const ZERO_WIDTH_JOINER_CHARACTER: u32 = 0x200D;

const EMOJI_TEXT_PRESENTATION: u8 = 1;
const EMOJI_EMOJI_PRESENTATION: u8 = 2;
const EMOJI_MODIFIER_BASE: u8 = 3;
const EMOJI_MODIFIER: u8 = 4;
const REGIONAL_INDICATOR: u8 = 6;
const KEYCAP_BASE: u8 = 7;
const COMBINING_ENCLOSING_KEYCAP: u8 = 8;
const COMBINING_ENCLOSING_CIRCLE_BACKSLASH: u8 = 9;
const ZWJ: u8 = 10;
const VS15: u8 = 11;
const VS16: u8 = 12;
const TAG_BASE: u8 = 13;
const TAG_SEQUENCE: u8 = 14;
const TAG_TERM: u8 = 15;
const MAX_EMOJI_SCANNER_CATEGORY: u8 = 16;

fn segmentation_category(codepoint: u32) -> u8 {
    if (u32::from(b'a')..=u32::from(b'z')).contains(&codepoint)
        || (u32::from(b'A')..=u32::from(b'Z')).contains(&codepoint)
        || codepoint == u32::from(b' ')
    {
        return MAX_EMOJI_SCANNER_CATEGORY;
    }
    if (u32::from(b'0')..=u32::from(b'9')).contains(&codepoint) {
        return KEYCAP_BASE;
    }
    if codepoint < 0x80 {
        return if codepoint == u32::from(b'#') || codepoint == u32::from(b'*') {
            KEYCAP_BASE
        } else {
            MAX_EMOJI_SCANNER_CATEGORY
        };
    }
    match codepoint {
        COMBINING_ENCLOSING_KEYCAP_CHARACTER => return COMBINING_ENCLOSING_KEYCAP,
        COMBINING_ENCLOSING_CIRCLE_BACKSLASH_CHARACTER => {
            return COMBINING_ENCLOSING_CIRCLE_BACKSLASH;
        }
        ZERO_WIDTH_JOINER_CHARACTER => return ZWJ,
        VARIATION_SELECTOR_15_CHARACTER => return VS15,
        VARIATION_SELECTOR_16_CHARACTER => return VS16,
        0x1F3F4 => return TAG_BASE,
        0xE007F => return TAG_TERM,
        _ => {}
    }
    if (0xE0030..=0xE0039).contains(&codepoint) || (0xE0061..=0xE007A).contains(&codepoint) {
        return TAG_SEQUENCE;
    }
    if is_emoji_modifier_base(codepoint) {
        return EMOJI_MODIFIER_BASE;
    }
    if is_emoji_modifier(codepoint) {
        return EMOJI_MODIFIER;
    }
    if is_regional_indicator(codepoint) {
        return REGIONAL_INDICATOR;
    }
    if is_emoji_keycap_base(codepoint) {
        return KEYCAP_BASE;
    }
    if is_emoji_presentation(codepoint) {
        return EMOJI_EMOJI_PRESENTATION;
    }
    if is_emoji(codepoint) {
        return EMOJI_TEXT_PRESENTATION;
    }
    MAX_EMOJI_SCANNER_CATEGORY
}

const ACTIONS: [u8; 35] = [
    0, 1, 0, 1, 1, 1, 6, 1, 7, 1, 8, 1, 9, 1, 10, 1, 11, 1, 12, 1, 13, 1, 14, 1, 15, 2, 2, 3, 2, 2,
    4, 2, 2, 5, 0,
];

const KEY_OFFSETS: [usize; 13] = [0, 5, 7, 14, 18, 20, 21, 24, 29, 30, 34, 35, 37];

const TRANS_KEYS: [u8; 43] = [
    3, 7, 13, 0, 2, 14, 15, 2, 3, 6, 7, 13, 0, 1, 9, 10, 11, 12, 10, 12, 10, 4, 10, 12, 4, 9, 10,
    11, 12, 6, 9, 10, 11, 12, 8, 8, 10, 9, 10, 11, 12, 14, 0,
];

const SINGLE_LENGTHS: [usize; 13] = [3, 2, 5, 4, 2, 1, 3, 5, 1, 4, 1, 2, 5];

const RANGE_LENGTHS: [usize; 13] = [1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

const INDEX_OFFSETS: [usize; 13] = [0, 5, 8, 15, 20, 23, 25, 29, 35, 37, 42, 44, 47];

const COND_TARGS: [usize; 67] = [
    6, 4, 4, 4, 2, 1, 2, 2, 3, 7, 8, 9, 12, 3, 2, 2, 0, 2, 5, 2, 0, 5, 2, 0, 2, 5, 0, 5, 2, 5, 2,
    0, 2, 5, 2, 2, 2, 2, 0, 10, 11, 2, 2, 2, 2, 0, 2, 2, 0, 2, 5, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 0,
];

const COND_ACTIONS: [usize; 67] = [
    28, 28, 28, 28, 23, 0, 9, 21, 28, 28, 0, 31, 28, 31, 11, 9, 0, 5, 25, 23, 0, 28, 17, 0, 23, 28,
    0, 28, 17, 28, 9, 0, 5, 25, 17, 9, 19, 9, 0, 0, 25, 19, 5, 13, 7, 0, 15, 9, 0, 5, 25, 0, 17,
    23, 21, 0, 23, 17, 23, 17, 17, 19, 19, 13, 15, 17, 0,
];

const EOF_TRANS: [usize; 13] = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66];

const START_STATE: usize = 2;

struct Scan {
    end: usize,
    is_emoji: bool,
    has_vs: bool,
}

fn transition(state: usize, category: u8) -> usize {
    let mut keys = KEY_OFFSETS[state];
    let mut trans = INDEX_OFFSETS[state];

    let single_length = SINGLE_LENGTHS[state];
    if single_length > 0 {
        let singles = &TRANS_KEYS[keys..keys + single_length];
        if let Some(found) = singles.iter().position(|&key| key == category) {
            return trans + found;
        }
        keys += single_length;
        trans += single_length;
    }

    let range_length = RANGE_LENGTHS[state];
    if range_length > 0 {
        let ranges = &TRANS_KEYS[keys..keys + 2 * range_length];
        let found = ranges
            .chunks_exact(2)
            .position(|pair| pair[0] <= category && category <= pair[1]);
        return trans + found.unwrap_or(range_length);
    }

    trans
}

fn scan_emoji_presentation(types: &[u8], start: usize) -> Scan {
    let eof = types.len();
    let mut p = start;
    let mut state = START_STATE;
    let mut token_end = 0;
    let mut act = 0;

    loop {
        let trans = if p == eof {
            EOF_TRANS[state] - 1
        } else {
            transition(state, types[p])
        };
        state = COND_TARGS[trans];

        let action_offset = COND_ACTIONS[trans];
        if action_offset != 0 {
            let count = usize::from(ACTIONS[action_offset]);
            for &action in &ACTIONS[action_offset + 1..action_offset + 1 + count] {
                let result = match action {
                    2 => {
                        token_end = p + 1;
                        None
                    }
                    3 => {
                        act = 2;
                        None
                    }
                    4 => {
                        act = 3;
                        None
                    }
                    5 => {
                        act = 4;
                        None
                    }
                    6 => Some((p + 1, false, true)),
                    7 => Some((p + 1, true, true)),
                    8 => Some((p + 1, true, false)),
                    9 => Some((p + 1, false, false)),
                    10 => Some((p, false, true)),
                    11 => Some((p, true, true)),
                    12 => Some((p, true, false)),
                    13 => Some((p, false, false)),
                    14 => Some((token_end, true, false)),
                    15 => match act {
                        2 => Some((token_end, true, true)),
                        3 => Some((token_end, true, false)),
                        4 => Some((token_end, false, false)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((end, is_emoji, has_vs)) = result {
                    return Scan {
                        end,
                        is_emoji,
                        has_vs,
                    };
                }
            }
        }

        if p == eof {
            return Scan {
                end: p,
                is_emoji: false,
                has_vs: false,
            };
        }
        p += 1;
    }
}

#[derive(Clone, Debug)]
pub struct EmojiIter<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    is_emoji: bool,
    has_vs: bool,
    types: Vec<u8>,
    cursor: usize,
}

impl<'a> EmojiIter<'a> {
    pub fn new(text: &'a str) -> EmojiIter<'a> {
        let mut iter = EmojiIter {
            text,
            start: 0,
            end: 0,
            is_emoji: false,
            has_vs: false,
            types: Vec::new(),
            cursor: 0,
        };

        if text.is_ascii() {
            iter.end = text.len();
            return iter;
        }

        iter.types = text
            .chars()
            .map(|ch| segmentation_category(u32::from(ch)))
            .collect();
        iter.next_run();
        iter
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn is_emoji(&self) -> bool {
        self.is_emoji
    }

    pub fn has_vs(&self) -> bool {
        self.has_vs
    }

    pub fn next_run(&mut self) -> bool {
        if self.end >= self.text.len() {
            return false;
        }

        self.start = self.end;
        let n_chars = self.types.len();
        let old_cursor = self.cursor;

        let mut scan = scan_emoji_presentation(&self.types, self.cursor);
        loop {
            self.cursor = scan.end;
            self.is_emoji = scan.is_emoji;
            self.has_vs = scan.has_vs;

            if scan.end == n_chars {
                break;
            }

            scan = scan_emoji_presentation(&self.types, scan.end);
            if self.is_emoji != scan.is_emoji || self.has_vs != scan.has_vs {
                break;
            }
        }

        let advanced = self.cursor - old_cursor;
        self.end = self.text[self.start..]
            .char_indices()
            .nth(advanced)
            .map_or(self.text.len(), |(offset, _)| self.start + offset);
        true
    }
}
