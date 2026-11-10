//! Southstar — the checks WebGL makes on shader source before a GPU driver's compiler sees it: character set, token length, reserved names, nesting, the version directive and macro expansion size.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

const MAX_IDENTIFIER_V1: usize = 256;
const MAX_IDENTIFIER_V2: usize = 1024;
const MAX_NESTING: usize = 256;
const MAX_EXPANSION: u64 = 1 << 20;
const MAX_MACROS: usize = 4096;
const MACRO_DEPTH: u32 = 64;
const PUNCTUATION: &[u8] = b".+-/*%<>[](){}^|&~=!:;,?#\\";

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c | b'\r' | b'\n')
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn without_comments(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        match (source[i], source.get(i + 1)) {
            (b'/', Some(b'/')) => {
                i += 2;
                while i < source.len() && source[i] != b'\n' {
                    let continued = source[i] == b'\\'
                        && matches!(source.get(i + 1), Some(b'\n') | Some(b'\r'));
                    i += if continued { 2 } else { 1 };
                }
            }
            (b'/', Some(b'*')) => {
                i += 2;
                while i < source.len() && !(source[i] == b'*' && source.get(i + 1) == Some(&b'/')) {
                    if source[i] == b'\n' {
                        out.push(b'\n');
                    }
                    i += 1;
                }
                i = (i + 2).min(source.len());
                out.push(b' ');
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

fn joined_lines(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if source[i] == b'\\' && source.get(i + 1) == Some(&b'\n') {
            i += 2;
        } else if source[i] == b'\\' && source[i + 1..].starts_with(b"\r\n") {
            i += 3;
        } else {
            out.push(source[i]);
            i += 1;
        }
    }
    out
}

fn words(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    text.split(|&byte| !is_word(byte))
        .filter(|word| !word.is_empty())
}

fn identifiers(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    words(text).filter(|word| !word[0].is_ascii_digit())
}

fn character_error(code: &[u8]) -> Option<&'static str> {
    code.iter()
        .any(|&byte| !(is_word(byte) || is_space(byte) || PUNCTUATION.contains(&byte)))
        .then_some("a character outside the GLSL ES character set")
}

fn identifier_error(code: &[u8], version: i32) -> Option<&'static str> {
    let longest = if version >= 2 {
        MAX_IDENTIFIER_V2
    } else {
        MAX_IDENTIFIER_V1
    };
    for word in words(code) {
        if word.len() > longest {
            return Some("a token longer than WebGL allows");
        }
        if word.starts_with(b"webgl_") || word.starts_with(b"_webgl_") {
            return Some("an identifier with a prefix reserved by WebGL");
        }
    }
    None
}

fn nesting_error(code: &[u8]) -> Option<&'static str> {
    let mut depth = 0usize;
    for &byte in code {
        match byte {
            b'(' | b'[' | b'{' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Some("nesting deeper than WebGL allows");
                }
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn directive(line: &[u8]) -> Option<&[u8]> {
    let line = line.trim_ascii_start();
    line.strip_prefix(b"#").map(<[u8]>::trim_ascii)
}

fn version_error(code: &[u8], version: i32) -> Option<&'static str> {
    for line in code.split(|&byte| byte == b'\n') {
        let Some(rest) = directive(line).and_then(|d| d.strip_prefix(b"version")) else {
            continue;
        };
        let fields: Vec<&[u8]> = words(rest).collect();
        let known = match fields.as_slice() {
            [b"100"] => true,
            [b"300", b"es"] => version >= 2,
            _ => false,
        };
        if !known {
            return Some("a #version other than 100 or 300 es");
        }
    }
    None
}

struct Macros<'a> {
    bodies: HashMap<&'a [u8], &'a [u8]>,
    sizes: HashMap<&'a [u8], u64>,
}

impl<'a> Macros<'a> {
    fn collect(code: &'a [u8]) -> Option<Macros<'a>> {
        let mut bodies = HashMap::new();
        for line in code.split(|&byte| byte == b'\n') {
            let Some(rest) = directive(line).and_then(|d| d.strip_prefix(b"define")) else {
                continue;
            };
            let rest = rest.trim_ascii_start();
            let end = rest
                .iter()
                .position(|&byte| !is_word(byte))
                .unwrap_or(rest.len());
            let (name, mut body) = rest.split_at(end);
            if body.first() == Some(&b'(') {
                let close = body.iter().position(|&byte| byte == b')')?;
                body = &body[close + 1..];
            }
            if !name.is_empty() {
                bodies.insert(name, body);
            }
            if bodies.len() > MAX_MACROS {
                return None;
            }
        }
        Some(Macros {
            bodies,
            sizes: HashMap::new(),
        })
    }

    fn size(&mut self, name: &'a [u8], depth: u32) -> u64 {
        if let Some(&known) = self.sizes.get(name) {
            return known;
        }
        let Some(&body) = self.bodies.get(name) else {
            return 1;
        };
        if depth > MACRO_DEPTH {
            return MAX_EXPANSION + 1;
        }
        self.sizes.insert(name, 1);
        let mut total = 1u64;
        for word in identifiers(body) {
            total = total.saturating_add(self.size(word, depth + 1));
            if total > MAX_EXPANSION {
                break;
            }
        }
        self.sizes.insert(name, total);
        total
    }
}

fn macro_error(code: &[u8]) -> Option<&'static str> {
    let Some(mut macros) = Macros::collect(code) else {
        return Some("more macros than WebGL allows");
    };
    let names: Vec<&[u8]> = macros.bodies.keys().copied().collect();
    names
        .into_iter()
        .any(|name| macros.size(name, 0) > MAX_EXPANSION)
        .then_some("a macro that expands beyond what WebGL allows")
}

pub(crate) fn violation(source: &[u8], version: i32) -> Option<&'static str> {
    let end = source
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(source.len());
    let code = joined_lines(&without_comments(&source[..end]));
    character_error(&code)
        .or_else(|| identifier_error(&code, version))
        .or_else(|| nesting_error(&code))
        .or_else(|| version_error(&code, version))
        .or_else(|| macro_error(&code))
}

pub(crate) fn rejected_source(reason: &str) -> Vec<u8> {
    format!("#error WebGL: the shader source has {reason}\n").into_bytes()
}
