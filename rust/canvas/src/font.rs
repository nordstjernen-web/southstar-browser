//! Southstar — the serialized form of a canvas 2D context's font: keywords, the size in px and the family list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi;

const SIZE_KEYWORDS: [(&str, f64); 10] = [
    ("xx-small", 9.0),
    ("x-small", 10.0),
    ("small", 13.0),
    ("medium", 16.0),
    ("large", 18.0),
    ("x-large", 24.0),
    ("xx-large", 32.0),
    ("xxx-large", 48.0),
    ("larger", 12.0),
    ("smaller", 25.0 / 3.0),
];

const SIZE_UNITS: [(&str, f64); 12] = [
    ("px", 1.0),
    ("pt", 4.0 / 3.0),
    ("pc", 16.0),
    ("in", 96.0),
    ("cm", 96.0 / 2.54),
    ("mm", 96.0 / 25.4),
    ("q", 96.0 / 101.6),
    ("em", 10.0),
    ("ex", 5.0),
    ("ch", 5.0),
    ("rem", 16.0),
    ("%", 0.1),
];

#[derive(PartialEq)]
enum Slot {
    Style,
    Weight,
    Variant,
    Stretch,
    Normal,
    Size,
}

fn size_px(size: &[u8]) -> Option<f64> {
    if let Some(&(_, px)) = SIZE_KEYWORDS
        .iter()
        .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(size))
    {
        return Some(px);
    }
    let (v, used) = ffi::strtod_prefix(size);
    if used == 0 {
        return None;
    }
    let unit = &size[used..];
    SIZE_UNITS
        .iter()
        .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(unit))
        .map(|&(_, factor)| v * factor)
}

fn is_weight(t: &[u8]) -> bool {
    t.eq_ignore_ascii_case(b"bold")
        || t.eq_ignore_ascii_case(b"bolder")
        || t.eq_ignore_ascii_case(b"lighter")
        || (t.first().is_some_and(u8::is_ascii_digit) && t.iter().all(u8::is_ascii_digit))
}

fn slot(t: &[u8]) -> Slot {
    if t.eq_ignore_ascii_case(b"italic") || t.eq_ignore_ascii_case(b"oblique") {
        Slot::Style
    } else if t.eq_ignore_ascii_case(b"small-caps") {
        Slot::Variant
    } else if is_weight(t) {
        Slot::Weight
    } else if t.ends_with(b"condensed") || t.ends_with(b"expanded") {
        Slot::Stretch
    } else if t.eq_ignore_ascii_case(b"normal") {
        Slot::Normal
    } else {
        Slot::Size
    }
}

fn emit_family(out: &mut Vec<u8>, name: &[u8]) {
    if out.last().is_some_and(|&c| c != b' ') {
        out.extend_from_slice(b", ");
    }
    if name.contains(&b' ') {
        out.push(b'"');
        out.extend_from_slice(name);
        out.push(b'"');
    } else {
        out.extend_from_slice(name);
    }
}

fn append_families(out: &mut Vec<u8>, family: &[u8]) {
    let mut p = 0;
    while p < family.len() {
        while p < family.len() && matches!(family[p], b' ' | b',') {
            p += 1;
        }
        if p >= family.len() {
            break;
        }
        let quote = family[p];
        if quote == b'"' || quote == b'\'' {
            let start = p + 1;
            let mut q = start;
            while q < family.len() && family[q] != quote {
                q += if family[q] == b'\\' && q + 1 < family.len() {
                    2
                } else {
                    1
                };
            }
            let end = q.min(family.len());
            emit_family(out, &family[start..end]);
            p = if q < family.len() { q + 1 } else { q };
        } else {
            let start = p;
            while p < family.len() && family[p] != b',' {
                p += 1;
            }
            let mut end = p;
            while end > start && family[end - 1] == b' ' {
                end -= 1;
            }
            emit_family(out, &family[start..end]);
        }
    }
}

pub(crate) fn canonical(canon: &[u8]) -> Vec<u8> {
    let tokens: Vec<&[u8]> = canon.split(|&c| c == b' ').collect();
    let mut parts: [Option<&[u8]>; 4] = [None; 4];
    let mut i = 0;
    while i < tokens.len() {
        let slot = slot(tokens[i]);
        match slot {
            Slot::Size => break,
            Slot::Normal => {}
            Slot::Style => parts[0] = Some(tokens[i]),
            Slot::Weight => parts[1] = Some(tokens[i]),
            Slot::Variant => parts[2] = Some(tokens[i]),
            Slot::Stretch => parts[3] = Some(tokens[i]),
        }
        i += 1;
    }
    let mut out = Vec::new();
    for part in parts.into_iter().flatten() {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(if part == b"700" { b"bold" } else { part });
    }
    if let Some(size) = tokens.get(i) {
        if !out.is_empty() {
            out.push(b' ');
        }
        match size_px(size) {
            Some(px) => {
                out.extend_from_slice(&ffi::format_g(px));
                out.extend_from_slice(b"px");
            }
            None => out.extend_from_slice(size),
        }
        i += 1;
    }
    if tokens.get(i) == Some(&&b"/"[..]) && i + 1 < tokens.len() {
        i += 2;
    }
    if i < tokens.len() {
        out.push(b' ');
        append_families(&mut out, &tokens[i..].join(&b' '));
    }
    out
}
