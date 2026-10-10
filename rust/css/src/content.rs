//! Southstar — the CSS content property and its pieces: strings with their escapes, counter() and counters() with counter styles and symbols(), attr(), quotes and images, in canonical form, and the identifiers and CSS-wide keywords they are checked against.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::image;
use crate::scan::{is_ws, strip};

pub(crate) fn wide_keyword_or_default(item: &[u8]) -> bool {
    [
        &b"initial"[..],
        b"inherit",
        b"unset",
        b"revert",
        b"revert-layer",
        b"default",
    ]
    .iter()
    .any(|word| item.eq_ignore_ascii_case(word))
}

fn unescape_cp(cp: u32) -> char {
    if cp == 0 || cp > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
        '\u{FFFD}'
    } else {
        char::from_u32(cp).unwrap_or('\u{FFFD}')
    }
}

pub(crate) fn append_unescaped(out: &mut Vec<u8>, s: &[u8], pos: &mut usize) {
    let mut p = *pos;
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    if at(p) == b'\\' && at(p + 1) != 0 {
        p += 1;
        if at(p).is_ascii_hexdigit() {
            let mut cp = 0u32;
            let mut n = 0;
            while n < 6 && at(p).is_ascii_hexdigit() {
                cp = cp * 16 + char::from(at(p)).to_digit(16).unwrap_or(0);
                p += 1;
                n += 1;
            }
            if is_ws(at(p)) {
                let cr = at(p) == b'\r';
                p += 1;
                if cr && at(p) == b'\n' {
                    p += 1;
                }
            }
            let mut buf = [0u8; 4];
            out.extend_from_slice(unescape_cp(cp).encode_utf8(&mut buf).as_bytes());
        } else {
            out.push(at(p));
            p += 1;
        }
    } else {
        out.push(at(p));
        p += 1;
    }
    *pos = p;
}

pub(crate) fn string_canonical(raw: &[u8]) -> Vec<u8> {
    let raw = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
    let mut decoded = Vec::with_capacity(raw.len());
    let mut p = 0;
    while p < raw.len() {
        append_unescaped(&mut decoded, raw, &mut p);
    }
    let decoded = &decoded[..decoded
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(decoded.len())];
    let mut out = Vec::with_capacity(decoded.len() + 2);
    out.push(b'"');
    for &c in decoded {
        if c == b'"' || c == b'\\' {
            out.push(b'\\');
        }
        out.push(c);
    }
    out.push(b'"');
    out
}

pub(crate) fn scan_string_end(s: &[u8], start: usize) -> Option<usize> {
    let quote = s[start];
    let mut p = start + 1;
    while p < s.len() {
        if s[p] == b'\\' && p + 1 < s.len() {
            p += 2;
            continue;
        }
        if s[p] == quote {
            return Some(p);
        }
        p += 1;
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    String,
    Ident,
    Func,
    Slash,
}

struct Token {
    kind: Kind,
    text: Vec<u8>,
    args: Vec<u8>,
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80 || c == b'\\'
}

fn tokenize(text: &[u8]) -> Option<Vec<Token>> {
    let mut toks = Vec::new();
    let mut p = 0;
    while p < text.len() {
        let c = text[p];
        if is_ws(c) {
            p += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            let e = scan_string_end(text, p)?;
            toks.push(Token {
                kind: Kind::String,
                text: string_canonical(&text[p + 1..e]),
                args: Vec::new(),
            });
            p = e + 1;
        } else if c == b'/' {
            toks.push(Token {
                kind: Kind::Slash,
                text: b"/".to_vec(),
                args: Vec::new(),
            });
            p += 1;
        } else if c.is_ascii_alphabetic() || c == b'-' || c == b'_' || c == b'\\' || c >= 0x80 {
            let start = p;
            while p < text.len() && is_ident_char(text[p]) {
                if text[p] == b'\\' && p + 1 < text.len() {
                    p += 2;
                } else {
                    p += 1;
                }
            }
            let name = &text[start..p];
            if p < text.len() && text[p] == b'(' {
                let mut depth = 0i32;
                let args_start = p + 1;
                let mut q = p;
                let mut close = None;
                while q < text.len() {
                    let d = text[q];
                    if d == b'"' || d == b'\'' {
                        match scan_string_end(text, q) {
                            Some(e) => {
                                q = e + 1;
                                continue;
                            }
                            None => return None,
                        }
                    }
                    if d == b'(' {
                        depth += 1;
                    } else if d == b')' {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(q);
                            break;
                        }
                    }
                    q += 1;
                }
                let close = close?;
                toks.push(Token {
                    kind: Kind::Func,
                    text: name.to_ascii_lowercase(),
                    args: strip(&text[args_start..close]).to_vec(),
                });
                p = close + 1;
            } else {
                toks.push(Token {
                    kind: Kind::Ident,
                    text: name.to_vec(),
                    args: Vec::new(),
                });
            }
        } else {
            return None;
        }
    }
    Some(toks)
}

