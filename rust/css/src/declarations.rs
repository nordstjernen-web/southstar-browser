//! Southstar — a declaration block read into declarations: custom properties and values that wait for var(), attr() or container units set aside, var() fallbacks substituted, and the validity checks the CSSOM and @supports ask of one declaration.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::content;
use crate::ffi;
use crate::image::{self, unicode_range_canonical};
use crate::lex;
use crate::prop::Prop;
use crate::property::{self, contains, eq, wide_keyword};
use crate::scan::{
    byte, is_ident, is_ws, match_close_paren, scan_until, skip_comment, skip_to_block_end,
    skip_ws_comments, split_ws_paren, strip, strip_important, trim_range,
};
use crate::shorthand;
use crate::supports;
use crate::{animation, calc};

pub(crate) trait Sink {
    fn capturing(&self) -> bool;
    fn custom(&mut self, name: &[u8], value: &[u8], important: bool);
    fn pending(&mut self, name: &[u8], raw: &[u8], important: bool);
    fn declaration(&mut self, name: &[u8], text: &[u8], important: bool);
}

pub(crate) struct Collected {
    pub declarations: Vec<(Vec<u8>, Vec<u8>, bool)>,
}

impl Sink for Collected {
    fn capturing(&self) -> bool {
        false
    }

    fn custom(&mut self, _: &[u8], _: &[u8], _: bool) {}

    fn pending(&mut self, _: &[u8], _: &[u8], _: bool) {}

    fn declaration(&mut self, name: &[u8], text: &[u8], important: bool) {
        self.declarations
            .push((name.to_vec(), text.to_vec(), important));
    }
}

const CONTAINER_UNITS: [&[u8]; 6] = [b"cqw", b"cqh", b"cqi", b"cqb", b"cqmin", b"cqmax"];

const ATTR_UNITS: [&[u8]; 63] = [
    b"px", b"em", b"rem", b"ex", b"rex", b"ch", b"rch", b"cap", b"rcap", b"ic", b"ric", b"lh",
    b"rlh", b"vw", b"vh", b"vi", b"vb", b"vmin", b"vmax", b"svw", b"svh", b"svi", b"svb", b"svmin",
    b"svmax", b"lvw", b"lvh", b"lvi", b"lvb", b"lvmin", b"lvmax", b"dvw", b"dvh", b"dvi", b"dvb",
    b"dvmin", b"dvmax", b"cqw", b"cqh", b"cqi", b"cqb", b"cqmin", b"cqmax", b"cm", b"mm", b"q",
    b"in", b"pt", b"pc", b"deg", b"grad", b"rad", b"turn", b"s", b"ms", b"hz", b"khz", b"dpi",
    b"dpcm", b"dppx", b"x", b"fr", b"%",
];

const CSSOM_PROPERTIES: [&[u8]; 25] = [
    b"alignment-baseline",
    b"background-attachment",
    b"baseline-shift",
    b"baseline-source",
    b"background",
    b"border",
    b"column-rule",
    b"columns",
    b"empty-cells",
    b"flex",
    b"flex-flow",
    b"font",
    b"grid",
    b"grid-template",
    b"list-style",
    b"outline",
    b"page-break-after",
    b"page-break-before",
    b"page-break-inside",
    b"place-content",
    b"place-items",
    b"place-self",
    b"src",
    b"unicode-range",
    b"animation-range",
];

pub(crate) fn find_function(s: &[u8], mut p: usize, end: usize, name: &[u8]) -> Option<usize> {
    let n = name.len();
    let start = p;
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
        if end - p > n
            && s[p + n] == b'('
            && s[p..p + n].eq_ignore_ascii_case(name)
            && (p == start || !is_ident(s[p - 1]))
        {
            return Some(p);
        }
        p += 1;
    }
    None
}

pub(crate) fn substitute_var_fallbacks(text: &[u8], depth: u32) -> Vec<u8> {
    if depth > 16 {
        return text.to_vec();
    }
    let end = text.len();
    let mut out = Vec::with_capacity(end);
    let mut p = 0;
    while p < end {
        let Some(at) = find_function(text, p, end, b"var") else {
            out.extend_from_slice(&text[p..end]);
            break;
        };
        out.extend_from_slice(&text[p..at]);
        let args_start = at + 4;
        let (args_end, term) = scan_until(text, args_start, end, b")");
        if term != b')' {
            break;
        }
        let (comma, comma_term) = scan_until(text, args_start, args_end, b",");
        if comma_term == b',' {
            let nested = trim_range(text, comma + 1, args_end);
            out.extend_from_slice(&substitute_var_fallbacks(nested, depth + 1));
        }
        p = args_end + 1;
    }
    out
}

