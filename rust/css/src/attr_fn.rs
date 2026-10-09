//! Southstar — attr() substituted into a declaration value: the attribute read as a quoted string, as any value, against a type() syntax or with a unit, falling back through nested fallbacks, and the taint that keeps attribute text out of url() and the image functions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::{Node, attrs};

use crate::content;
use crate::declarations;
use crate::ffi::{self, SyntaxDef};
use crate::image::quoted_end;
use crate::scan::{is_ident, scan_until, split_ws_paren, strip, trim_range};
use crate::units;

const FALLBACK_DEPTH_MAX: i32 = 8;
const PAREN_STACK_MAX: usize = 64;
const URL_FUNCTIONS: [&[u8]; 5] = [b"url", b"src", b"image", b"image-set", b"-webkit-image-set"];

enum Kind {
    String,
    Any,
    Syntax(SyntaxDef),
    Unit,
}

fn contains(text: &[u8], needle: &[u8]) -> bool {
    text.windows(needle.len()).any(|w| w == needle)
}

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn string_quote(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.push(b'"');
    for &c in text {
        if c == b'"' || c == b'\\' {
            out.push(b'\\');
            out.push(c);
        } else if c < 0x20 || c == 0x7f {
            out.extend_from_slice(format!("\\{c:x} ").as_bytes());
        } else {
            out.push(c);
        }
    }
    out.push(b'"');
    out
}

fn attr_kind(ty: Option<&[u8]>) -> Option<Kind> {
    let Some(ty) = ty else {
        return Some(Kind::String);
    };
    if ty.eq_ignore_ascii_case(b"raw-string") {
        return Some(Kind::String);
    }
    if starts_ci(ty, b"type(") && ty.last() == Some(&b')') {
        let inner = strip(&ty[5..ty.len() - 1]);
        if inner == b"*" {
            return Some(Kind::Any);
        }
        let syntax = SyntaxDef::parse(inner)?;
        return inner.contains(&b'<').then_some(Kind::Syntax(syntax));
    }
    declarations::attr_unit_ident_valid(ty).then_some(Kind::Unit)
}

fn typed_value(kind: &Kind, raw: &[u8], ty: Option<&[u8]>) -> Option<Vec<u8>> {
    let value = strip(raw);
    let plain = !value.is_empty() && !contains(value, b"var(") && !contains(value, b"attr(");
    match kind {
        Kind::String => Some(string_quote(raw)),
        Kind::Any => (plain && declarations::value_syntax_valid(value)).then(|| value.to_vec()),
        Kind::Syntax(syntax) => (plain && syntax.matches(value)).then(|| value.to_vec()),
        Kind::Unit => {
            let text = CString::new(value).unwrap_or_default();
            let (num, end) = ffi::strtod(&text, 0);
            (end != 0 && end == value.len())
                .then(|| [units::number_text(num), ty.unwrap_or_default().to_vec()].concat())
        }
    }
}

fn attr_function_value(args: &[u8], node: Option<Node<'_>>, depth: i32) -> Option<Vec<u8>> {
    let end = args.len();
    let (comma, term) = scan_until(args, 0, end, b",");
    let head = trim_range(args, 0, comma);
    let fallback = (term == b',').then(|| trim_range(args, comma + 1, end));
    let toks = split_ws_paren(head, 4);
    if toks.is_empty() || toks.len() > 2 || !content::ident_valid(toks[0]) {
        return None;
    }
    let ty = toks.get(1).copied();
    let kind = attr_kind(ty)?;
    let name = CString::new(toks[0].to_ascii_lowercase()).unwrap_or_default();
    let raw = node
        .filter(|n| n.is_element())
        .and_then(|n| attrs::get(n, &name));
    let result = raw.and_then(|raw| typed_value(&kind, raw.to_bytes(), ty));
    if result.is_some() {
        return result;
    }
    match fallback {
        Some(fallback) if depth < FALLBACK_DEPTH_MAX => {
            substitute(fallback, node, depth + 1, &mut false)
        }
        None if matches!(kind, Kind::String) => Some(b"\"\"".to_vec()),
        _ => None,
    }
}

fn url_function_at(text: &[u8], open_paren: usize) -> bool {
    let start = open_paren
        - text[..open_paren]
            .iter()
            .rev()
            .take_while(|&&c| is_ident(c) || c == b'-')
            .count();
    let name = &text[start..open_paren];
    URL_FUNCTIONS.iter().any(|f| name.eq_ignore_ascii_case(f))
}

fn quote_close(text: &[u8], open: usize) -> Option<usize> {
    quoted_end(&text[open + 1..], text[open]).map(|i| open + 1 + i)
}

pub(crate) fn substitute(
    text: &[u8],
    node: Option<Node<'_>>,
    depth: i32,
    tainted: &mut bool,
) -> Option<Vec<u8>> {
    if !contains(text, b"attr(") {
        return Some(text.to_vec());
    }
    let end = text.len();
    let mut out = Vec::with_capacity(end);
    let mut url_depth = 0;
    let mut stack: Vec<bool> = Vec::new();
    let mut p = 0;
    while p < end {
        let c = text[p];
        if c == b'"' || c == b'\'' {
            let Some(q) = quote_close(text, p) else {
                out.extend_from_slice(&text[p..]);
                break;
            };
            out.extend_from_slice(&text[p..=q]);
            p = q + 1;
            continue;
        }
        if c == b'(' && stack.len() < PAREN_STACK_MAX {
            let url = url_function_at(text, p);
            stack.push(url);
            url_depth += i32::from(url);
        } else if c == b')' {
            if let Some(url) = stack.pop() {
                url_depth -= i32::from(url);
            }
        }
        if starts_ci(&text[p..], b"attr(") && (p == 0 || !is_ident(text[p - 1])) {
            if url_depth > 0 {
                *tainted = true;
            }
            let mut d = 1;
            let mut q = p + 5;
            while q < end && d > 0 {
                if text[q] == b'"' || text[q] == b'\'' {
                    match quote_close(text, q) {
                        Some(close) => {
                            q = close + 1;
                            continue;
                        }
                        None => break,
                    }
                }
                if text[q] == b'(' {
                    d += 1;
                } else if text[q] == b')' {
                    d -= 1;
                }
                if d > 0 {
                    q += 1;
                }
            }
            if d != 0 {
                return None;
            }
            let value = attr_function_value(&text[p + 5..q], node, depth)?;
            if contains(&value, b"url(")
                || contains(&value, b"src(")
                || contains(&value, b"image-set(")
            {
                *tainted = true;
            }
            out.extend_from_slice(&value);
            p = q + 1;
            continue;
        }
        out.push(c);
        p += 1;
    }
    Some(out)
}
