//! Southstar — WebVTT cue parsing for a <video>'s default subtitle track.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use std::ffi::CString;

pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: CString,
}

fn read_uint(s: &[u8], p: &mut usize) -> Option<i64> {
    if !s.get(*p).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut v: i64 = 0;
    while let Some(&d) = s.get(*p).filter(|c| c.is_ascii_digit()) {
        if v < 100_000_000 {
            v = v * 10 + i64::from(d - b'0');
        }
        *p += 1;
    }
    Some(v)
}

fn parse_ts(s: &[u8]) -> f64 {
    let mut p = 0;
    while matches!(s.get(p), Some(b' ' | b'\t')) {
        p += 1;
    }
    let Some(a) = read_uint(s, &mut p) else {
        return -1.0;
    };
    if s.get(p) != Some(&b':') {
        return -1.0;
    }
    p += 1;
    let Some(b) = read_uint(s, &mut p) else {
        return -1.0;
    };
    let (hours, mins, secs) = if s.get(p) == Some(&b':') {
        p += 1;
        let Some(c) = read_uint(s, &mut p) else {
            return -1.0;
        };
        (a, b, c)
    } else {
        (0, a, b)
    };
    let mut ms: i64 = 0;
    if matches!(s.get(p), Some(b'.' | b',')) {
        p += 1;
        let mut f: i64 = 0;
        let mut nd = 0;
        while let Some(&d) = s.get(p).filter(|c| c.is_ascii_digit()) {
            f = f.wrapping_mul(10).wrapping_add(i64::from(d - b'0'));
            p += 1;
            nd += 1;
        }
        while nd < 3 {
            f = f.wrapping_mul(10);
            nd += 1;
        }
        while nd > 3 {
            f /= 10;
            nd -= 1;
        }
        ms = f;
    }
    hours as f64 * 3600.0 + mins as f64 * 60.0 + secs as f64 + ms as f64 / 1000.0
}

const ENTITIES: [(&[u8], Option<u8>); 6] = [
    (b"&amp;", Some(b'&')),
    (b"&lt;", Some(b'<')),
    (b"&gt;", Some(b'>')),
    (b"&nbsp;", Some(b' ')),
    (b"&lrm;", None),
    (b"&rlm;", None),
];

fn clean_text(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    'scan: while i < raw.len() {
        let c = raw[i];
        if c == b'<' {
            match raw[i..].iter().position(|&b| b == b'>') {
                Some(e) => {
                    i += e + 1;
                    continue;
                }
                None => break,
            }
        }
        if c == b'&' {
            for (pattern, replacement) in ENTITIES {
                if raw[i..].starts_with(pattern) {
                    if let Some(r) = replacement {
                        out.push(r);
                    }
                    i += pattern.len();
                    continue 'scan;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn strip_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn parse(body: &[u8]) -> Vec<Cue> {
    let text = body.split(|&b| b == 0).next().unwrap_or_default();
    let lines: Vec<&[u8]> = text.split(|&b| b == b'\n').map(strip_cr).collect();
    let mut cues = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let Some(arrow) = find(line, b"-->") else {
            i += 1;
            continue;
        };
        let start = parse_ts(line);
        let end = parse_ts(&line[arrow + 3..]);
        i += 1;
        let mut txt = Vec::new();
        while i < lines.len() {
            let tl = lines[i];
            if tl.is_empty() {
                i += 1;
                break;
            }
            if !txt.is_empty() {
                txt.push(b'\n');
            }
            txt.extend_from_slice(&clean_text(tl));
            i += 1;
        }
        if start >= 0.0
            && end > start
            && !txt.is_empty()
            && let Ok(text) = CString::new(txt)
        {
            cues.push(Cue { start, end, text });
        }
    }
    cues
}

pub fn active(cues: &[Cue], t: f64) -> Option<&Cue> {
    cues.iter().find(|c| t >= c.start && t < c.end)
}
