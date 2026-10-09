//! Southstar — @supports conditions and CSS.supports(): declarations tried by parsing them as a rule, selector() through the selector parser, font-tech() and font-format() and other functions as unsupported, combined with not, and and or.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::Cell;

use crate::ffi;
use crate::lex::read_ident;
use crate::scan::{is_ws, scan_until, skip_ws_comments, strip};
use crate::selector;

const MAX_AT_NESTING: i32 = 32;

thread_local! {
    static PARSE_DEPTH: Cell<i32> = const { Cell::new(0) };
}

pub(crate) fn declaration(property: &[u8], value: &[u8]) -> bool {
    if PARSE_DEPTH.get() >= MAX_AT_NESTING {
        return false;
    }
    let property = strip(property);
    let value = strip(value);
    let mut scan = 0;
    let name = read_ident(property, &mut scan, property.len());
    let property_valid = !name.is_empty() && scan == property.len();
    let empty_custom = property.starts_with(b"--") && property.len() > 2;
    let (value_scan, _) = scan_until(value, 0, value.len(), b";}");
    if property.is_empty()
        || (value.is_empty() && !empty_custom)
        || !property_valid
        || value_scan != value.len()
    {
        return false;
    }
    let css = [b"x{".as_slice(), property, b":", value, b"}"].concat();
    PARSE_DEPTH.set(PARSE_DEPTH.get() + 1);
    let declares = ffi::first_rule_declares(&css);
    PARSE_DEPTH.set(PARSE_DEPTH.get() - 1);
    declares
}

fn feature_matches(src: &[u8]) -> bool {
    let s = strip(src);
    let (colon, term) = scan_until(s, 0, s.len(), b":");
    if term != b':' {
        return false;
    }
    declaration(strip(&s[..colon]), strip(&s[colon + 1..]))
}

fn match_kw(s: &[u8], p: usize, kw: &[u8]) -> bool {
    let end = s.len();
    let n = kw.len();
    if end - p.min(end) < n || !s[p..p + n].eq_ignore_ascii_case(kw) {
        return false;
    }
    if p + n == end {
        return true;
    }
    let c = s[p + n];
    is_ws(c) || (c == b'/' && p + n + 1 < end && s[p + n + 1] == b'*')
}

fn function_start(s: &[u8], p: usize) -> bool {
    let mut q = p;
    let name = read_ident(s, &mut q, s.len());
    !name.is_empty() && q < s.len() && s[q] == b'('
}

fn term(s: &[u8], pp: &mut usize, depth: i32) -> bool {
    let end = s.len();
    if depth > MAX_AT_NESTING {
        *pp = end;
        return false;
    }
    let mut p = skip_ws_comments(s, *pp, end);
    let mut negate = false;
    if match_kw(s, p, b"not") {
        negate = true;
        p = skip_ws_comments(s, p + 3, end);
    }
    if end - p > 9 && s[p..p + 9].eq_ignore_ascii_case(b"selector(") {
        p += 9;
        let start = p;
        let (sel_end, term) = scan_until(s, p, end, b")");
        p = if term == b')' { sel_end + 1 } else { sel_end };
        *pp = p;
        return selector::supports_selector(&s[start..sel_end]) != negate;
    }
    if function_start(s, p) {
        let mut q = p;
        read_ident(s, &mut q, end);
        let (close, term) = scan_until(s, q + 1, end, b")");
        *pp = if term == b')' { close + 1 } else { close };
        return negate && term == b')';
    }
    if p >= end || s[p] != b'(' {
        *pp = p;
        return false;
    }
    p = skip_ws_comments(s, p + 1, end);
    let nested = (p < end && s[p] == b'(') || match_kw(s, p, b"not") || function_start(s, p);
    let result = if nested {
        let result = expr(s, &mut p, depth + 1);
        p = skip_ws_comments(s, p, end);
        result
    } else {
        let start = p;
        let (feature_end, _) = scan_until(s, p, end, b")");
        p = feature_end;
        feature_matches(&s[start..feature_end])
    };
    if p >= end || s[p] != b')' {
        *pp = p;
        return false;
    }
    *pp = p + 1;
    result != negate
}

fn expr(s: &[u8], pp: &mut usize, depth: i32) -> bool {
    let mut acc = term(s, pp, depth);
    let mut p = *pp;
    let mut op = 0;
    loop {
        p = skip_ws_comments(s, p, s.len());
        let (this_op, kw_len) = if match_kw(s, p, b"and") {
            (1, 3)
        } else if match_kw(s, p, b"or") {
            (2, 2)
        } else {
            break;
        };
        if op != 0 && op != this_op {
            *pp = p;
            return false;
        }
        op = this_op;
        *pp = p + kw_len;
        let rhs = term(s, pp, depth);
        p = *pp;
        acc = if this_op == 1 { acc && rhs } else { acc || rhs };
    }
    *pp = p;
    acc
}

pub(crate) fn condition(condition: &[u8], allow_bare_declaration: bool) -> bool {
    let query = strip(condition);
    if allow_bare_declaration {
        let (colon, term) = scan_until(query, 0, query.len(), b":");
        if term == b':' {
            return declaration(strip(&query[..colon]), strip(&query[colon + 1..]));
        }
    }
    let mut p = 0;
    let result = expr(query, &mut p, 0);
    result && skip_ws_comments(query, p, query.len()) == query.len()
}
