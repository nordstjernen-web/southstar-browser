//! Southstar — matches keyword-shaped CSS values against a compact value grammar and serializes them in canonical order.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use southstar_css_syntax::{Component, Kind};

#[cfg(test)]
mod tests;

const GROUPS_MAX: usize = 8;

const CSS_WIDE: [&[u8]; 6] = [
    b"initial",
    b"inherit",
    b"unset",
    b"revert",
    b"revert-layer",
    b"default",
];

#[derive(Clone, Copy)]
struct Group<'g> {
    terms: &'g [u8],
    required: bool,
    min_count: usize,
    max_count: usize,
}

struct Matcher<'a> {
    grammar: &'a [u8],
    src: &'a [u8],
    number: &'a dyn Fn(&[u8]) -> Option<f64>,
}

fn trim_spaces(mut s: &[u8]) -> &[u8] {
    while let [b' ', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' '] = s {
        s = rest;
    }
    s
}

fn skip_spaces(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| c != b' ').unwrap_or(s.len());
    &s[start..]
}

fn find(s: &[u8], c: u8) -> Option<usize> {
    s.iter().position(|&x| x == c)
}

fn is_css_wide(word: &[u8]) -> bool {
    CSS_WIDE.iter().any(|w| w.eq_ignore_ascii_case(word))
}

fn leading_int(s: &[u8]) -> usize {
    let digits = skip_spaces(s);
    let end = digits
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    core::str::from_utf8(&digits[..end])
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0)
}

fn range_bound(s: &[u8], fallback: f64) -> f64 {
    let s = trim_spaces(s);
    if s.is_empty() {
        return fallback;
    }
    if s.len() >= 3 && s[..3].eq_ignore_ascii_case(b"inf") {
        return f64::INFINITY;
    }
    if s.len() >= 4 && s[..4].eq_ignore_ascii_case(b"-inf") {
        return f64::NEG_INFINITY;
    }
    core::str::from_utf8(s)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(fallback)
}

fn in_range(ty: &[u8], v: f64) -> bool {
    let Some(open) = find(ty, b'[') else {
        return true;
    };
    let inner = &ty[open + 1..];
    let (Some(comma), Some(close)) = (find(inner, b','), find(inner, b']')) else {
        return true;
    };
    if comma > close {
        return true;
    }
    v >= range_bound(&inner[..comma], f64::NEG_INFINITY)
        && v <= range_bound(&inner[comma + 1..close], f64::INFINITY)
}

fn type_is(ty: &[u8], name: &[u8]) -> bool {
    ty.len() > name.len() + 1
        && &ty[1..=name.len()] == name
        && matches!(ty[name.len() + 1], b'>' | b' ')
}

fn unescape_into(out: &mut Vec<u8>, s: &[u8], pos: &mut usize) {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut p = *pos + 1;
    if at(p).is_ascii_hexdigit() {
        let mut cp = 0u32;
        let mut n = 0;
        while n < 6 && at(p).is_ascii_hexdigit() {
            cp = cp * 16 + char::from(at(p)).to_digit(16).unwrap_or(0);
            p += 1;
            n += 1;
        }
        if matches!(at(p), b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            let cr = at(p) == b'\r';
            p += 1;
            if cr && at(p) == b'\n' {
                p += 1;
            }
        }
        let valid = cp != 0 && cp <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&cp);
        let ch = if valid {
            char::from_u32(cp).unwrap_or('\u{FFFD}')
        } else {
            '\u{FFFD}'
        };
        let mut buf = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    } else {
        out.push(at(p));
        p += 1;
    }
    *pos = p;
}

fn string_serialize(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![b'"'];
    let mut p = 0;
    while p < raw.len() {
        if raw[p] == b'\\' && p + 1 < raw.len() {
            let mut one = Vec::new();
            unescape_into(&mut one, raw, &mut p);
            for &c in &one {
                if c == b'"' || c == b'\\' {
                    out.push(b'\\');
                }
                out.push(c);
            }
            continue;
        }
        if raw[p] == b'"' {
            out.push(b'\\');
        }
        out.push(raw[p]);
        p += 1;
    }
    out.push(b'"');
    out
}

fn number_text(v: f64) -> Vec<u8> {
    southstar_numfmt::g(v).into_bytes()
}