pub(crate) fn split_args(args: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut seg = 0;
    let mut depth = 0i32;
    let mut q = 0;
    loop {
        let c = args.get(q).copied().unwrap_or(0);
        if c == b'"' || c == b'\'' {
            q = scan_string_end(args, q)? + 1;
            continue;
        }
        if c == b'(' {
            depth += 1;
        } else if c == b')' && depth > 0 {
            depth -= 1;
        }
        if (c == b',' && depth == 0) || c == 0 {
            out.push(strip(&args[seg..q.min(args.len())]).to_vec());
            if c == 0 {
                break;
            }
            seg = q + 1;
        }
        q += 1;
    }
    Some(out)
}

pub(crate) fn ident_valid(s: &[u8]) -> bool {
    if s.is_empty() || s[0].is_ascii_digit() {
        return false;
    }
    let mut p = 0;
    while p < s.len() {
        let c = s[p];
        if c == b'\\' {
            if p + 1 >= s.len() {
                return false;
            }
            p += 2;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80) {
            return false;
        }
        p += 1;
    }
    true
}

pub(crate) fn is_string_token(s: &[u8]) -> bool {
    match s.first() {
        Some(b'"' | b'\'') => scan_string_end(s, 0).is_some_and(|e| e + 1 == s.len()),
        _ => false,
    }
}

const SYSTEMS: &[&[u8]] = &[
    b"cyclic",
    b"numeric",
    b"alphabetic",
    b"symbolic",
    b"additive",
    b"fixed",
];

pub(crate) fn symbols_canonical(args: &[u8]) -> Option<Vec<u8>> {
    let toks = tokenize(args)?;
    let mut system: Option<&[u8]> = None;
    let mut i = 0;
    if let Some(first) = toks.first()
        && first.kind == Kind::Ident
    {
        system = SYSTEMS
            .iter()
            .rev()
            .find(|s| first.text.eq_ignore_ascii_case(s))
            .copied();
        system?;
        i = 1;
    }
    let mut out = Vec::new();
    if let Some(system) = system
        && system != b"symbolic"
    {
        out.extend_from_slice(system);
    }
    let mut n_symbols = 0;
    for t in &toks[i..] {
        let image = t.kind == Kind::Func
            && (t.text == b"url"
                || t.text == b"image"
                || t.text.windows(8).any(|w| w == b"gradient"));
        if t.kind != Kind::String && !image {
            return None;
        }
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(&t.text);
        if image {
            out.push(b'(');
            out.extend_from_slice(&t.args);
            out.push(b')');
        }
        n_symbols += 1;
    }
    let mut ok = n_symbols >= 1;
    if matches!(system, Some(b"numeric" | b"alphabetic")) && n_symbols < 2 {
        ok = false;
    }
    if system == Some(b"additive") {
        ok = false;
    }
    ok.then_some(out)
}

fn counter_style_canonical(style: &[u8]) -> Option<Vec<u8>> {
    if starts_with_symbols(style) {
        let canon = symbols_canonical(&style[8..style.len() - 1])?;
        let mut out = b"symbols(".to_vec();
        out.extend_from_slice(&canon);
        out.push(b')');
        return Some(out);
    }
    if !ident_valid(style) || wide_keyword_or_default(style) || style.eq_ignore_ascii_case(b"none")
    {
        return None;
    }
    if style.eq_ignore_ascii_case(b"decimal") {
        return Some(Vec::new());
    }
    Some(style.to_vec())
}

fn starts_with_symbols(style: &[u8]) -> bool {
    style.len() >= 8
        && style[..8].eq_ignore_ascii_case(b"symbols(")
        && style.last() == Some(&b')')
        && style.len() > 8
}

