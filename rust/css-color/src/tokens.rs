//! Southstar — a cursor over CSS component values with the identifier helpers the colour parser needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css_syntax::{Component, Kind};

#[derive(Clone, Copy)]
pub(crate) struct Tokens<'a> {
    pub input: &'a [u8],
    pub items: &'a [Component],
    pub pos: usize,
}

impl<'a> Tokens<'a> {
    pub fn new(input: &'a [u8], items: &'a [Component]) -> Tokens<'a> {
        Tokens {
            input,
            items,
            pos: 0,
        }
    }

    pub fn children(&self, c: &'a Component) -> Tokens<'a> {
        Tokens::new(self.input, c.children.as_deref().unwrap_or(&[]))
    }

    pub fn skip_ws(&mut self) -> bool {
        let start = self.pos;
        while self
            .items
            .get(self.pos)
            .is_some_and(|c| c.kind == Kind::Whitespace)
        {
            self.pos += 1;
        }
        self.pos != start
    }

    pub fn peek(&mut self) -> Option<&'a Component> {
        self.skip_ws();
        self.items.get(self.pos)
    }

    pub fn next(&mut self) -> Option<&'a Component> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    pub fn at_end(&mut self) -> bool {
        self.peek().is_none()
    }

    pub fn eat_delim(&mut self, d: u8) -> bool {
        match self.peek() {
            Some(c) if c.kind == Kind::Delim && c.delimiter == d => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    pub fn has_comma(&self) -> bool {
        self.items.iter().any(|c| c.kind == Kind::Comma)
    }

    pub fn eat_ident(&mut self, name: &str) -> bool {
        match self.peek() {
            Some(c) if c.kind == Kind::Ident && self.name(c) == name => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    pub fn name(&self, c: &Component) -> String {
        c.value
            .clone()
            .map(|r| unescape(&self.input[r]))
            .unwrap_or_default()
    }

    pub fn unit(&self, c: &Component) -> String {
        self.name(c)
    }

    pub fn split_commas(&self) -> Vec<Tokens<'a>> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, c) in self.items.iter().enumerate() {
            if c.kind == Kind::Comma {
                out.push(Tokens::new(self.input, &self.items[start..i]));
                start = i + 1;
            }
        }
        out.push(Tokens::new(self.input, &self.items[start..]));
        out
    }
}

fn unescape(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\\' && i + 1 < raw.len() {
            let digits = raw[i + 1..]
                .iter()
                .take(6)
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if digits > 0 {
                let hex = core::str::from_utf8(&raw[i + 1..i + 1 + digits]).unwrap_or("0");
                let code = u32::from_str_radix(hex, 16).unwrap_or(0xfffd);
                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                i += 1 + digits;
                if raw.get(i).is_some_and(u8::is_ascii_whitespace) {
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
        let len = utf8_len(raw[i]);
        out.push_str(&String::from_utf8_lossy(&raw[i..(i + len).min(raw.len())]));
        i += len;
    }
    out.to_ascii_lowercase()
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xf0.. => 4,
        0xe0.. => 3,
        0xc0.. => 2,
        _ => 1,
    }
}
