//! Southstar — CSSOM serialization of CSS selector lists from their component values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use core::cell::Cell;

use southstar_css_syntax::{Component, Kind};

#[cfg(test)]
mod tests;

const MAX_DEPTH: u32 = 32;

const SELECTOR_LIST_FUNCTIONS: &[&str] = &[
    "is",
    "where",
    "not",
    "has",
    "host",
    "host-context",
    "slotted",
    "has-slotted",
    "matches",
    "-webkit-any",
    "current",
    "past",
    "future",
];

const NTH_FUNCTIONS: &[&str] = &[
    "nth-child",
    "nth-last-child",
    "nth-of-type",
    "nth-last-of-type",
];

const LEGACY_PSEUDO_ELEMENTS: &[&str] = &["before", "after", "first-line", "first-letter"];

pub fn serialize_selector_list(text: &str) -> Option<String> {
    let input = text.as_bytes();
    let (items, valid) = southstar_css_syntax::parse(input);
    if !valid {
        return None;
    }
    Writer {
        input,
        invalid: Cell::new(false),
    }
    .list(&items, Scope::default())
}

struct Writer<'a> {
    input: &'a [u8],
    invalid: Cell<bool>,
}

#[derive(Clone, Copy, Default)]
struct Scope {
    depth: u32,
    in_has: bool,
    forgiving: bool,
    after_pseudo: bool,
    compound_only: bool,
}

impl Scope {
    fn nested(self, in_has: bool, forgiving: bool) -> Scope {
        Scope {
            depth: self.depth + 1,
            in_has,
            forgiving,
            after_pseudo: false,
            compound_only: false,
        }
    }
}

fn is_ws(c: &Component) -> bool {
    c.kind == Kind::Whitespace
}

fn trim(items: &[Component]) -> &[Component] {
    let start = items.iter().position(|c| !is_ws(c)).unwrap_or(items.len());
    let end = items
        .iter()
        .rposition(|c| !is_ws(c))
        .map_or(start, |e| e + 1);
    &items[start..end]
}

fn split_commas(items: &[Component]) -> Vec<&[Component]> {
    items.split(|c| c.kind == Kind::Comma).map(trim).collect()
}

fn delim(c: &Component) -> Option<u8> {
    (c.kind == Kind::Delim).then_some(c.delimiter)
}

