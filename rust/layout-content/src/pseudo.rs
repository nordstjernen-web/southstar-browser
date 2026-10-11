//! Southstar — the text ::before and ::after generate: strings, attr(), counter() and counters(), quotes, and the text-transform applied to collected text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_css::append_unescaped;
use southstar_dom::Node;
use southstar_style::{PropId, StyleRef, ValueRef};

use crate::counters::{is_space, substitute};
use crate::ffi;
use crate::text_input::utf8_skip;

fn unescape_all(raw: &[u8], out: &mut Vec<u8>) {
    let mut r = 0;
    while r < raw.len() {
        append_unescaped(out, raw, &mut r);
    }
}

fn host_attr<'a>(host: Node<'a>, name: &[u8]) -> Option<&'a CStr> {
    host.attr(&CString::new(name).ok()?)
}

pub fn resolve_content(raw: &[u8], host: Option<Node<'_>>) -> Option<Vec<u8>> {
    if raw.is_empty() || raw == b"none" || raw == b"normal" {
        return None;
    }
    let has_func = raw.contains(&b'(');
    let has_string = raw.contains(&b'"') || raw.contains(&b'\'');
    if !has_func && !has_string {
        return Some(raw.to_vec());
    }
    let at = |i: usize| raw.get(i).copied().unwrap_or(0);
    let mut out = Vec::new();
    let mut p = 0;
    while at(p) != 0 {
        while at(p) != 0 && is_space(at(p)) {
            p += 1;
        }
        if at(p) == 0 || at(p) == b'/' {
            break;
        }
        let rest = &raw[p..];
        if at(p) == b'"' || at(p) == b'\'' {
            let q = at(p);
            p += 1;
            let start = p;
            while at(p) != 0 && at(p) != q {
                if at(p) == b'\\' && at(p + 1) != 0 {
                    p += 2;
                    continue;
                }
                p += 1;
            }
            unescape_all(&raw[start..p.min(raw.len())], &mut out);
            if at(p) == q {
                p += 1;
            }
        } else if rest.starts_with(b"attr(") {
            p += 5;
            while at(p) == b' ' {
                p += 1;
            }
            let start = p;
            while at(p) != 0 && !matches!(at(p), b')' | b',' | b' ') {
                p += 1;
            }
            if let Some(host) = host.filter(|_| p != start)
                && let Some(val) = host_attr(host, &raw[start..p])
            {
                out.extend_from_slice(val.to_bytes());
            }
            while at(p) != 0 && at(p) != b')' {
                p += 1;
            }
            if at(p) == b')' {
                p += 1;
            }
        } else if rest.starts_with(b"counter(") || rest.starts_with(b"counters(") {
            p += if at(p + 7) == b's' { 9 } else { 8 };
            let body_start = p;
            let mut depth = 1;
            while at(p) != 0 && depth > 0 {
                if at(p) == b'(' {
                    depth += 1;
                } else if at(p) == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                p += 1;
            }
            let body = &raw[body_start..p];
            if at(p) == b')' {
                p += 1;
            }
            out.extend_from_slice(&substitute(body, host));
        } else {
            let id = p;
            while at(p).is_ascii_alphanumeric() || at(p) == b'-' {
                p += 1;
            }
            if at(p) == b'(' {
                let mut depth = 0;
                while at(p) != 0 {
                    if at(p) == b'(' {
                        depth += 1;
                    } else if at(p) == b')' {
                        depth -= 1;
                        if depth == 0 {
                            p += 1;
                            break;
                        }
                    }
                    p += 1;
                }
            } else {
                while at(p) != 0 && !is_space(at(p)) && at(p) != b'"' && at(p) != b'\'' {
                    p += 1;
                }
                if p > id {
                    out.extend_from_slice(&raw[id..p]);
                } else {
                    p += 1;
                }
            }
        }
    }
    Some(out)
}

pub fn quotes_string(s: Option<StyleRef<'_>>, depth: c_int, closing: bool) -> Vec<u8> {
    let depth = depth.max(0) as usize;
    let spec = s
        .and_then(|s| s.get(PropId::Quotes))
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes);
    if spec == Some(b"none") {
        return Vec::new();
    }
    if let Some(spec) = spec.filter(|sp| sp.contains(&b'"') || sp.contains(&b'\'')) {
        let at = |i: usize| spec.get(i).copied().unwrap_or(0);
        let mut parts = Vec::new();
        let mut p = 0;
        while at(p) != 0 {
            if at(p) == b'"' || at(p) == b'\'' {
                let term = at(p);
                p += 1;
                let mut part = Vec::new();
                while at(p) != 0 && at(p) != term {
                    append_unescaped(&mut part, spec, &mut p);
                }
                parts.push(part);
                if at(p) != 0 {
                    p += 1;
                }
            } else {
                p += 1;
            }
        }
        let pairs = parts.len() / 2;
        if pairs > 0 {
            let pair = depth.min(pairs - 1);
            return parts.swap_remove(pair * 2 + usize::from(closing));
        }
    }
    let quote = match (depth.is_multiple_of(2), closing) {
        (true, false) => "\u{201C}",
        (true, true) => "\u{201D}",
        (false, false) => "\u{2018}",
        (false, true) => "\u{2019}",
    };
    quote.as_bytes().to_vec()
}

pub fn append_content(
    out: &mut Vec<u8>,
    cv: Option<ValueRef<'_>>,
    host: Option<Node<'_>>,
    host_style: Option<StyleRef<'_>>,
) {
    let Some(raw) = cv.and_then(ValueRef::keyword_text) else {
        return;
    };
    let Some(resolved) = resolve_content(raw.to_bytes(), host) else {
        return;
    };
    match resolved.as_slice() {
        b"open-quote" => out.extend_from_slice(&quotes_string(host_style, 0, false)),
        b"close-quote" => out.extend_from_slice(&quotes_string(host_style, 0, true)),
        b"no-open-quote" | b"no-close-quote" => {}
        text => out.extend_from_slice(text),
    }
}

pub fn generates_box(ps: Option<StyleRef<'_>>) -> bool {
    ps.and_then(|ps| ps.get(PropId::Content))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes() != b"none" && kw.to_bytes() != b"normal")
}

pub fn text_transform(src: &CStr, tt: &[u8]) -> Option<Vec<u8>> {
    match tt {
        b"uppercase" => Some(ffi::utf8_strup(src)),
        b"lowercase" => Some(ffi::utf8_strdown(src)),
        b"capitalize" => {
            let bytes = src.to_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut at_word_start = true;
            let mut p = 0;
            while p < bytes.len() {
                let c = ffi::utf8_get_char(src, p);
                let next = (p + utf8_skip(bytes[p])).min(bytes.len());
                if ffi::unichar_isspace(c) || c == u32::from(b'-') || c == u32::from(b'/') {
                    out.extend_from_slice(&bytes[p..next]);
                    at_word_start = true;
                } else if at_word_start {
                    out.extend_from_slice(&ffi::unichar_title_utf8(c));
                    at_word_start = false;
                } else {
                    out.extend_from_slice(&bytes[p..next]);
                }
                p = next;
            }
            Some(out)
        }
        _ => None,
    }
}