fn counter_canonical(t: &Token) -> Option<Vec<u8>> {
    let counters = t.text == b"counters";
    let args = split_args(&t.args)?;
    let (min_args, max_args) = if counters { (2, 3) } else { (1, 2) };
    if args.len() < min_args || args.len() > max_args {
        return None;
    }
    let name = &args[0];
    if !ident_valid(name) || name.eq_ignore_ascii_case(b"none") {
        return None;
    }
    let mut separator = None;
    if counters {
        let s = &args[1];
        if !is_string_token(s) {
            return None;
        }
        separator = Some(string_canonical(&s[1..s.len() - 1]));
    }
    let mut style = None;
    if args.len() == max_args {
        style = Some(counter_style_canonical(&args[max_args - 1])?);
    }
    let mut out = t.text.clone();
    out.push(b'(');
    out.extend_from_slice(name);
    if let Some(separator) = separator {
        out.extend_from_slice(b", ");
        out.extend_from_slice(&separator);
    }
    if let Some(style) = style.filter(|style| !style.is_empty()) {
        out.extend_from_slice(b", ");
        out.extend_from_slice(&style);
    }
    out.push(b')');
    Some(out)
}

fn func_is_image(t: &Token) -> bool {
    [
        &b"url"[..],
        b"image",
        b"image-set",
        b"cross-fade",
        b"element",
    ]
    .contains(&t.text.as_slice())
        || t.text.ends_with(b"-gradient")
}

fn function_text(t: &Token) -> Vec<u8> {
    let mut out = t.text.clone();
    out.push(b'(');
    out.extend_from_slice(&t.args);
    out.push(b')');
    out
}

const QUOTE_KEYWORDS: &[&[u8]] = &[
    b"open-quote",
    b"close-quote",
    b"no-open-quote",
    b"no-close-quote",
    b"contents",
];

fn item_canonical(t: &Token, alt: bool) -> Option<Vec<u8>> {
    match t.kind {
        Kind::String => return Some(t.text.clone()),
        Kind::Ident => {
            if alt {
                return None;
            }
            return QUOTE_KEYWORDS
                .iter()
                .find(|kw| t.text.eq_ignore_ascii_case(kw))
                .map(|kw| kw.to_vec());
        }
        Kind::Slash => return None,
        Kind::Func => {}
    }
    if t.text == b"counter" || t.text == b"counters" {
        return counter_canonical(t);
    }
    if t.text == b"attr" {
        let ok = split_args(&t.args).is_some_and(|args| {
            !args.is_empty() && args.len() <= 2 && !args[0].is_empty() && {
                let first = &args[0];
                let cut = first
                    .iter()
                    .position(|&c| c == b' ' || c == b'\t')
                    .unwrap_or(first.len());
                ident_valid(&first[..cut])
            }
        });
        return ok.then(|| function_text(t));
    }
    if alt {
        return None;
    }
    if t.text == b"image-set" || t.text == b"-webkit-image-set" {
        return image::image_set_canonical(&function_text(t), false);
    }
    if func_is_image(t)
        || t.text == b"leader"
        || t.text.starts_with(b"target-")
        || t.text == b"var"
        || t.text == b"string"
        || t.text == b"content"
    {
        return Some(function_text(t));
    }
    None
}

pub(crate) fn content_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let toks = tokenize(text)?;
    if toks.is_empty() {
        return None;
    }
    if toks.len() == 1 && toks[0].kind == Kind::Ident {
        let t = &toks[0].text;
        if t.eq_ignore_ascii_case(b"normal") || t.eq_ignore_ascii_case(b"none") {
            return Some(t.to_ascii_lowercase());
        }
    }
    let mut out = Vec::new();
    let mut alt = false;
    let (mut main_items, mut alt_items) = (0, 0);
    for t in &toks {
        if t.kind == Kind::Slash {
            if alt || main_items == 0 {
                return None;
            }
            alt = true;
            out.extend_from_slice(b" /");
            continue;
        }
        let item = item_canonical(t, alt)?;
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(&item);
        if alt {
            alt_items += 1;
        } else {
            main_items += 1;
        }
    }
    if main_items == 0 || (alt && alt_items == 0) {
        return None;
    }
    Some(out)
}