impl Writer<'_> {
    fn raw(&self, c: &Component) -> &str {
        core::str::from_utf8(&self.input[c.start..c.end]).unwrap_or("")
    }

    fn name(&self, c: &Component) -> String {
        c.value
            .clone()
            .map(|r| String::from_utf8_lossy(&self.input[r]).into_owned())
            .unwrap_or_default()
    }

    fn raw_span(&self, items: &[Component]) -> String {
        match (items.first(), items.last()) {
            (Some(first), Some(last)) => {
                String::from_utf8_lossy(&self.input[first.start..last.end]).into_owned()
            }
            _ => String::new(),
        }
    }

    fn list(&self, items: &[Component], scope: Scope) -> Option<String> {
        if scope.depth > MAX_DEPTH {
            return None;
        }
        let mut parts = Vec::new();
        for part in split_commas(items) {
            let outer = self.invalid.replace(false);
            let text = self.complex(part, scope)?;
            let invalid = self.invalid.get();
            parts.push(if invalid && scope.forgiving {
                self.raw_span(part)
            } else {
                text
            });
            self.invalid.set(outer || (invalid && !scope.forgiving));
        }
        Some(parts.join(", "))
    }

    fn complex(&self, items: &[Component], scope: Scope) -> Option<String> {
        let mut out = String::new();
        let mut combinator: Option<&str> = None;
        let mut after_pseudo_element = scope.after_pseudo;
        let mut i = 0;
        while i < items.len() {
            let c = &items[i];
            if is_ws(c) {
                combinator.get_or_insert(" ");
                i += 1;
                continue;
            }
            let explicit = match delim(c) {
                Some(b'>') => Some(" > "),
                Some(b'+') => Some(" + "),
                Some(b'~') => Some(" ~ "),
                Some(b'|') if items.get(i + 1).and_then(delim) == Some(b'|') => {
                    i += 1;
                    Some(" || ")
                }
                _ => None,
            };
            if let Some(explicit) = explicit {
                if scope.compound_only {
                    self.invalid.set(true);
                }
                combinator = Some(explicit);
                after_pseudo_element = false;
                i += 1;
                continue;
            }
            if let Some(comb) = combinator.take() {
                if scope.compound_only && !out.is_empty() {
                    self.invalid.set(true);
                }
                after_pseudo_element = false;
                if out.is_empty() {
                    out.push_str(comb.trim_start());
                } else {
                    out.push_str(comb);
                }
            }
            if after_pseudo_element && c.kind != Kind::Colon {
                self.invalid.set(true);
            }
            i = self.simple(items, i, &mut out, scope, &mut after_pseudo_element)?;
        }
        if let Some(comb) = combinator
            && comb != " "
        {
            out.push_str(comb.trim_end());
        }
        Some(out)
    }

    fn simple(
        &self,
        items: &[Component],
        i: usize,
        out: &mut String,
        scope: Scope,
        pseudo_element: &mut bool,
    ) -> Option<usize> {
        let c = &items[i];
        match c.kind {
            Kind::Colon => {
                let double = items.get(i + 1).is_some_and(|n| n.kind == Kind::Colon);
                let at = if double { i + 2 } else { i + 1 };
                let Some(next) = items.get(at) else {
                    out.push_str(if double { "::" } else { ":" });
                    return Some(at);
                };
                match next.kind {
                    Kind::Ident => {
                        let name = self.raw(next);
                        let legacy = !double
                            && LEGACY_PSEUDO_ELEMENTS.contains(&name.to_ascii_lowercase().as_str());
                        out.push_str(if double || legacy { "::" } else { ":" });
                        out.push_str(name);
                        *pseudo_element |= double || legacy;
                    }
                    Kind::Function => {
                        out.push_str(if double { "::" } else { ":" });
                        let inner = Scope {
                            after_pseudo: *pseudo_element,
                            ..scope
                        };
                        out.push_str(&self.function(next, inner)?);
                        *pseudo_element |= double;
                    }
                    _ => out.push_str(if double { "::" } else { ":" }),
                }
                Some(at + 1)
            }
            Kind::Block if c.delimiter == b']' => {
                let children = c.children.as_deref().unwrap_or(&[]);
                match self.attribute(children) {
                    Some(text) => out.push_str(&text),
                    None => out.push_str(self.raw(c)),
                }
                Some(i + 1)
            }
            _ => {
                out.push_str(self.raw(c));
                Some(i + 1)
            }
        }
    }

    fn function(&self, c: &Component, scope: Scope) -> Option<String> {
        let name = self.name(c);
        let lower = name.to_ascii_lowercase();
        let args = trim(c.children.as_deref().unwrap_or(&[]));
        if SELECTOR_LIST_FUNCTIONS.contains(&lower.as_str()) {
            let has = lower == "has";
            if has && scope.in_has {
                self.invalid.set(true);
            }
            let forgiving = lower == "is" || lower == "where";
            let compound = matches!(
                lower.as_str(),
                "host" | "host-context" | "slotted" | "has-slotted"
            );
            let nested = Scope {
                after_pseudo: scope.after_pseudo,
                compound_only: compound || (forgiving && scope.compound_only),
                ..scope.nested(scope.in_has || has, forgiving)
            };
            let inner = self.list(args, nested)?;
            return Some(format!("{lower}({inner})"));
        }
        if NTH_FUNCTIONS.contains(&lower.as_str()) {
            let of = args
                .iter()
                .position(|a| a.kind == Kind::Ident && self.name(a).eq_ignore_ascii_case("of"));
            let anb_items = trim(&args[..of.unwrap_or(args.len())]);
            let anb: String = anb_items.iter().map(|a| self.raw(a)).collect();
            let mut text = format!("{lower}({}", canonical_anb(&anb)?);
            if let Some(of) = of {
                text.push_str(" of ");
                text.push_str(
                    &self.list(trim(&args[of + 1..]), scope.nested(scope.in_has, false))?,
                );
            }
            text.push(')');
            return Some(text);
        }
        let inner: String = args.iter().map(|a| self.raw(a)).collect();
        let name = if lower == "lang" || lower == "dir" {
            lower
        } else {
            name
        };
        Some(format!("{name}({inner})"))
    }

    fn attribute(&self, items: &[Component]) -> Option<String> {
        let items = trim(items);
        let mut i = 0;
        let mut name = String::new();
        let first = items.first()?;
        let starts_ns = delim(first) == Some(b'|');
        let qualified = items.get(1).and_then(delim).is_some_and(|d| d == b'|')
            && items.get(2).is_some_and(|n| n.kind == Kind::Ident);
        if starts_ns {
            i = 1;
        } else if qualified && (first.kind == Kind::Ident || delim(first) == Some(b'*')) {
            name.push_str(self.raw(first));
            name.push('|');
            i = 2;
        }
        let attr = items.get(i).filter(|c| c.kind == Kind::Ident)?;
        name.push_str(self.raw(attr));
        i += 1;
        let rest: Vec<&Component> = items[i..].iter().filter(|c| !is_ws(c)).collect();
        if rest.is_empty() {
            return Some(format!("[{name}]"));
        }
        let (op, at) = match (
            rest.first().and_then(|c| delim(c)),
            rest.get(1).and_then(|c| delim(c)),
        ) {
            (Some(b'='), _) => ("=".to_string(), 1),
            (Some(m @ (b'~' | b'|' | b'^' | b'$' | b'*')), Some(b'=')) => {
                (format!("{}=", m as char), 2)
            }
            _ => return None,
        };
        let value = rest.get(at)?;
        let text = match value.kind {
            Kind::Ident => unescape(self.raw(value).as_bytes()),
            Kind::String => {
                let r = value.value.clone()?;
                unescape(&self.input[r])
            }
            _ => return None,
        };
        let mut out = format!("[{name}{op}{}", quote(&text));
        match rest.get(at + 1) {
            None => {}
            Some(flag) if flag.kind == Kind::Ident && rest.len() == at + 2 => {
                out.push(' ');
                out.push_str(&self.raw(flag).to_ascii_lowercase());
            }
            Some(_) => return None,
        }
        out.push(']');
        Some(out)
    }
}

