//! Southstar — view-source: documents, the fetched markup shown with its tags, attributes, values, comments and doctypes highlighted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_html_util::{escape_append, escape_text};

use crate::about;

pub const PREFIX: &[u8] = b"view-source:";

const HEAD: &str = concat!(
    "</title><style>",
    "body{background:#fff;color:#000;margin:0}",
    "pre{margin:0;padding:8px;font-family:monospace;font-size:13px;",
    "line-height:1.3;tab-size:4}",
    ".vs-tag{color:#800080;font-weight:bold}",
    ".vs-attr{color:#000000;font-weight:bold}",
    ".vs-val{color:#0000c0}",
    ".vs-comment{color:#008000;font-style:italic}",
    ".vs-doctype{color:#708090}",
    "</style></head><body><pre>"
);

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn span(out: &mut Vec<u8>, class: &[u8], text: &[u8]) {
    if text.is_empty() {
        return;
    }
    out.extend_from_slice(b"<span class=\"");
    out.extend_from_slice(class);
    out.extend_from_slice(b"\">");
    escape_append(out, text, false);
    out.extend_from_slice(b"</span>");
}

fn plain(out: &mut Vec<u8>, text: &[u8]) {
    escape_append(out, text, false);
}

fn tag(out: &mut Vec<u8>, s: &[u8], p: usize) -> usize {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut q = p + 1;
    if at(q) == b'/' {
        q += 1;
    }
    while at(q).is_ascii_alphanumeric() || at(q) == b'-' || at(q) == b':' {
        q += 1;
    }
    span(out, b"vs-tag", &s[p..q]);
    while at(q) != 0 && at(q) != b'>' {
        let c = at(q);
        if is_space(c) || c == b'/' || c == b'=' {
            let start = q;
            while is_space(at(q)) || at(q) == b'/' || at(q) == b'=' {
                q += 1;
            }
            plain(out, &s[start..q]);
        } else if c == b'"' || c == b'\'' {
            let start = q;
            q += 1;
            while at(q) != 0 && at(q) != c {
                q += 1;
            }
            if at(q) == c {
                q += 1;
            }
            span(out, b"vs-val", &s[start..q]);
        } else {
            let start = q;
            while at(q) != 0 && at(q) != b'>' && at(q) != b'=' && at(q) != b'/' && !is_space(at(q))
            {
                q += 1;
            }
            let is_value = s[p + 1..start].iter().rev().find(|&&b| !is_space(b)) == Some(&b'=');
            span(
                out,
                if is_value { b"vs-val" } else { b"vs-attr" },
                &s[start..q],
            );
        }
    }
    if at(q) == b'>' {
        span(out, b"vs-tag", b">");
        q += 1;
    }
    q
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

pub fn document(inner_url: &[u8], text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"<!doctype html><html><head><meta charset=\"utf-8\"><title>Source of: ");
    out.extend_from_slice(&escape_text(inner_url));
    out.extend_from_slice(HEAD.as_bytes());
    let at = |i: usize| text.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while p < text.len() {
        if text[p..].starts_with(b"<!--") {
            let end = find(text, p + 4, b"-->").map_or(text.len(), |e| e + 3);
            span(&mut out, b"vs-comment", &text[p..end]);
            p = end;
        } else if at(p) == b'<' && (at(p + 1) == b'!' || at(p + 1) == b'?') {
            let end = find(text, p, b">").map_or(text.len(), |e| e + 1);
            span(&mut out, b"vs-doctype", &text[p..end]);
            p = end;
        } else if at(p) == b'<'
            && (at(p + 1).is_ascii_alphabetic()
                || (at(p + 1) == b'/' && at(p + 2).is_ascii_alphabetic()))
        {
            p = tag(&mut out, text, p);
        } else {
            let next = find(text, p + 1, b"<").unwrap_or(text.len());
            plain(&mut out, &text[p..next]);
            p = next;
        }
    }
    out.extend_from_slice(b"</pre></body></html>");
    out
}

pub fn error_document(inner_url: &[u8], message: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        b"<!doctype html><html><head><meta charset=\"utf-8\"><title>Source unavailable</title></head><body><p>",
    );
    out.extend_from_slice(b"Could not load the source of ");
    out.extend_from_slice(&escape_text(inner_url));
    out.extend_from_slice(b": ");
    out.extend_from_slice(&escape_text(message));
    out.extend_from_slice(b"</p></body></html>");
    out
}

pub fn allowed(top_url: Option<&[u8]>, inner_url: &[u8]) -> bool {
    let from_chrome =
        about::request_from_chrome(top_url) || top_url.is_some_and(|t| t.starts_with(PREFIX));
    let inner_allowed = [&b"http:"[..], b"https:", b"file:", b"about:", b"data:"]
        .iter()
        .any(|scheme| inner_url.starts_with(scheme));
    from_chrome && inner_allowed
}
