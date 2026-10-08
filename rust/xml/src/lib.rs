//! Southstar — a minimal namespaced XML and XHTML parser that builds the engine's DOM through a small tree-building interface.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

const NS_XML: &[u8] = b"http://www.w3.org/XML/1998/namespace";
const NS_XMLNS: &[u8] = b"http://www.w3.org/2000/xmlns/";
const NS_XHTML: &[u8] = b"http://www.w3.org/1999/xhtml";
const NS_SVG: &[u8] = b"http://www.w3.org/2000/svg";
const MAX_DEPTH: i32 = 256;
const REPLACEMENT: &[u8] = "\u{fffd}".as_bytes();

pub const SVG_NS: u32 = 1 << 7;
pub const FOREIGN_NS: u32 = 1 << 9;
pub const CDATA: u32 = 1 << 10;
pub const PROCESSING_INSTRUCTION: u32 = 1 << 11;
pub const KEEP_CASE: u32 = 1 << 12;
pub const NOT_PARSER_INSERTED: u32 = 1 << 13;

pub trait Dom {
    type Node: Copy;
    fn document(&mut self) -> Self::Node;
    fn element(&mut self, name: Option<&[u8]>) -> Self::Node;
    fn text(&mut self, text: &[u8]) -> Self::Node;
    fn comment(&mut self, text: &[u8]) -> Self::Node;
    fn set_name(&mut self, node: Self::Node, name: &[u8]);
    fn add_flags(&mut self, node: Self::Node, flags: u32);
    fn mark_doctype(&mut self, node: Self::Node);
    fn set_attr(&mut self, element: Self::Node, name: &[u8], value: &[u8]);
    fn set_attr_ns(
        &mut self,
        element: Self::Node,
        namespace: Option<&[u8]>,
        prefix: Option<&[u8]>,
        local: &[u8],
        qualified: &[u8],
        value: &[u8],
    );
    fn append(&mut self, parent: Self::Node, child: Self::Node);
    fn free(&mut self, node: Self::Node);
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_name_char(c: u8, first: bool) -> bool {
    if c == b':' || c == b'_' || c.is_ascii_alphabetic() || c >= 0x80 {
        return true;
    }
    !first && (c == b'-' || c == b'.' || c.is_ascii_digit())
}

fn ascii_strtoull(text: &[u8], base: u64) -> u64 {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    if i >= text.len() {
        return 0;
    }
    let mut negative = false;
    if text[i] == b'-' {
        negative = true;
        i += 1;
    } else if text[i] == b'+' {
        i += 1;
    }
    if base == 16
        && text.get(i) == Some(&b'0')
        && text.get(i + 1).map(u8::to_ascii_uppercase) == Some(b'X')
    {
        i += 2;
    }
    let start = i;
    let (cutoff, cutlim) = (u64::MAX / base, u64::MAX % base);
    let (mut value, mut overflow) = (0u64, false);
    while let Some(&c) = text.get(i) {
        let digit = if c.is_ascii_digit() {
            u64::from(c - b'0')
        } else if c.is_ascii_alphabetic() {
            u64::from(c.to_ascii_uppercase() - b'A') + 10
        } else {
            break;
        };
        if digit >= base {
            break;
        }
        if value > cutoff || (value == cutoff && digit > cutlim) {
            overflow = true;
        } else {
            value = value * base + digit;
        }
        i += 1;
    }
    if i == start {
        0
    } else if overflow {
        u64::MAX
    } else if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

fn append_codepoint(out: &mut Vec<u8>, codepoint: u64) {
    let valid = u32::try_from(codepoint)
        .ok()
        .filter(|&c| c != 0)
        .and_then(char::from_u32);
    match valid {
        Some(c) => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        None => out.extend_from_slice(REPLACEMENT),
    }
}

fn decode_text(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] != b'&' {
            out.push(text[i]);
            i += 1;
            continue;
        }
        let Some(semi) = text[i..].iter().position(|&c| c == b';').map(|at| i + at) else {
            out.push(b'&');
            i += 1;
            continue;
        };
        let entity = &text[i + 1..semi];
        if entity.is_empty() {
            out.push(b'&');
            i += 1;
            continue;
        }
        if entity[0] == b'#' {
            let codepoint = if entity.len() >= 2 && (entity[1] == b'x' || entity[1] == b'X') {
                ascii_strtoull(&entity[2..], 16)
            } else {
                ascii_strtoull(&entity[1..], 10)
            };
            append_codepoint(&mut out, codepoint);
            i = semi + 1;
            continue;
        }
        let replacement: &[u8] = match entity {
            b"amp" => b"&",
            b"lt" => b"<",
            b"gt" => b">",
            b"quot" => b"\"",
            b"apos" => b"'",
            b"nbsp" => "\u{a0}".as_bytes(),
            _ => {
                out.push(b'&');
                i += 1;
                continue;
            }
        };
        out.extend_from_slice(replacement);
        i = semi + 1;
    }
    out
}

