//! Southstar — the CSS Syntax tokenizer and component-value parser.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ops::Range;

mod ffi;

const MAX_NESTING: u32 = 128;
const MAX_BRACKET_DEPTH: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Whitespace,
    Ident,
    Function,
    String,
    Number,
    Percentage,
    Dimension,
    Hash,
    AtKeyword,
    Colon,
    Semicolon,
    Comma,
    Delim,
    Block,
}

#[derive(Clone, Debug)]
pub struct Component {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub value: Option<Range<usize>>,
    pub number: f64,
    pub children: Option<Vec<Component>>,
    pub delimiter: u8,
}

impl Component {
    fn new(kind: Kind, start: usize, end: usize) -> Self {
        Component {
            kind,
            start,
            end,
            value: None,
            number: 0.0,
            children: None,
            delimiter: 0,
        }
    }
}

fn is_newline(c: u8) -> bool {
    matches!(c, b'\n' | b'\r' | 0x0c)
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn is_name(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-'
}

fn is_digit_at(input: &[u8], offset: usize) -> bool {
    input.get(offset).is_some_and(u8::is_ascii_digit)
}

fn digits_end(input: &[u8], offset: usize) -> usize {
    offset
        + input[offset..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
}

fn closer(c: u8) -> Option<u8> {
    match c {
        b'(' => Some(b')'),
        b'[' => Some(b']'),
        b'{' => Some(b'}'),
        _ => None,
    }
}

fn comment_close(input: &[u8], offset: usize) -> Option<usize> {
    input[offset..]
        .windows(2)
        .position(|pair| pair == b"*/")
        .map(|at| offset + at + 2)
}

fn escape_end(input: &[u8], offset: usize) -> usize {
    if input.get(offset) != Some(&b'\\') {
        return offset;
    }
    let offset = offset + 1;
    match input.get(offset) {
        None => offset,
        Some(c) if c.is_ascii_hexdigit() => {
            let digits = input[offset..]
                .iter()
                .take(6)
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            let after = offset + digits;
            if input.get(after).is_some_and(u8::is_ascii_whitespace) {
                after + 1
            } else {
                after
            }
        }
        Some(&c) if is_newline(c) => offset - 1,
        Some(_) => offset + 1,
    }
}

fn name_end(input: &[u8], mut offset: usize) -> usize {
    while let Some(&c) = input.get(offset) {
        if is_name(c) {
            offset += 1;
        } else if c == b'\\' {
            let next = escape_end(input, offset);
            if next == offset {
                break;
            }
            offset = next;
        } else {
            break;
        }
    }
    offset
}

fn valid_escape(input: &[u8], offset: usize) -> bool {
    offset + 1 < input.len() && input[offset] == b'\\' && !is_newline(input[offset + 1])
}

fn starts_ident(input: &[u8], offset: usize) -> bool {
    let Some(&a) = input.get(offset) else {
        return false;
    };
    if is_name_start(a) {
        return true;
    }
    if a == b'\\' {
        return valid_escape(input, offset);
    }
    if a != b'-' {
        return false;
    }
    let Some(&b) = input.get(offset + 1) else {
        return false;
    };
    is_name_start(b) || b == b'-' || valid_escape(input, offset + 1)
}

fn starts_number(input: &[u8], offset: usize) -> bool {
    match input.get(offset) {
        Some(c) if c.is_ascii_digit() => true,
        Some(b'.') => is_digit_at(input, offset + 1),
        Some(b'+' | b'-') => {
            is_digit_at(input, offset + 1)
                || (input.get(offset + 1) == Some(&b'.') && is_digit_at(input, offset + 2))
        }
        _ => false,
    }
}

fn number_end(input: &[u8], mut offset: usize) -> usize {
    if matches!(input.get(offset), Some(b'+' | b'-')) {
        offset += 1;
    }
    offset = digits_end(input, offset);
    if input.get(offset) == Some(&b'.') && is_digit_at(input, offset + 1) {
        offset = digits_end(input, offset + 1);
    }
    if matches!(input.get(offset), Some(b'e' | b'E')) {
        let mut exponent = offset + 1;
        if matches!(input.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if is_digit_at(input, exponent) {
            offset = digits_end(input, exponent + 1);
        }
    }
    offset
}

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
    nesting: u32,
    valid: bool,
}

impl Parser<'_> {
    fn skip_comment(&mut self) {
        self.offset = comment_close(self.input, self.offset + 2).unwrap_or(self.input.len());
    }

    fn consume_string(&mut self) -> Component {
        let input = self.input;
        let start = self.offset;
        let quote = input[start];
        self.offset += 1;
        let value_start = self.offset;
        let value_end = loop {
            let Some(&c) = input.get(self.offset) else {
                break self.offset;
            };
            self.offset += 1;
            if c == quote {
                break self.offset - 1;
            }
            if is_newline(c) {
                self.valid = false;
                break self.offset - 1;
            }
            if c == b'\\' && self.offset < input.len() {
                let escape = self.offset - 1;
                let next = escape_end(input, escape);
                if next == escape {
                    self.valid = false;
                    break escape;
                }
                self.offset = next;
            }
        };
        let mut component = Component::new(Kind::String, start, self.offset);
        component.value = Some(value_start..value_end);
        component.delimiter = quote;
        component
    }

    fn consume_numeric(&mut self) -> Component {
        let input = self.input;
        let start = self.offset;
        let number_end = number_end(input, start);
        let (kind, end) = if input.get(number_end) == Some(&b'%') {
            (Kind::Percentage, number_end + 1)
        } else if starts_ident(input, number_end) {
            (Kind::Dimension, name_end(input, number_end))
        } else {
            (Kind::Number, number_end)
        };
        let mut component = Component::new(kind, start, end);
        component.number = southstar_glib::ascii_strtod(&input[start..number_end]);
        component.value = Some(number_end..end);
        self.offset = end;
        component
    }

    fn consume_ident(&mut self, kind: Kind) -> Component {
        let input = self.input;
        let start = self.offset;
        if kind != Kind::Ident {
            self.offset += 1;
        }
        let name_start = self.offset;
        self.offset = name_end(input, self.offset);
        let name = name_start..self.offset;
        if kind == Kind::Ident && input.get(self.offset) == Some(&b'(') {
            self.offset += 1;
            let mut component = Component::new(Kind::Function, start, 0);
            component.value = Some(name);
            component.children = Some(self.parse_list(b')'));
            component.delimiter = b')';
            component.end = self.offset;
            return component;
        }
        let mut component = Component::new(kind, start, self.offset);
        component.value = Some(name);
        component
    }

    fn consume(&mut self) -> Component {
        let input = self.input;
        let start = self.offset;
        let c = input[start];
        if c.is_ascii_whitespace() {
            self.offset += input[start..]
                .iter()
                .take_while(|c| c.is_ascii_whitespace())
                .count();
            return Component::new(Kind::Whitespace, start, self.offset);
        }
        if c == b'"' || c == b'\'' {
            return self.consume_string();
        }
        if starts_number(input, start) {
            return self.consume_numeric();
        }
        if c == b'@' && starts_ident(input, start + 1) {
            return self.consume_ident(Kind::AtKeyword);
        }
        if c == b'#'
            && input
                .get(start + 1)
                .is_some_and(|&next| is_name(next) || valid_escape(input, start + 1))
        {
            return self.consume_ident(Kind::Hash);
        }
        if starts_ident(input, start) {
            return self.consume_ident(Kind::Ident);
        }
        self.offset += 1;
        if let Some(closing) = closer(c) {
            let mut component = Component::new(Kind::Block, start, 0);
            component.delimiter = closing;
            component.children = Some(self.parse_list(closing));
            component.end = self.offset;
            return component;
        }
        let kind = match c {
            b':' => Kind::Colon,
            b';' => Kind::Semicolon,
            b',' => Kind::Comma,
            _ => Kind::Delim,
        };
        let mut component = Component::new(kind, start, self.offset);
        component.delimiter = c;
        component
    }

    fn parse_list(&mut self, closing: u8) -> Vec<Component> {
        let mut components = Vec::new();
        if self.nesting >= MAX_NESTING {
            self.valid = false;
            self.offset = self.input.len();
            return components;
        }
        self.nesting += 1;
        while let Some(&c) = self.input.get(self.offset) {
            if c == b'/' && self.input.get(self.offset + 1) == Some(&b'*') {
                self.skip_comment();
                continue;
            }
            if closing != 0 && c == closing {
                self.offset += 1;
                break;
            }
            if matches!(c, b')' | b']' | b'}') {
                self.valid = false;
                self.offset += 1;
                if closing != 0 {
                    break;
                }
                continue;
            }
            components.push(self.consume());
        }
        self.nesting -= 1;
        components
    }
}

pub fn parse(input: &[u8]) -> (Vec<Component>, bool) {
    let mut parser = Parser {
        input,
        offset: 0,
        nesting: 0,
        valid: true,
    };
    let components = parser.parse_list(0);
    (components, parser.valid)
}

pub fn value_valid(input: &[u8]) -> bool {
    parse(input).1
}

pub fn scan(input: &[u8], terminators: &[u8]) -> (usize, u8) {
    let end = input.len();
    let mut p = 0;
    let mut quote = 0;
    let mut stack = [0u8; MAX_BRACKET_DEPTH];
    let mut depth = 0;
    while p < end {
        let c = input[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
                continue;
            }
            if c == quote {
                quote = 0;
            }
            p += 1;
            continue;
        }
        if c == b'/' && input.get(p + 1) == Some(&b'*') {
            p = comment_close(input, p + 2).unwrap_or(end);
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
        if depth == 0 && (c == 0 || terminators.contains(&c)) {
            return (p, c);
        }
        if let Some(closing) = closer(c) {
            if depth < MAX_BRACKET_DEPTH {
                stack[depth] = closing;
                depth += 1;
            }
        } else if depth > 0 && c == stack[depth - 1] {
            depth -= 1;
        }
        p += 1;
    }
    (p, 0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Statement {
    None,
    At,
    Qualified,
}

pub fn is_self_contained(input: &[u8]) -> bool {
    let end = input.len();
    let mut p = 0;
    let mut quote = 0;
    let mut stack = [0u8; MAX_BRACKET_DEPTH];
    let mut depth = 0;
    let mut statement = Statement::None;
    while p < end {
        let c = input[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
                continue;
            }
            if c == quote || is_newline(c) {
                quote = 0;
            }
            p += 1;
            continue;
        }
        if c == b'/' && input.get(p + 1) == Some(&b'*') {
            match comment_close(input, p + 2) {
                Some(next) => p = next,
                None => return false,
            }
            continue;
        }
        if c.is_ascii_whitespace() {
            p += 1;
            continue;
        }
        if depth == 0 && statement == Statement::None {
            if input[p..].starts_with(b"<!--") {
                p += 4;
                continue;
            }
            if input[p..].starts_with(b"-->") {
                p += 3;
                continue;
            }
            statement = if c == b'@' {
                Statement::At
            } else {
                Statement::Qualified
            };
        }
        if c == b'\\' {
            if p + 1 >= end {
                return false;
            }
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
        } else if let Some(closing) = closer(c) {
            if depth == MAX_BRACKET_DEPTH {
                return false;
            }
            stack[depth] = closing;
            depth += 1;
        } else if depth > 0 && c == stack[depth - 1] {
            depth -= 1;
            if depth == 0 && c == b'}' {
                statement = Statement::None;
            }
        } else if c == b';' && depth == 0 && statement == Statement::At {
            statement = Statement::None;
        }
        p += 1;
    }
    quote == 0 && depth == 0 && statement == Statement::None
}
