//! Southstar — reading one character from UTF-8 bytes with GLib's lenient rules, as layout and editing count text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const INVALID_CHAR: u32 = u32::MAX;

pub fn utf8_get_char(bytes: &[u8], at: usize) -> u32 {
    let byte = |i: usize| bytes.get(at + i).copied().unwrap_or(0);
    let lead = byte(0);
    let (len, mask) = match lead {
        0x00..=0x7f => (1, 0x7f),
        _ if lead & 0xe0 == 0xc0 => (2, 0x1f),
        _ if lead & 0xf0 == 0xe0 => (3, 0x0f),
        _ if lead & 0xf8 == 0xf0 => (4, 0x07),
        _ if lead & 0xfc == 0xf8 => (5, 0x03),
        _ if lead & 0xfe == 0xfc => (6, 0x01),
        _ => return INVALID_CHAR,
    };
    let mut c = u32::from(lead & mask);
    for i in 1..len {
        let b = byte(i);
        if b & 0xc0 != 0x80 {
            return INVALID_CHAR;
        }
        c = (c << 6) | u32::from(b & 0x3f);
    }
    c
}
