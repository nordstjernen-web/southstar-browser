//! Southstar — stepping over UTF-8 code points and stacking text one code point per line for upright vertical text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

pub fn utf8_skip(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

pub fn vertical_stack_text(text: &[u8]) -> CString {
    let mut out = Vec::with_capacity(text.len() * 2);
    let mut p = 0;
    while p < text.len() {
        let next = (p + utf8_skip(text[p])).min(text.len());
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(&text[p..next]);
        p = next;
    }
    CString::new(out).unwrap_or_default()
}