type Attribute = (Vec<u8>, Vec<u8>);

struct Binding {
    prefix: Option<Vec<u8>>,
    uri: Vec<u8>,
}

struct Parser<'a, D: Dom> {
    input: &'a [u8],
    p: usize,
    bindings: Vec<Binding>,
    ok: bool,
    dom: &'a mut D,
}

impl<D: Dom> Parser<'_, D> {
    fn at(&self, offset: usize) -> u8 {
        self.input.get(self.p + offset).copied().unwrap_or(0)
    }

    fn remaining(&self) -> usize {
        self.input.len() - self.p
    }

    fn skip_space(&mut self) {
        while self.p < self.input.len() && is_space(self.input[self.p]) {
            self.p += 1;
        }
    }

    fn read_name(&mut self) -> Option<Vec<u8>> {
        let start = self.p;
        if self.p >= self.input.len() || !is_name_char(self.input[self.p], true) {
            return None;
        }
        self.p += 1;
        while self.p < self.input.len() && is_name_char(self.input[self.p], false) {
            self.p += 1;
        }
        Some(self.input[start..self.p].to_vec())
    }

    fn lookup(&self, prefix: Option<&[u8]>) -> Option<Vec<u8>> {
        match prefix {
            Some(b"xml") => return Some(NS_XML.to_vec()),
            Some(b"xmlns") => return Some(NS_XMLNS.to_vec()),
            _ => {}
        }
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.prefix.as_deref() == prefix)
            .map(|binding| binding.uri.clone())
    }

    fn bind(&mut self, prefix: &[u8], uri: Vec<u8>) {
        self.bindings.push(Binding {
            prefix: (!prefix.is_empty()).then(|| prefix.to_vec()),
            uri,
        });
    }

    fn processing_instruction(&mut self, end: usize) -> D::Node {
        let target_start = self.p + 2;
        let target_end = (target_start..end)
            .find(|&at| is_space(self.input[at]))
            .unwrap_or(end);
        let data = (target_end..end)
            .find(|&at| !is_space(self.input[at]))
            .unwrap_or(end);
        let node = self.dom.comment(&self.input[data..end]);
        self.dom
            .set_name(node, &self.input[target_start..target_end]);
        self.dom.add_flags(node, PROCESSING_INSTRUCTION);
        node
    }

    fn pi_end(&self) -> Option<usize> {
        let mut e = self.p + 2;
        while e + 1 < self.input.len() && !(self.input[e] == b'?' && self.input[e + 1] == b'>') {
            e += 1;
        }
        (e + 1 < self.input.len()).then_some(e)
    }

    fn delimited_end(&self, from: usize, close: &[u8]) -> Option<usize> {
        let mut e = from;
        while e + 2 < self.input.len() && &self.input[e..e + 3] != close {
            e += 1;
        }
        (e + 2 < self.input.len()).then_some(e)
    }

    fn misc_and_doctype(&mut self, doc: D::Node) {
        loop {
            self.skip_space();
            if self.remaining() < 2 || self.at(0) != b'<' {
                return;
            }
            if self.at(1) == b'?' {
                let Some(end) = self.pi_end() else {
                    self.p = self.input.len();
                    return;
                };
                let declaration = self.at(2) == b'x'
                    && self.at(3) == b'm'
                    && self.at(4) == b'l'
                    && (end == self.p + 5 || is_space(self.at(5)));
                if !declaration {
                    let pi = self.processing_instruction(end);
                    self.dom.append(doc, pi);
                }
                self.p = end + 2;
                continue;
            }
            if self.at(1) != b'!' {
                return;
            }
            if self.remaining() >= 4 && self.input[self.p..].starts_with(b"<!--") {
                let Some(end) = self.delimited_end(self.p + 4, b"-->") else {
                    self.p = self.input.len();
                    return;
                };
                let comment = self.dom.comment(&self.input[self.p + 4..end]);
                self.dom.append(doc, comment);
                self.p = end + 3;
                continue;
            }
            if self.remaining() >= 9
                && self.input[self.p..self.p + 9].eq_ignore_ascii_case(b"<!DOCTYPE")
            {
                let mut e = self.p + 9;
                let mut bracket = 0;
                while e < self.input.len() {
                    match self.input[e] {
                        b'[' => bracket += 1,
                        b']' if bracket > 0 => bracket -= 1,
                        b'>' if bracket == 0 => break,
                        _ => {}
                    }
                    e += 1;
                }
                let start = (self.p + 9..e)
                    .find(|&at| !is_space(self.input[at]))
                    .unwrap_or(e);
                let name_end = (start..e)
                    .find(|&at| {
                        let c = self.input[at];
                        is_space(c) || c == b'>' || c == b'['
                    })
                    .unwrap_or(e);
                if name_end > start {
                    let doctype = self.dom.element(None);
                    self.dom.set_name(doctype, &self.input[start..name_end]);
                    self.dom.set_attr(doctype, b"publicId", b"");
                    self.dom.set_attr(doctype, b"systemId", b"");
                    self.dom.mark_doctype(doctype);
                    self.dom.append(doc, doctype);
                }
                self.p = if e < self.input.len() {
                    e + 1
                } else {
                    self.input.len()
                };
                continue;
            }
            return;
        }
    }

    fn fail<T>(&mut self) -> Option<T> {
        self.ok = false;
        None
    }

    fn attributes(&mut self) -> Option<(Vec<Attribute>, usize)> {
        let base = self.bindings.len();
        let mut attributes = Vec::new();
        loop {
            self.skip_space();
            if self.p >= self.input.len() {
                return self.fail();
            }
            if matches!(self.at(0), b'>' | b'/') {
                return Some((attributes, base));
            }
            let Some(name) = self.read_name() else {
                return self.fail();
            };
            self.skip_space();
            if self.p >= self.input.len() || self.at(0) != b'=' {
                return self.fail();
            }
            self.p += 1;
            self.skip_space();
            if self.p >= self.input.len() || !matches!(self.at(0), b'"' | b'\'') {
                return self.fail();
            }
            let quote = self.at(0);
            self.p += 1;
            let start = self.p;
            while self.p < self.input.len() && self.input[self.p] != quote {
                self.p += 1;
            }
            if self.p >= self.input.len() {
                return self.fail();
            }
            let value = decode_text(&self.input[start..self.p]);
            self.p += 1;
            if name == b"xmlns" {
                self.bind(b"", value.clone());
            } else if let Some(prefix) = name.strip_prefix(b"xmlns:") {
                self.bind(prefix, value.clone());
            }
            attributes.push((name, value));
        }
    }

    fn apply_namespace(&mut self, element: D::Node, qualified: &[u8], uri: Option<&[u8]>) {
        let colon = qualified.iter().position(|&c| c == b':');
        self.dom.add_flags(element, KEEP_CASE);
        if uri == Some(NS_XHTML) {
            if let Some(colon) = colon {
                self.dom
                    .set_attr(element, b"data-nd-ns-prefix", &qualified[..colon]);
            }
            return;
        }
        self.dom.add_flags(
            element,
            if uri == Some(NS_SVG) {
                SVG_NS
            } else {
                FOREIGN_NS
            },
        );
        if let Some(uri) = uri.filter(|uri| !uri.is_empty()) {
            self.dom.set_attr(element, b"data-nd-ns-uri", uri);
        }
    }

    fn element(&mut self, parent: D::Node, depth: i32) -> bool {
        if depth > MAX_DEPTH {
            self.ok = false;
            return false;
        }
        self.p += 1;
        let Some(qualified) = self.read_name() else {
            self.ok = false;
            return false;
        };
        let Some((attributes, base)) = self.attributes() else {
            return false;
        };
        let colon = qualified.iter().position(|&c| c == b':');
        let uri = self.lookup(colon.map(|colon| &qualified[..colon]));
        let stored = match colon {
            Some(colon) if uri.as_deref() == Some(NS_XHTML) => &qualified[colon + 1..],
            _ => &qualified[..],
        };
        let element = self.dom.element(Some(stored));
        self.dom.add_flags(element, NOT_PARSER_INSERTED);
        self.apply_namespace(element, &qualified, uri.as_deref());
        for (name, value) in &attributes {
            if name == b"xmlns" {
                self.dom
                    .set_attr_ns(element, Some(NS_XMLNS), None, b"xmlns", b"xmlns", value);
            } else if let Some(colon) = name.iter().position(|&c| c == b':') {
                let prefix = &name[..colon];
                let namespace = self.lookup(Some(prefix));
                self.dom.set_attr_ns(
                    element,
                    namespace.as_deref(),
                    Some(prefix),
                    &name[colon + 1..],
                    name,
                    value,
                );
            } else {
                self.dom.set_attr_ns(element, None, None, name, name, value);
            }
        }
        self.dom.append(parent, element);

        self.skip_space();
        if self.p < self.input.len() && self.at(0) == b'/' {
            self.p += 1;
            if self.p >= self.input.len() || self.at(0) != b'>' {
                self.ok = false;
            } else {
                self.p += 1;
            }
            self.bindings.truncate(base);
            return self.ok;
        }
        if self.p >= self.input.len() || self.at(0) != b'>' {
            self.ok = false;
            return false;
        }
        self.p += 1;
        self.content(element, &qualified, depth);
        self.bindings.truncate(base);
        self.ok
    }

    fn content(&mut self, element: D::Node, qualified: &[u8], depth: i32) {
        while self.ok && self.p < self.input.len() {
            if self.at(0) != b'<' {
                let start = self.p;
                while self.p < self.input.len() && self.input[self.p] != b'<' {
                    self.p += 1;
                }
                let text = decode_text(&self.input[start..self.p]);
                let node = self.dom.text(&text);
                self.dom.append(element, node);
                continue;
            }
            if self.remaining() >= 2 && self.at(1) == b'/' {
                self.p += 2;
                let name = self.read_name();
                self.skip_space();
                if self.p < self.input.len() && self.at(0) == b'>' {
                    self.p += 1;
                } else {
                    self.ok = false;
                }
                if name.as_deref() != Some(qualified) {
                    self.ok = false;
                }
                break;
            }
            if self.remaining() >= 4 && self.input[self.p..].starts_with(b"<!--") {
                let Some(end) = self.delimited_end(self.p + 4, b"-->") else {
                    self.ok = false;
                    break;
                };
                let comment = self.dom.comment(&self.input[self.p + 4..end]);
                self.dom.append(element, comment);
                self.p = end + 3;
                continue;
            }
            if self.remaining() >= 9 && self.input[self.p..].starts_with(b"<![CDATA[") {
                let Some(end) = self.delimited_end(self.p + 9, b"]]>") else {
                    self.ok = false;
                    break;
                };
                let text = self.dom.text(&self.input[self.p + 9..end]);
                self.dom.add_flags(text, CDATA);
                self.dom.append(element, text);
                self.p = end + 3;
                continue;
            }
            if self.remaining() >= 2 && self.at(1) == b'?' {
                let Some(end) = self.pi_end() else {
                    self.ok = false;
                    break;
                };
                let pi = self.processing_instruction(end);
                self.dom.append(element, pi);
                self.p = end + 2;
                continue;
            }
            if !self.element(element, depth + 1) {
                break;
            }
        }
    }
}

fn line_and_column(input: &[u8], offset: usize) -> (i32, i32) {
    let (mut line, mut column) = (1i32, 1i32);
    for &c in &input[..offset] {
        if c == b'\n' {
            line = line.wrapping_add(1);
            column = 1;
        } else {
            column = column.wrapping_add(1);
        }
    }
    (line, column)
}

pub fn parse<D: Dom>(dom: &mut D, input: &[u8]) -> Result<D::Node, (i32, i32)> {
    let start = if input.starts_with(b"\xEF\xBB\xBF") {
        3
    } else {
        0
    };
    let doc = dom.document();
    let mut parser = Parser {
        input,
        p: start,
        bindings: Vec::new(),
        ok: true,
        dom,
    };
    parser.misc_and_doctype(doc);
    let mut rooted = false;
    if parser.p < input.len() && parser.at(0) == b'<' {
        rooted = parser.element(doc, 0);
    } else {
        parser.ok = false;
    }
    if parser.ok {
        parser.misc_and_doctype(doc);
    }
    if !parser.ok || !rooted {
        let position = line_and_column(input, parser.p);
        parser.dom.free(doc);
        return Err(position);
    }
    Ok(doc)
}
