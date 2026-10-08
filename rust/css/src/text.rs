//! Southstar — text clean-ups css.c applies to specified values: leading zeros before bare decimals, negative zero written as zero, and splitting at top-level commas.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::ffi;
use crate::scan::{is_ident, scan_until, trim_range};

fn zero_needed_after(prev: Option<u8>) -> bool {
    matches!(
        prev,
        None | Some(b' ' | b'\t' | b'(' | b',' | b'+' | b'-' | b'/' | b'*')
    )
}

pub(crate) fn add_leading_zeros(v: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() + 2);
    for (i, &c) in v.iter().enumerate() {
        if c == b'.'
            && v.get(i + 1).is_some_and(u8::is_ascii_digit)
            && zero_needed_after(i.checked_sub(1).map(|j| v[j]))
        {
            out.push(b'0');
        }
        out.push(c);
    }
    out
}

pub(crate) fn normalize_negative_zero(value: &[u8]) -> Vec<u8> {
    let text = CString::new(value).unwrap_or_default();
    let s = text.to_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut p = 0;
    while p < s.len() {
        let boundary = p == 0 || !(is_ident(s[p - 1]) || s[p - 1] == b'.' || s[p - 1] == b'\\');
        let next = s.get(p + 1).copied().unwrap_or(0);
        if s[p] == b'-' && boundary && (next.is_ascii_digit() || next == b'.') {
            let (number, end) = ffi::strtod(&text, p);
            if end > p + 1 && number == 0.0 {
                out.push(b'0');
                p = end;
                continue;
            }
        }
        out.push(s[p]);
        p += 1;
    }
    out
}

pub(crate) fn split_top_level_commas(text: &[u8]) -> Vec<Vec<u8>> {
    let end = text.len();
    let mut parts = Vec::new();
    let mut p = 0;
    while p <= end {
        let (seg_end, term) = scan_until(text, p, end, b",");
        parts.push(trim_range(text, p, seg_end).to_vec());
        if term != b',' {
            break;
        }
        p = seg_end + 1;
    }
    parts
}
