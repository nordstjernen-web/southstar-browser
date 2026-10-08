//! Southstar — which @font-face rules a page needs: the families a font-family list names, the code points styled with each family, and unicode-range matching.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

const MAX_CODE_POINT: u32 = 0x10_FFFF;

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn trim(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s {
        if !is_space(*first) {
            break;
        }
        s = rest;
    }
    while let [rest @ .., last] = s {
        if !is_space(*last) {
            break;
        }
        s = rest;
    }
    s
}

pub fn list_names_family(list: &[u8], family: &[u8]) -> bool {
    if family.is_empty() {
        return false;
    }
    let mut p = 0;
    while p < list.len() {
        while p < list.len() && (is_space(list[p]) || list[p] == b',') {
            p += 1;
        }
        let mut token = Vec::new();
        let mut quote = 0u8;
        while p < list.len() {
            let mut ch = list[p];
            p += 1;
            if quote != 0 {
                if ch == b'\\' && p < list.len() {
                    ch = list[p];
                    p += 1;
                } else if ch == quote {
                    quote = 0;
                    continue;
                }
            } else {
                if ch == b'\'' || ch == b'"' {
                    quote = ch;
                    continue;
                }
                if ch == b',' {
                    break;
                }
                if ch == b'\\' && p < list.len() {
                    ch = list[p];
                    p += 1;
                }
            }
            token.push(ch);
        }
        if trim(&token).eq_ignore_ascii_case(family) {
            return true;
        }
    }
    false
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

#[derive(Default)]
pub struct Usage {
    code_points: Vec<u32>,
    seen: HashSet<u32>,
}

impl Usage {
    pub fn add_text(&mut self, text: &[u8]) {
        let mut p = 0;
        while p < text.len() {
            let width = utf8_width(text[p]);
            let decoded = text
                .get(p..p + width)
                .filter(|_| width > 0)
                .and_then(|bytes| core::str::from_utf8(bytes).ok())
                .and_then(|s| s.chars().next());
            match decoded {
                Some(c) => {
                    if self.seen.insert(u32::from(c)) {
                        self.code_points.push(u32::from(c));
                    }
                    p += width;
                }
                None => p += 1,
            }
        }
    }

    fn any_in(&self, lo: u32, hi: u32) -> bool {
        self.code_points.iter().any(|&cp| cp >= lo && cp <= hi)
    }
}

fn hex_value(c: u8) -> Option<u32> {
    char::from(c).to_digit(16)
}

fn parse_range(part: &[u8]) -> Option<(u32, u32)> {
    let s = trim(part);
    if s.len() < 3 || !s[0].eq_ignore_ascii_case(&b'u') || s[1] != b'+' {
        return None;
    }
    let mut i = 2;
    let (mut lo, mut hi) = (0u32, 0u32);
    let (mut digits, mut wildcards) = (0, 0);
    while i < s.len() && digits < 6 {
        if let Some(hex) = hex_value(s[i]) {
            if wildcards > 0 {
                return None;
            }
            lo = (lo << 4) | hex;
            hi = (hi << 4) | hex;
        } else if s[i] == b'?' {
            wildcards += 1;
            lo <<= 4;
            hi = (hi << 4) | 0xf;
        } else {
            break;
        }
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return None;
    }
    if wildcards > 0 {
        if i != s.len() {
            return None;
        }
    } else if s.get(i) == Some(&b'-') {
        i += 1;
        let (mut end, mut end_digits) = (0u32, 0);
        while i < s.len() && end_digits < 6 {
            let Some(hex) = hex_value(s[i]) else {
                break;
            };
            end = (end << 4) | hex;
            end_digits += 1;
            i += 1;
        }
        if end_digits == 0 || i != s.len() {
            return None;
        }
        hi = end;
    } else if i != s.len() {
        return None;
    }
    if lo > hi || lo > MAX_CODE_POINT {
        return None;
    }
    Some((lo, hi.min(MAX_CODE_POINT)))
}

pub fn range_matches(range: Option<&[u8]>, usage: Option<&Usage>) -> bool {
    let Some(usage) = usage.filter(|u| !u.code_points.is_empty()) else {
        return false;
    };
    let Some(range) = range.filter(|r| !r.is_empty()) else {
        return true;
    };
    let mut valid = false;
    for part in range.split(|&c| c == b',') {
        if let Some((lo, hi)) = parse_range(part) {
            valid = true;
            if usage.any_in(lo, hi) {
                return true;
            }
        }
    }
    !valid
}
