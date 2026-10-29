//! Southstar — starting the fetches of a module's static imports as soon as its source is known, so the loader's one-at-a-time requests find them in flight.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_js_engine::Scope;
use southstar_js_runtime::api::Page as JsPage;

use crate::ffi::{self, CSP_SCRIPT, Js};
use crate::{import_map, page};

const MAX_PREFETCHES: usize = 512;

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

struct Lexer<'a> {
    src: &'a [u8],
    at: usize,
}

impl<'a> Lexer<'a> {
    fn skip_string(&mut self, quote: u8) -> Option<&'a [u8]> {
        let start = self.at + 1;
        let mut i = start;
        while i < self.src.len() {
            match self.src[i] {
                b'\\' => i += 2,
                b'\n' if quote != b'`' => break,
                b if b == quote => {
                    self.at = i + 1;
                    return Some(&self.src[start..i]);
                }
                _ => i += 1,
            }
        }
        self.at = i.min(self.src.len());
        None
    }

    fn skip_comment(&mut self) -> bool {
        match self.src.get(self.at..self.at + 2) {
            Some(b"//") => {
                self.at = self.src[self.at..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(self.src.len(), |end| self.at + end);
                true
            }
            Some(b"/*") => {
                self.at = self.src[self.at + 2..]
                    .windows(2)
                    .position(|w| w == b"*/")
                    .map_or(self.src.len(), |end| self.at + 2 + end + 2);
                true
            }
            _ => false,
        }
    }

    fn skip_blank(&mut self) {
        loop {
            while self.at < self.src.len() && self.src[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if !self.skip_comment() {
                return;
            }
        }
    }

    fn specifier_after_keyword(&mut self) -> Option<&'a [u8]> {
        self.skip_blank();
        match self.src.get(self.at) {
            Some(&quote @ (b'"' | b'\'')) => self.skip_string(quote),
            _ => None,
        }
    }
}

pub(crate) fn static_imports(src: &[u8]) -> Vec<&[u8]> {
    let mut lexer = Lexer { src, at: 0 };
    let mut found = Vec::new();
    while lexer.at < src.len() {
        let b = src[lexer.at];
        if lexer.skip_comment() {
            continue;
        }
        if matches!(b, b'"' | b'\'' | b'`') {
            lexer.skip_string(b);
            continue;
        }
        if !is_ident(b) {
            lexer.at += 1;
            continue;
        }
        let start = lexer.at;
        while lexer.at < src.len() && is_ident(src[lexer.at]) {
            lexer.at += 1;
        }
        let word = &src[start..lexer.at];
        let member = start > 0 && src[start - 1] == b'.';
        if !member
            && (word == b"from" || word == b"import")
            && let Some(specifier) = lexer.specifier_after_keyword()
            && !specifier.is_empty()
        {
            found.push(specifier);
        }
    }
    found
}

fn resolve(js: Js, base: &CStr, specifier: &[u8]) -> Option<CString> {
    if let Some(mapped) = import_map::resolve(js, specifier) {
        return CString::new(mapped).ok();
    }
    let bare = !(specifier.starts_with(b"./")
        || specifier.starts_with(b"../")
        || specifier.starts_with(b"/")
        || specifier.contains(&b':'));
    if bare {
        return None;
    }
    ffi::url_resolve(Some(base), &CString::new(specifier).ok()?)
}

fn claim(js: Js, url: &CStr) -> bool {
    page::with_page(js, |page| {
        page.prefetched.len() < MAX_PREFETCHES && page.prefetched.insert(url.to_bytes().to_vec())
    })
}

pub(crate) fn imports(scope: &mut Scope<'_>, src: &[u8], name: &[u8]) {
    let js = Js::of(JsPage::current(scope));
    if js.is_null() {
        return;
    }
    let Ok(base) = CString::new(name) else {
        return;
    };
    if !ffi::url_is_http_or_https(&base) {
        return;
    }
    let top_url = js.module_top_url();
    let secure_page = top_url
        .as_deref()
        .is_some_and(|top| top.to_bytes().starts_with(b"https://"));
    for specifier in static_imports(src) {
        let Some(url) = resolve(js, &base, specifier) else {
            continue;
        };
        if !ffi::url_is_http_or_https(&url)
            || (secure_page && url.to_bytes().starts_with(b"http://"))
            || !js.csp_allows(CSP_SCRIPT, &url, top_url.as_deref(), None, false)
            || !claim(js, &url)
        {
            continue;
        }
        js.prefetch_script(&url, top_url.as_deref());
    }
}
