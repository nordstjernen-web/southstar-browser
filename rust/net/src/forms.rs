//! Southstar — encoding form submissions: application/x-www-form-urlencoded in the page's submission charset, and multipart/form-data boundaries and quoted field names.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::Mutex;

static SUBMISSION_CHARSET: Mutex<Option<Vec<u8>>> = Mutex::new(None);

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

pub fn set_submission_charset(charset: Option<&[u8]>) {
    let mut slot = SUBMISSION_CHARSET.lock().unwrap_or_else(|e| e.into_inner());
    *slot = None;
    let Some(charset) = charset.filter(|c| !c.is_empty()) else {
        return;
    };
    let start = charset
        .iter()
        .position(|&c| !is_space(c))
        .unwrap_or(charset.len());
    let end = charset
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |i| i + 1);
    let stripped = &charset[start..end.max(start)];
    let first = &stripped[..stripped
        .iter()
        .position(|&c| c == b' ' || c == b',' || c == b'\t')
        .unwrap_or(stripped.len())];
    let unicode = [&b"UTF-8"[..], b"UTF8", b"UTF-16LE", b"UTF-16BE"]
        .iter()
        .any(|u| first.eq_ignore_ascii_case(u));
    if !first.is_empty() && !unicode {
        *slot = Some(first.to_vec());
    }
}

pub fn submission_charset() -> Option<Vec<u8>> {
    SUBMISSION_CHARSET
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn urlencode(out: &mut Vec<u8>, text: &[u8]) {
    for &c in text {
        if c.is_ascii_alphanumeric() || matches!(c, b'*' | b'-' | b'.' | b'_') {
            out.push(c);
        } else if c == b' ' {
            out.push(b'+');
        } else {
            out.extend_from_slice(format!("%{c:02X}").as_bytes());
        }
    }
}

pub fn quote_field(out: &mut Vec<u8>, text: &[u8]) {
    for &c in text {
        match c {
            b'"' => out.extend_from_slice(b"%22"),
            b'\r' => out.extend_from_slice(b"%0D"),
            b'\n' => out.extend_from_slice(b"%0A"),
            _ => out.push(c),
        }
    }
}

pub fn boundary(words: [u32; 4]) -> Vec<u8> {
    format!(
        "----SouthstarFormBoundary{:08x}{:08x}{:08x}{:08x}",
        words[0], words[1], words[2], words[3]
    )
    .into_bytes()
}
