//! Southstar — the top-level var and function names of a frame's classic scripts, scanned from their source and re-exposed as window properties when the scripts run wrapped in the page realm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

const BLOCKED: &[&[u8]] = &[
    b"window",
    b"self",
    b"globalThis",
    b"top",
    b"parent",
    b"document",
    b"location",
    b"history",
    b"arguments",
    b"eval",
    b"undefined",
];

const MAX_NAMES: usize = 512;
const MAX_NAME_LEN: usize = 128;
const WEBPACK_PREFIX: usize = 4096;

#[derive(Default)]
pub(crate) struct ExposedNames {
    names: Vec<Vec<u8>>,
    seen: HashSet<Vec<u8>>,
}

fn ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn is_space(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 0x0b
}

fn skip_space(src: &[u8], mut p: usize) -> usize {
    while p < src.len() && is_space(src[p]) {
        p += 1;
    }
    p
}

fn skip_quoted(src: &[u8], mut p: usize, quote: u8) -> usize {
    let end = src.len();
    p += 1;
    while p < end {
        if src[p] == b'\\' {
            p += if p + 1 < end { 2 } else { 1 };
            continue;
        }
        if src[p] == quote {
            return p + 1;
        }
        p += 1;
    }
    end
}

fn skip_line_comment(src: &[u8], mut p: usize) -> usize {
    p += 2;
    while p < src.len() && src[p] != b'\n' && src[p] != b'\r' {
        p += 1;
    }
    p
}

fn skip_block_comment(src: &[u8], mut p: usize) -> usize {
    p += 2;
    while p + 1 < src.len() {
        if src[p] == b'*' && src[p + 1] == b'/' {
            return p + 2;
        }
        p += 1;
    }
    src.len()
}

fn skip_literal_or_comment(src: &[u8], p: usize) -> Option<usize> {
    let byte = src[p];
    if byte == b'\'' || byte == b'"' || byte == b'`' {
        return Some(skip_quoted(src, p, byte));
    }
    if byte == b'/' && p + 1 < src.len() {
        if src[p + 1] == b'/' {
            return Some(skip_line_comment(src, p));
        }
        if src[p + 1] == b'*' {
            return Some(skip_block_comment(src, p));
        }
    }
    None
}

fn word_at(src: &[u8], p: usize, word: &[u8]) -> bool {
    let len = word.len();
    if src.len() - p < len || &src[p..p + len] != word {
        return false;
    }
    if p > 0 && ident_char(src[p - 1]) {
        return false;
    }
    !(p + len < src.len() && ident_char(src[p + len]))
}

fn opens(byte: u8) -> bool {
    byte == b'(' || byte == b'[' || byte == b'{'
}

fn closes(byte: u8) -> bool {
    byte == b')' || byte == b']' || byte == b'}'
}

impl ExposedNames {
    fn add(&mut self, name: &[u8]) {
        if name.is_empty() || name.len() > MAX_NAME_LEN || !ident_start(name[0]) {
            return;
        }
        if !name[1..].iter().all(|&byte| ident_char(byte)) {
            return;
        }
        if BLOCKED.contains(&name) || self.seen.contains(name) {
            return;
        }
        self.seen.insert(name.to_vec());
        self.names.push(name.to_vec());
    }

    fn collect_var_names(&mut self, src: &[u8], mut p: usize) -> usize {
        let end = src.len();
        let mut depth = 0u32;
        while p < end {
            p = skip_space(src, p);
            if p >= end {
                return p;
            }
            if src[p] == b';' {
                return p + 1;
            }
            if let Some(next) = skip_literal_or_comment(src, p) {
                p = next;
                continue;
            }
            if depth == 0 && ident_start(src[p]) {
                let start = p;
                p += 1;
                while p < end && ident_char(src[p]) {
                    p += 1;
                }
                self.add(&src[start..p]);
                while p < end {
                    if let Some(next) = skip_literal_or_comment(src, p) {
                        p = next;
                        continue;
                    }
                    let byte = src[p];
                    if opens(byte) {
                        depth += 1;
                    } else if closes(byte) && depth > 0 {
                        depth -= 1;
                    } else if depth == 0 && (byte == b',' || byte == b';') {
                        if byte == b',' {
                            p += 1;
                        }
                        break;
                    }
                    p += 1;
                }
                continue;
            }
            if opens(src[p]) {
                depth += 1;
            } else if closes(src[p]) && depth > 0 {
                depth -= 1;
            }
            p += 1;
        }
        p
    }

    fn collect(&mut self, src: &[u8]) {
        let end = src.len();
        let mut p = 0;
        while p < end && self.names.len() < MAX_NAMES {
            if let Some(next) = skip_literal_or_comment(src, p) {
                p = next;
                continue;
            }
            if word_at(src, p, b"function") {
                let mut q = skip_space(src, p + 8);
                if q < end && src[q] == b'*' {
                    q = skip_space(src, q + 1);
                }
                if q < end && ident_start(src[q]) {
                    let start = q;
                    q += 1;
                    while q < end && ident_char(src[q]) {
                        q += 1;
                    }
                    self.add(&src[start..q]);
                }
                p += 8;
                continue;
            }
            if word_at(src, p, b"var") {
                p = self.collect_var_names(src, p + 3);
                continue;
            }
            p += 1;
        }
    }

    pub fn scan(&mut self, src: &[u8]) {
        if !is_webpack_chunk(src) {
            self.collect(src);
        }
    }

    pub fn exposing_script(&self) -> Option<String> {
        if self.names.is_empty() {
            return None;
        }
        let mut script = String::from("\n;(function(){var n=[");
        for (index, name) in self.names.iter().enumerate() {
            if index > 0 {
                script.push(',');
            }
            script.push('"');
            script.push_str(&String::from_utf8_lossy(name));
            script.push('"');
        }
        script.push_str(
            "];for(var i=0;i<n.length;i++){(function(k){try{if(k in window)return;\
Object.defineProperty(window,k,{configurable:true,get:function(){try{return eval(k);}catch(e){return void 0;}},\
set:function(v){try{eval(k+'=v');}catch(e){try{Object.defineProperty(window,k,{configurable:true,writable:true,value:v});}catch(_){}}}});\
}catch(e){}})(n[i]);}})();\n",
        );
        Some(script)
    }
}

fn is_webpack_chunk(src: &[u8]) -> bool {
    if src.is_empty() {
        return false;
    }
    let prefix = &src[..src.len().min(WEBPACK_PREFIX)];
    let start = skip_space(prefix, 0);
    if start >= prefix.len() {
        return false;
    }
    let rest = &prefix[start..];
    let rest = &rest[..rest.iter().position(|&b| b == 0).unwrap_or(rest.len())];
    contains(rest, b"webpackChunk") || contains(rest, b"webpackJsonp")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
