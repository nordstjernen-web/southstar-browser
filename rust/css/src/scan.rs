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
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
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

pub(crate) fn skip_comment(s: &[u8], p: usize, end: usize) -> usize {
    if p + 1 >= end || s[p] != b'/' || s[p + 1] != b'*' {
        return p;
    }
    let mut p = p + 2;
    while p + 1 < end && !(s[p] == b'*' && s[p + 1] == b'/') {
        p += 1;
    }
    if p + 1 < end { p + 2 } else { end }
}

pub(crate) fn skip_ws_comments(s: &[u8], mut p: usize, end: usize) -> usize {
    loop {
        p = skip_ws(s, p, end);
        if p + 1 < end && s[p] == b'/' && s[p + 1] == b'*' {
            p = skip_comment(s, p, end);
            continue;
        }
        return p;
    }
}

pub(crate) fn skip_to_block_end(s: &[u8], mut p: usize, end: usize) -> usize {
    let mut depth = 0i32;
    let mut quote = 0u8;
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
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth <= 0 {
                return p + 1;
            }
        }
        p += 1;
    }
    end
}

pub(crate) fn strip_important(text: &[u8]) -> (&[u8], bool) {
    let end = text.len();
    let mut p = 0;
    let mut bang = None;
    while p < end {
        let (q, term) = scan_until(text, p, end, b"!");
        if term != b'!' {
            break;
        }
        bang = Some(q);
        p = q + 1;
    }
    let Some(bang) = bang else {
        return (text, false);
    };
    let tail = skip_ws_comments(text, bang + 1, end);
    if end - tail < 9 || !text[tail..tail + 9].eq_ignore_ascii_case(b"important") {
        return (text, false);
    }
    let after = tail + 9;
    if after < end && is_ident(text[after]) {
        return (text, false);
    }
    if skip_ws_comments(text, after, end) != end {
        return (text, false);
    }
    let mut stop = bang;
    while stop > 0 && is_gspace(text[stop - 1]) {
        stop -= 1;
    }
    (&text[..stop], true)
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

pub(crate) fn split_ws_paren(text: &[u8], max: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let end = text.len();
    let mut p = 0;
    while p < end && out.len() < max {
        p = skip_ws(text, p, end);
        if p >= end {
            break;
        }
        let start = p;
        let mut depth = 0u32;
        let mut quote = 0u8;
        while p < end {
            let c = text[p];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
                p += 1;
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
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                depth = depth.saturating_sub(1);
            } else if is_ws(c) && depth == 0 {
                break;
            }
            p += 1;
        }
        out.push(&text[start..p]);
    }
    out
}

pub(crate) fn match_paren_quoted(s: &[u8], p: usize, end: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut q = p;
    while q < end {
        let c = s[q];
        if c == b'"' || c == b'\'' {
            q += 1;
            while q < end && s[q] != c {
                if s[q] == b'\\' && q + 1 < end {
                    q += 1;
                }
                q += 1;
            }
            q += 1;
            continue;
        }
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(q);
            }
        }
        q += 1;
    }
    None
}

pub(crate) fn strtol10(text: &[u8]) -> (i64, usize) {
    let mut p = 0;
    while p < text.len() && (text[p] == b' ' || (b'\t'..=b'\r').contains(&text[p])) {
        p += 1;
    }
    let negative = text.get(p) == Some(&b'-');
    if matches!(text.get(p), Some(b'+' | b'-')) {
        p += 1;
    }
    let digits = p;
    let mut value: i64 = 0;
    let mut overflow = false;
    while p < text.len() && text[p].is_ascii_digit() {
        let digit = i64::from(text[p] - b'0');
        match value.checked_mul(10).and_then(|v| {
            if negative {
                v.checked_sub(digit)
            } else {
                v.checked_add(digit)
            }
        }) {
            Some(v) => value = v,
            None => overflow = true,
        }
        p += 1;
    }
    if p == digits {
        return (0, 0);
    }
    if overflow {
        value = if negative { i64::MIN } else { i64::MAX };
    }
    (value, p)
}

pub(crate) fn utf8_char(s: &[u8], p: usize) -> (u32, usize) {
    let c = s[p];
    let (mask, len): (u8, usize) = match c {
        0x00..=0x7f => (0x7f, 1),
        _ if c & 0xe0 == 0xc0 => (0x1f, 2),
        _ if c & 0xf0 == 0xe0 => (0x0f, 3),
        _ if c & 0xf8 == 0xf0 => (0x07, 4),
        _ if c & 0xfc == 0xf8 => (0x03, 5),
        _ if c & 0xfe == 0xfc => (0x01, 6),
        _ => return (u32::MAX, 1),
    };
    let next = (p + len).min(s.len());
    let mut value = u32::from(c & mask);
    for i in 1..len {
        match s.get(p + i) {
            Some(&b) if b & 0xc0 == 0x80 => value = (value << 6) | u32::from(b & 0x3f),
            _ => return (u32::MAX, next),
        }
    }
    (value, next)
}
