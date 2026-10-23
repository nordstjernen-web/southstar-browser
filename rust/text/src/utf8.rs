//! Southstar text — UTF-8 stepping and counting over byte offsets, with GLib's answers for offsets that fall inside a character.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

const SKIP: [u8; 16] = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 3, 4];

pub(crate) fn skip(lead: u8) -> usize {
    match lead {
        0xF8..=0xFB => 5,
        0xFC..=0xFD => 6,
        0xFE..=0xFF => 1,
        _ => usize::from(SKIP[usize::from(lead >> 4)]),
    }
}

pub(crate) fn next_char(text: &[u8], index: usize) -> usize {
    index + skip(text.get(index).copied().unwrap_or(0))
}

pub(crate) fn prev_char(text: &[u8], index: usize) -> usize {
    let mut prev = index.saturating_sub(1);
    while prev > 0 && (text.get(prev).copied().unwrap_or(0) & 0xC0) == 0x80 {
        prev -= 1;
    }
    prev
}

pub(crate) fn char_at(text: &[u8], index: usize) -> u32 {
    let Some(&lead) = text.get(index) else {
        return 0;
    };
    let (len, mut value) = match lead {
        0x00..=0x7F => return u32::from(lead),
        0x80..=0xBF => return u32::MAX,
        0xC0..=0xDF => (2, u32::from(lead & 0x1F)),
        0xE0..=0xEF => (3, u32::from(lead & 0x0F)),
        0xF0..=0xF7 => (4, u32::from(lead & 0x07)),
        0xF8..=0xFB => (5, u32::from(lead & 0x03)),
        0xFC..=0xFD => (6, u32::from(lead & 0x01)),
        _ => return u32::MAX,
    };
    for offset in 1..len {
        let byte = text.get(index + offset).copied().unwrap_or(0);
        if (byte & 0xC0) != 0x80 {
            return u32::MAX;
        }
        value = (value << 6) | u32::from(byte & 0x3F);
    }
    value
}

pub(crate) fn g_utf8_strlen(text: &[u8], max: usize) -> i32 {
    let at = |i: usize| text.get(i).copied().unwrap_or(0);
    if max == 0 || at(0) == 0 {
        return 0;
    }
    let mut len = 0;
    let mut p = next_char(text, 0);
    while p < max && at(p) != 0 {
        len += 1;
        p = next_char(text, p);
    }
    if p <= max {
        len += 1;
    }
    len
}

pub(crate) fn pango_utf8_strlen(text: &[u8], max: usize) -> i32 {
    if max == 0 {
        return 0;
    }
    let mut len = 0;
    let mut p = next_char(text, 0);
    while p < max {
        len += 1;
        p = next_char(text, p);
    }
    if p <= max {
        len += 1;
    }
    len
}

pub(crate) fn pointer_to_offset(text: &[u8], start: usize, pos: usize) -> i32 {
    let mut offset = 0;
    let mut s = start;
    while s < pos {
        s = next_char(text, s);
        offset += 1;
    }
    offset
}