impl<'a> Matcher<'a> {
    fn text(&self, c: &Component) -> &'a [u8] {
        &self.src[c.start..c.end]
    }

    fn value(&self, c: &Component) -> &'a [u8] {
        c.value.clone().map_or(&[][..], |r| &self.src[r])
    }

    fn component_number(&self, c: &Component) -> Option<f64> {
        match c.kind {
            Kind::Number => Some(c.number),
            Kind::Function => (self.number)(self.text(c)),
            _ => None,
        }
    }

    fn url_serialize(&self, c: &Component) -> Vec<u8> {
        let text = self.text(c);
        let inner = trim_spaces(text.get(4..text.len().saturating_sub(1)).unwrap_or(&[]));
        let n = inner.len();
        let body = if n >= 2 && matches!(inner[0], b'"' | b'\'') && inner[n - 1] == inner[0] {
            &inner[1..n - 1]
        } else {
            inner
        };
        let mut out = b"url(".to_vec();
        out.extend(string_serialize(body));
        out.push(b')');
        out
    }

    fn type_match(&self, ty: &[u8], c: &Component) -> Option<Vec<u8>> {
        if type_is(ty, b"string") {
            return (c.kind == Kind::String).then(|| string_serialize(self.value(c)));
        }
        if type_is(ty, b"integer") {
            let v = self.component_number(c)?;
            if c.kind == Kind::Number
                && self.text(c).iter().any(|b| matches!(b, b'.' | b'e' | b'E'))
            {
                return None;
            }
            let v = v.round();
            return in_range(ty, v).then(|| format!("{v:.0}").into_bytes());
        }
        if type_is(ty, b"number") {
            let v = self.component_number(c)?;
            return in_range(ty, v).then(|| number_text(v));
        }
        if type_is(ty, b"percentage") {
            if c.kind != Kind::Percentage || !in_range(ty, c.number) {
                return None;
            }
            let mut out = number_text(c.number);
            out.push(b'%');
            return Some(out);
        }
        if type_is(ty, b"angle") {
            if c.kind == Kind::Number && c.number == 0.0 {
                return Some(b"0deg".to_vec());
            }
            if c.kind != Kind::Dimension {
                return None;
            }
            let unit = self.value(c);
            let factor = [
                (&b"deg"[..], 1.0),
                (b"grad", 0.9),
                (b"rad", 180.0 / core::f64::consts::PI),
                (b"turn", 360.0),
            ]
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(unit))
            .map(|&(_, f)| f)?;
            let mut out = number_text(c.number * factor);
            out.extend_from_slice(b"deg");
            return Some(out);
        }
        if type_is(ty, b"dashed-ident") {
            return (c.kind == Kind::Ident && self.value(c).starts_with(b"--"))
                .then(|| self.text(c).to_vec());
        }
        if type_is(ty, b"custom-ident") {
            let name = self.value(c);
            let ok = c.kind == Kind::Ident
                && !is_css_wide(name)
                && !has_keyword_bytes(self.grammar, &name.to_ascii_lowercase());
            return ok.then(|| self.text(c).to_vec());
        }
        if type_is(ty, b"url") {
            let is_url = c.kind == Kind::Function && self.value(c).eq_ignore_ascii_case(b"url");
            return is_url.then(|| self.url_serialize(c));
        }
        None
    }

    fn term_match(&self, term: &[u8], c: &Component) -> Option<Vec<u8>> {
        if term.first() == Some(&b'<') {
            return self.type_match(term, c);
        }
        let word = term.strip_suffix(b"*").unwrap_or(term);
        let name = self.value(c);
        (c.kind == Kind::Ident && name.eq_ignore_ascii_case(word))
            .then(|| name.to_ascii_lowercase())
    }

    fn group_match(&self, terms: &[u8], c: &Component) -> Option<(Vec<u8>, bool)> {
        let mut p = terms;
        loop {
            p = skip_spaces(p);
            if p.is_empty() {
                return None;
            }
            let end = term_end(p);
            let term = &p[..end];
            if let Some(hit) = self.term_match(term, c) {
                return Some((hit, term.last() == Some(&b'*')));
            }
            p = &p[end..];
        }
    }

    fn repeat_match(&self, g: &Group, tokens: &[&Component]) -> Option<Vec<u8>> {
        if tokens.len() < g.min_count || tokens.len() > g.max_count {
            return None;
        }
        let mut out = Vec::new();
        for c in tokens {
            let (hit, _) = self.group_match(g.terms, c)?;
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend(hit);
        }
        Some(out)
    }

    fn any_order_match(&self, groups: &[Group], tokens: &[&Component]) -> Option<Vec<u8>> {
        if tokens.is_empty() {
            return None;
        }
        let mut slots: [Option<(Vec<u8>, bool)>; GROUPS_MAX] = Default::default();
        for c in tokens {
            let hit = groups
                .iter()
                .enumerate()
                .filter(|(k, _)| slots[*k].is_none())
                .find_map(|(k, g)| self.group_match(g.terms, c).map(|hit| (k, hit)));
            let (k, hit) = hit?;
            slots[k] = Some(hit);
        }
        if groups
            .iter()
            .enumerate()
            .any(|(k, g)| g.required && slots[k].is_none())
        {
            return None;
        }
        let filled = slots.iter().flatten().count();
        let kept = slots.iter().flatten().filter(|(_, omit)| !omit).count();
        let mut out = Vec::new();
        for (hit, omittable) in slots.iter().flatten() {
            if *omittable && kept > 0 && filled > 1 {
                continue;
            }
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend_from_slice(hit);
        }
        Some(out)
    }

    fn alternative_match(&self, alt: &[u8], tokens: &[&Component]) -> Option<Vec<u8>> {
        let alt = trim_spaces(alt);
        if alt.is_empty() || tokens.is_empty() {
            return None;
        }
        if !matches!(alt[0], b'[' | b'!') {
            let repeated = alt.last() == Some(&b'+');
            let term = if repeated { &alt[..alt.len() - 1] } else { alt };
            if !repeated && tokens.len() != 1 {
                return None;
            }
            let mut out = Vec::new();
            for c in tokens {
                let hit = self.term_match(term, c)?;
                if !out.is_empty() {
                    out.push(b' ');
                }
                out.extend(hit);
            }
            return Some(out);
        }
        let groups = parse_groups(alt)?;
        if groups.len() == 1 && groups[0].max_count > 1 {
            return self.repeat_match(&groups[0], tokens);
        }
        self.any_order_match(&groups, tokens)
    }

    fn grammar_match(&self, grammar: &[u8], tokens: &[&Component]) -> Option<Vec<u8>> {
        let mut p = grammar;
        loop {
            let end = alternative_end(p);
            if let Some(hit) = self.alternative_match(&p[..end], tokens) {
                return Some(hit);
            }
            if end >= p.len() {
                return None;
            }
            p = &p[end + 1..];
        }
    }

    fn list_match(&self, grammar: &[u8], tokens: &[&Component]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for item in tokens.split(|c| c.kind == Kind::Comma) {
            let hit = self.grammar_match(grammar, item)?;
            if !out.is_empty() {
                out.extend_from_slice(b", ");
            }
            out.extend(hit);
        }
        Some(out)
    }
}