pub(crate) fn value_syntax_valid(text: &[u8]) -> bool {
    let (value, _) = strip_important(text);
    let end = value.len();
    let mut p = 0;
    let mut quote = 0u8;
    let (mut paren, mut bracket, mut brace) = (0u32, 0u32, 0u32);
    let mut valid = true;
    while p < end && valid {
        let c = value[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
                continue;
            }
            if c == quote {
                quote = 0;
            } else if matches!(c, b'\n' | b'\r' | 0x0c) {
                valid = false;
            }
            p += 1;
            continue;
        }
        if c == b'/' && p + 1 < end && value[p + 1] == b'*' {
            p = skip_comment(value, p, end);
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
        let top = paren == 0 && bracket == 0 && brace == 0;
        match c {
            b'(' => paren += 1,
            b')' if paren == 0 => valid = false,
            b')' => paren -= 1,
            b'[' => bracket += 1,
            b']' if bracket == 0 => valid = false,
            b']' => bracket -= 1,
            b'{' => brace += 1,
            b'}' if brace == 0 => valid = false,
            b'}' => brace -= 1,
            b'!' | b';' if top => valid = false,
            _ => {}
        }
        p += 1;
    }
    valid && quote == 0 && paren == 0 && bracket == 0 && brace == 0
}

pub(crate) fn has_container_unit(text: &[u8]) -> bool {
    (1..text.len()).any(|p| {
        let prev = text[p - 1];
        (prev.is_ascii_digit() || prev == b'.')
            && CONTAINER_UNITS.iter().any(|unit| {
                text.len() - p >= unit.len()
                    && text[p..p + unit.len()].eq_ignore_ascii_case(unit)
                    && !is_ident(byte(text, p + unit.len()))
            })
    })
}

pub(crate) fn attr_unit_ident_valid(unit: &[u8]) -> bool {
    ATTR_UNITS.iter().any(|known| eq(unit, known))
}

fn attr_args_valid(args: &[u8]) -> bool {
    let end = args.len();
    let (comma_at, term) = scan_until(args, 0, end, b",");
    let comma = (term == b',').then_some(comma_at);
    let head = trim_range(args, 0, comma.unwrap_or(end));
    let toks = split_ws_paren(head, 4);
    let mut ok = (1..=2).contains(&toks.len()) && content::ident_valid(toks[0]);
    if ok && toks.len() == 2 {
        let kind = toks[1];
        ok = if eq(kind, b"raw-string") {
            true
        } else if kind.len() >= 5
            && kind[..5].eq_ignore_ascii_case(b"type(")
            && kind.last() == Some(&b')')
        {
            let inner = strip(&kind[5..kind.len() - 1]);
            inner == b"*" || (ffi::syntax_def_valid(inner) && inner.contains(&b'<'))
        } else {
            attr_unit_ident_valid(kind)
        };
    }
    if let (true, Some(comma)) = (ok, comma) {
        ok = attr_functions_valid(trim_range(args, comma + 1, end));
    }
    ok
}

pub(crate) fn attr_functions_valid(text: &[u8]) -> bool {
    let end = text.len();
    let mut p = 0;
    while p < end {
        if text[p] == b'"' || text[p] == b'\'' {
            let Some(close) = image::quoted_end(&text[p + 1..], text[p]) else {
                return true;
            };
            p += 1 + close + 1;
            continue;
        }
        if end - p >= 5
            && text[p..p + 5].eq_ignore_ascii_case(b"attr(")
            && (p == 0 || !is_ident(text[p - 1]))
        {
            let mut depth = 1;
            let mut q = p + 5;
            while q < end && depth > 0 {
                if text[q] == b'"' || text[q] == b'\'' {
                    let Some(close) = image::quoted_end(&text[q + 1..], text[q]) else {
                        return false;
                    };
                    q += 1 + close + 1;
                    continue;
                }
                match text[q] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    q += 1;
                }
            }
            if depth != 0 || !attr_args_valid(&text[p + 5..q]) {
                return false;
            }
            p = q + 1;
            continue;
        }
        p += 1;
    }
    true
}

pub(crate) fn parse_block(s: &[u8], mut p: usize, sink: &mut impl Sink) -> usize {
    let end = s.len();
    let skip_semicolon = |p: usize| if p < end && s[p] == b';' { p + 1 } else { p };
    while p < end && s[p] != b'}' {
        p = skip_ws_comments(s, p, end);
        while p < end && s[p] == b';' {
            p = skip_ws_comments(s, p + 1, end);
        }
        if p >= end || s[p] == b'}' {
            break;
        }
        let name = lex::read_ident(s, &mut p, end);
        if name.is_empty() {
            let before = p;
            let (skip_to, term) = scan_until(s, p, end, b"{;}");
            p = match term {
                b'{' => skip_to_block_end(s, skip_to, end),
                b';' => skip_to + 1,
                _ => skip_to,
            };
            if p <= before {
                p = before + 1;
            }
            continue;
        }
        let name = if name.starts_with(b"--") {
            name
        } else {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with(b"-webkit-border-") && lower.ends_with(b"-radius") {
                lower[8..].to_vec()
            } else {
                lower
            }
        };
        p = skip_ws_comments(s, p, end);
        if p >= end || s[p] != b':' {
            let (skip_to, term) = scan_until(s, p, end, b"{;}");
            p = if term == b';' { skip_to + 1 } else { skip_to };
            continue;
        }
        let vstart = p + 1;
        let (vend, _) = scan_until(s, vstart, end, b";}");
        p = vend;
        let raw = &s[vstart..vend];
        if !value_syntax_valid(raw) {
            p = skip_semicolon(p);
            continue;
        }
        if sink.capturing() && name.starts_with(b"--") && name.len() > 2 {
            let (value, important) = strip_important(strip(raw));
            sink.custom(&name, value, important);
            p = skip_semicolon(p);
            continue;
        }
        if contains(raw, b"attr(") && !attr_functions_valid(raw) {
            p = skip_semicolon(p);
            continue;
        }
        if sink.capturing()
            && (contains(raw, b"var(") || contains(raw, b"attr(") || has_container_unit(raw))
        {
            let (raw, important) = strip_important(raw);
            sink.pending(&name, raw, important);
            p = skip_semicolon(p);
            continue;
        }
        let substituted = substitute_var_fallbacks(raw, 0);
        let (text, important) = strip_important(&substituted);
        sink.declaration(&name, text, important);
        p = skip_semicolon(p);
    }
    if p < end && s[p] == b'}' {
        p += 1;
    }
    p
}

