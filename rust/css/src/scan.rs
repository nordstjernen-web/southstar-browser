//! Southstar — the byte-level CSS scanning helpers of css.c: whitespace and identifier classes, bracket matching, and splitting text at top-level separators outside strings, comments and brackets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

pub(crate) fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'-' || c >= 128
}

pub(crate) fn is_ident(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

pub(crate) fn is_gspace(c: u8) -> bool {
    c == b' ' || (b'\t'..=b'\r').contains(&c)
}

pub(crate) fn strip(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_gspace(c)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&c| !is_gspace(c))
        .map_or(start, |last| last + 1);
    &s[start..end.max(start)]
}

pub(crate) fn byte(s: &[u8], at: usize) -> u8 {
    s.get(at).copied().unwrap_or(0)
}

pub(crate) fn starts_with_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub(crate) fn skip_ws(s: &[u8], mut p: usize, end: usize) -> usize {
    while p < end && is_ws(s[p]) {
        p += 1;
    }
    p
}

pub(crate) fn match_close_paren(s: &[u8], mut p: usize, end: usize) -> Option<usize> {
    let mut depth = 1;
    while p < end && depth > 0 {
        match s[p] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(p);
                }
            }
            _ => {}
        }
        p += 1;
    }
    None
}

fn skip_comment(s: &[u8], p: usize, end: usize) -> usize {
    if p + 1 >= end || s[p] != b'/' || s[p + 1] != b'*' {
        return p;
    }
    let mut p = p + 2;
    while p + 1 < end && !(s[p] == b'*' && s[p + 1] == b'/') {
        p += 1;
    }
    if p + 1 < end { p + 2 } else { end }
}

pub(crate) fn scan_until(s: &[u8], mut p: usize, end: usize, terminators: &[u8]) -> (usize, u8) {
    let mut quote = 0u8;
    let (mut paren, mut bracket, mut brace) = (0u32, 0u32, 0u32);
    while p < end {
        let c = s[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
                continue;
            }
            if c == quote || matches!(c, b'\n' | b'\r' | 0x0c) {
                quote = 0;
            }
            p += 1;
            continue;
        }
        if c == b'/' && p + 1 < end && s[p + 1] == b'*' {
            p = skip_comment(s, p, end);
            continue;
        }
        if c == b'\\' && p + 1 < end {
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            p += 1;
            continue;
        }
        if paren == 0 && bracket == 0 && brace == 0 && (c == 0 || terminators.contains(&c)) {
            return (p, c);
        }
        match c {
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            b'[' => bracket += 1,
            b']' if bracket > 0 => bracket -= 1,
            b'{' => brace += 1,
            b'}' if brace > 0 => brace -= 1,
            _ => {}
        }
        p += 1;
    }
    (p, 0)
}

pub(crate) fn trim_range(s: &[u8], mut start: usize, mut end: usize) -> &[u8] {
    while start < end && is_ws(s[start]) {
        start += 1;
    }
    while end > start && is_ws(s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

pub(crate) fn split_ws_limit(s: &[u8], max: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let end = s.len();
    let mut p = 0;
    while p < end && out.len() < max {
        p = skip_ws(s, p, end);
        if p >= end {
            break;
        }
        let start = p;
        p = scan_until(s, p, end, b" \t\n\r\x0c").0;
        out.push(&s[start..p]);
    }
    out
}

pub(crate) fn split_args(s: &[u8], start: usize, body_end: usize, max: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut seg = start;
    while seg < body_end && out.len() < max {
        let (next, term) = scan_until(s, seg, body_end, b",");
        out.push(trim_range(s, seg, next));
        if term != b',' {
            break;
        }
        seg = next + 1;
    }
    out
}
