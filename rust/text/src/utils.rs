//! Southstar text — small text helpers: the library version, zero-width characters, paragraph boundaries and the cache switches.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::sync::OnceLock;

pub const VERSION_MAJOR: i32 = 1;
pub const VERSION_MINOR: i32 = 58;
pub const VERSION_MICRO: i32 = 2;

pub const fn version_encode(major: i32, minor: i32, micro: i32) -> i32 {
    major * 10000 + minor * 100 + micro
}

pub const fn version() -> i32 {
    version_encode(VERSION_MAJOR, VERSION_MINOR, VERSION_MICRO)
}

pub const fn version_string() -> &'static str {
    "1.58.2"
}

pub fn is_zero_width(ch: u32) -> bool {
    ((ch & !0x007F) == 0x2000
        && ((0x200B..=0x200F).contains(&ch)
            || (0x202A..=0x202E).contains(&ch)
            || (0x2060..=0x2063).contains(&ch)
            || (0x2066..=0x2069).contains(&ch)
            || ch == 0x2028))
        || ch == 0x00AD
        || ch == 0x034F
        || ch == 0xFEFF
}

const PARAGRAPH_SEPARATOR_UTF8: &[u8] = "\u{2029}".as_bytes();

pub fn find_paragraph_boundary(text: &str) -> (usize, usize) {
    let bytes = text.as_bytes();
    let length = bytes.len();
    let mut paragraph_delimiter_index = length;
    let mut next_paragraph_start = length;
    if length == 0 {
        return (paragraph_delimiter_index, next_paragraph_start);
    }

    let mut start = None;
    let mut delimiter = None;
    let mut prev_sep = 0u8;
    for (p, character) in text.char_indices() {
        if prev_sep == b'\n'
            || prev_sep == PARAGRAPH_SEPARATOR_UTF8[0]
            || (prev_sep == b'\r' && bytes[p] != b'\n')
        {
            start = Some(p);
            break;
        }

        if character == '\n' || character == '\r' || character == '\u{2029}' {
            if delimiter.is_none() {
                delimiter = Some(p);
            }
            prev_sep = bytes[p];
        } else {
            prev_sep = 0;
        }
    }

    if let Some(delimiter) = delimiter {
        paragraph_delimiter_index = delimiter;
    }
    if let Some(start) = start {
        next_paragraph_start = start;
    }
    (paragraph_delimiter_index, next_paragraph_start)
}

fn cache_env() -> Option<String> {
    std::env::var("NS_PANGO_SHAPE_CACHE").ok()
}

pub fn caches_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| cache_env().is_none_or(|v| !(v == "0" || v.eq_ignore_ascii_case("off"))))
}

pub fn caches_verifying() -> bool {
    static VERIFYING: OnceLock<bool> = OnceLock::new();
    *VERIFYING.get_or_init(|| cache_env().is_some_and(|v| v.eq_ignore_ascii_case("verify")))
}