pub(crate) fn expands(name: &[u8], text: &[u8]) -> bool {
    !shorthand::expand(name, text).is_empty()
}

pub(crate) fn named_property_supported(name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.starts_with(b"--") && name.len() > 2 {
        return true;
    }
    if eq(name, b"all") || ffi::prop_named(name).is_some() || eq(name, b"unicode-range") {
        return true;
    }
    if CSSOM_PROPERTIES.iter().any(|known| eq(name, known)) {
        return true;
    }
    let declaration = [name, b": initial;"].concat();
    let mut collected = Collected {
        declarations: Vec::new(),
    };
    parse_block(&declaration, 0, &mut collected);
    collected
        .declarations
        .iter()
        .any(|(name, text, _)| expands(name, text))
}

pub(crate) fn declaration_valid(prop: Option<Prop>, text: &[u8]) -> bool {
    if text.is_empty() {
        return true;
    }
    if contains(text, b"attr(") && !attr_functions_valid(text) {
        return false;
    }
    if contains(text, b"var(") || contains(text, b"attr(") {
        return true;
    }
    property::parse_for(prop, text).is_some()
}

pub(crate) fn named_declaration_valid(name: &[u8], text: &[u8]) -> bool {
    if text.is_empty() {
        return true;
    }
    if !named_property_supported(name) {
        return false;
    }
    if name.starts_with(b"--") {
        return value_syntax_valid(text);
    }
    if eq(name, b"border-image") || eq(name, b"-webkit-border-image") {
        return contains(text, b"var(") || expands(b"border-image", text);
    }
    if eq(name, b"unicode-range") {
        return unicode_range_canonical(text).is_some();
    }
    if eq(name, b"animation-range") && !contains(text, b"var(") {
        return wide_keyword(text).is_some() || animation::range_shorthand_expand(text).is_some();
    }
    if !eq(name, b"all") {
        if let Some(prop) = ffi::prop_named(name)
            && declaration_valid(Some(prop), text)
        {
            return true;
        }
        return supports::declaration(name, text);
    }
    contains(text, b"var(") || wide_keyword(text).is_some()
}

fn sizes_is_length_fn(p: &[u8]) -> bool {
    [&b"calc("[..], b"min(", b"max(", b"clamp("]
        .iter()
        .any(|f| p.len() >= f.len() && p[..f.len()].eq_ignore_ascii_case(f))
}

pub(crate) fn sizes_resolve(sizes: &[u8]) -> f64 {
    let (viewport_w, _) = ffi::viewport();
    let end = sizes.len();
    let mut p = 0;
    while p < end {
        while p < end && (is_ws(sizes[p]) || sizes[p] == b',') {
            p += 1;
        }
        if p >= end {
            break;
        }
        let entry = p;
        while p < end && sizes[p] != b',' {
            if sizes[p] == b'(' {
                p = match_close_paren(sizes, p + 1, end).map_or(end, |close| close + 1);
            } else {
                p += 1;
            }
        }
        let entry_end = p;
        let mut q = entry;
        let mut len_start = None;
        while q < entry_end {
            while q < entry_end && is_ws(sizes[q]) {
                q += 1;
            }
            if q >= entry_end {
                break;
            }
            let c = sizes[q];
            if sizes_is_length_fn(&sizes[q..])
                || c.is_ascii_digit()
                || matches!(c, b'.' | b'+' | b'-')
            {
                len_start = Some(q);
                break;
            }
            if c == b'(' {
                q = match_close_paren(sizes, q + 1, entry_end).map_or(entry_end, |close| close + 1);
            } else {
                while q < entry_end && !is_ws(sizes[q]) {
                    q += 1;
                }
            }
        }
        let Some(len_start) = len_start else {
            continue;
        };
        let condition = strip(&sizes[entry..len_start]);
        if !condition.is_empty() && !ffi::media_query_matches(condition) {
            continue;
        }
        let (ok, resolved) = calc::resolve_to_px_pct(&sizes[len_start..entry_end], false);
        let px = if ok {
            resolved.px + resolved.pct * 0.01 * viewport_w
        } else {
            -1.0
        };
        if px > 0.0 {
            return px;
        }
    }
    viewport_w
}