fn unescape(raw: &[u8]) -> String {
    let mut out = String::new();
    let text = String::from_utf8_lossy(raw);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
            hex.push(chars.next().unwrap_or('0'));
        }
        if hex.is_empty() {
            match chars.next() {
                Some('\n') | None => {}
                Some(other) => out.push(other),
            }
            continue;
        }
        if chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            chars.next();
        }
        let code = u32::from_str_radix(&hex, 16).unwrap_or(0xfffd);
        let code = if code == 0 { 0xfffd } else { code };
        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
    }
    out
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{1}'..='\u{1f}' | '\u{7f}' => out.push_str(&format!("\\{:x} ", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn anb_digits(s: &[u8], p: &mut usize) -> Option<i64> {
    let start = *p;
    while s.get(*p).is_some_and(u8::is_ascii_digit) {
        *p += 1;
    }
    if *p == start {
        return None;
    }
    core::str::from_utf8(&s[start..*p])
        .ok()?
        .parse::<i64>()
        .ok()
}

fn skip_ws(s: &[u8], p: &mut usize) {
    while s.get(*p).is_some_and(u8::is_ascii_whitespace) {
        *p += 1;
    }
}

pub fn parse_anb(text: &str) -> Option<(i64, i64)> {
    let s = text.trim().to_ascii_lowercase().into_bytes();
    if s == b"odd" {
        return Some((2, 1));
    }
    if s == b"even" {
        return Some((2, 0));
    }
    let mut p = 0;
    let sign = match s.first() {
        Some(b'+') => {
            p = 1;
            1
        }
        Some(b'-') => {
            p = 1;
            -1
        }
        _ => 1,
    };
    let digits = anb_digits(&s, &mut p);
    if s.get(p) != Some(&b'n') {
        return (p == s.len()).then_some((0, sign * digits?));
    }
    p += 1;
    let a = sign * digits.unwrap_or(1);
    if p == s.len() {
        return Some((a, 0));
    }
    let b = if s[p] == b'-' {
        p += 1;
        skip_ws(&s, &mut p);
        -anb_digits(&s, &mut p)?
    } else {
        skip_ws(&s, &mut p);
        let negative = match s.get(p) {
            Some(b'+') => false,
            Some(b'-') => true,
            _ => return None,
        };
        p += 1;
        skip_ws(&s, &mut p);
        let b = anb_digits(&s, &mut p)?;
        if negative { -b } else { b }
    };
    (p == s.len()).then_some((a, b))
}

pub fn canonical_anb(text: &str) -> Option<String> {
    let (a, b) = parse_anb(text)?;
    Some(match (a, b) {
        (0, b) => b.to_string(),
        (a, 0) => anb_a(a),
        (a, b) if b > 0 => format!("{}+{b}", anb_a(a)),
        (a, b) => format!("{}{b}", anb_a(a)),
    })
}

fn anb_a(a: i64) -> String {
    match a {
        1 => "n".into(),
        -1 => "-n".into(),
        a => format!("{a}n"),
    }
}