fn term_end(p: &[u8]) -> usize {
    if p.first() == Some(&b'<') {
        return find(p, b'>').map_or(p.len(), |close| close + 1);
    }
    p.iter()
        .position(|&c| c == b' ' || c == b']')
        .unwrap_or(p.len())
}

fn group_close(p: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            b'<' => i += find(&p[i..], b'>')?,
            b']' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_groups(alt: &[u8]) -> Option<Vec<Group<'_>>> {
    let mut groups = Vec::new();
    let mut p = alt;
    loop {
        p = skip_spaces(p);
        if p.is_empty() {
            break;
        }
        let required = p[0] == b'!';
        if required {
            p = &p[1..];
        }
        if p.first() != Some(&b'[') || groups.len() == GROUPS_MAX {
            return None;
        }
        let close = group_close(&p[1..])? + 1;
        let mut group = Group {
            terms: &p[1..close],
            required,
            min_count: 0,
            max_count: 1,
        };
        p = &p[close + 1..];
        if p.first() == Some(&b'{') {
            let brace = find(p, b'}')?;
            let body = &p[1..brace];
            group.min_count = leading_int(body);
            group.max_count =
                find(body, b',').map_or(group.min_count, |comma| leading_int(&body[comma + 1..]));
            p = &p[brace + 1..];
        }
        groups.push(group);
    }
    (!groups.is_empty()).then_some(groups)
}

fn alternative_end(p: &[u8]) -> usize {
    let mut depth = 0i32;
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            b'<' => {
                if let Some(close) = find(&p[i..], b'>') {
                    i += close;
                }
            }
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'|' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    p.len()
}

fn has_keyword_bytes(grammar: &[u8], keyword: &[u8]) -> bool {
    if keyword.is_empty() {
        return false;
    }
    let n = keyword.len();
    (0..grammar.len().saturating_sub(n - 1)).any(|i| {
        &grammar[i..i + n] == keyword
            && (i == 0 || b" [|!".contains(&grammar[i - 1]))
            && grammar.get(i + n).is_none_or(|c| b" ]*|+{".contains(c))
    })
}

pub fn canonical(
    grammar: &str,
    text: &[u8],
    number: &dyn Fn(&[u8]) -> Option<f64>,
) -> Option<String> {
    let (components, valid) = southstar_css_syntax::parse(text);
    let tokens: Vec<&Component> = components
        .iter()
        .filter(|c| c.kind != Kind::Whitespace)
        .collect();
    if !valid || tokens.is_empty() {
        return None;
    }
    let grammar = grammar.as_bytes();
    let matcher = Matcher {
        grammar,
        src: text,
        number,
    };
    let hit = match grammar.strip_suffix(b" #") {
        Some(item) if !item.is_empty() => matcher.list_match(item, &tokens),
        _ => matcher.grammar_match(grammar, &tokens),
    }?;
    String::from_utf8(hit).ok()
}

pub fn has_keyword(grammar: &str, keyword: &str) -> bool {
    has_keyword_bytes(grammar.as_bytes(), keyword.as_bytes())
}
