//! Southstar — reading CSS identifiers and strings with their escapes, and writing an identifier back out with the escapes it needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::scan::{is_ident, is_ws};

fn push_char(out: &mut Vec<u8>, cp: u32) {
    let c = if cp == 0 || cp > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
        '\u{FFFD}'
    } else {
        char::from_u32(cp).unwrap_or('\u{FFFD}')
    };
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

fn hex_escape(out: &mut Vec<u8>, s: &[u8], p: &mut usize, end: usize) {
    let mut cp = 0u32;
    let mut n = 0;
    while *p < end && n < 6 && s[*p].is_ascii_hexdigit() {
        cp = cp * 16 + char::from(s[*p]).to_digit(16).unwrap_or(0);
        *p += 1;
        n += 1;
    }
    if *p < end && is_ws(s[*p]) {
        let cr = s[*p] == b'\r';
        *p += 1;
        if cr && *p < end && s[*p] == b'\n' {
            *p += 1;
        }
    }
    push_char(out, cp);
}

pub(crate) fn read_ident(s: &[u8], pos: &mut usize, end: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut p = *pos;
    while p < end {
        let c = s[p];
        if c == b'\\' {
            let escaped = if p + 1 < end { s[p + 1] } else { 0 };
            if p + 1 >= end || matches!(escaped, b'\n' | b'\r' | 0x0c) {
                push_char(&mut out, 0xFFFD);
                p += 1;
                break;
            }
            if escaped.is_ascii_hexdigit() {
                p += 1;
                hex_escape(&mut out, s, &mut p, end);
                continue;
            }
            out.push(escaped);
            p += 2;
            continue;
        }
        if is_ident(c) {
            out.push(c);
            p += 1;
            continue;
        }
        break;
    }
    *pos = p;
    out
}

pub(crate) fn read_string(s: &[u8], pos: &mut usize, end: usize) -> Vec<u8> {
    let mut p = *pos;
    if p >= end || (s[p] != b'"' && s[p] != b'\'') {
        return Vec::new();
    }
    let quote = s[p];
    p += 1;
    let mut out = Vec::new();
    while p < end {
        let c = s[p];
        if c == quote {
            p += 1;
            break;
        }
        if matches!(c, b'\n' | b'\r' | 0x0c) {
            break;
        }
        if c == b'\\' && p + 1 < end {
            let escaped = s[p + 1];
            if matches!(escaped, b'\n' | b'\r' | 0x0c) {
                p += 2;
                continue;
            }
            if escaped.is_ascii_hexdigit() {
                p += 1;
                hex_escape(&mut out, s, &mut p, end);
                continue;
            }
            out.push(escaped);
            p += 2;
            continue;
        }
        out.push(c);
        p += 1;
    }
    *pos = p;
    out
}

fn push_hex_escape(out: &mut Vec<u8>, c: u8) {
    out.extend_from_slice(format!("\\{c:x} ").as_bytes());
}

pub(crate) fn ident_serialize(name: &[u8]) -> Vec<u8> {
    let name = &name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())];
    let mut out = Vec::with_capacity(name.len() + 4);
    let mut p = 0;
    while p < name.len() {
        let c = name[p];
        let first = p == 0;
        let next = name.get(p + 1).copied().unwrap_or(0);
        if c <= 0x1f || c == 0x7f || (first && c.is_ascii_digit()) {
            push_hex_escape(&mut out, c);
        } else if first && c == b'-' && next.is_ascii_digit() {
            out.push(b'-');
            p += 1;
            push_hex_escape(&mut out, name[p]);
        } else if first && c == b'-' && next == 0 {
            out.extend_from_slice(b"\\-");
        } else if c >= 0x80 || c == b'-' || c == b'_' || c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push(b'\\');
            out.push(c);
        }
        p += 1;
    }
    out
}
