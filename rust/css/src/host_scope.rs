//! Southstar — a shadow tree's or framed document's style sheet scoped to its host: :host, :host(), :host-context() and ::slotted() rewritten to the host's scope attribute, and every other selector confined beneath it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::nesting::block_body_end;
use crate::scan::{is_ident, is_ws, scan_until, skip_to_block_end};

const MAX_AT_NESTING: i32 = 32;
const GROUP_RULES: [&[u8]; 5] = [b"@media", b"@supports", b"@container", b"@layer", b"@scope"];
const LEGACY_PSEUDO_ELEMENTS: [&[u8]; 4] = [b"before", b"after", b"first-line", b"first-letter"];

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn contains(s: &[u8], needle: &[u8]) -> bool {
    s.windows(needle.len()).any(|w| w == needle)
}

fn paren_body_end(s: &[u8], inner: usize) -> usize {
    let mut q = inner;
    let mut depth = 1;
    while q < s.len() {
        if s[q] == b'(' {
            depth += 1;
        } else if s[q] == b')' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        q += 1;
    }
    q
}

fn past_close(s: &[u8], q: usize) -> usize {
    if q < s.len() && s[q] == b')' {
        q + 1
    } else {
        q
    }
}

fn rewrite_host_selectors(css: &[u8], marker: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(css.len() + marker.len());
    let mut p = 0;
    while p < css.len() {
        let rest = &css[p..];
        if starts_ci(rest, b"::slotted(") {
            let inner = p + 10;
            let q = paren_body_end(css, inner);
            out.extend_from_slice(marker);
            out.extend_from_slice(b" > ");
            out.extend_from_slice(&css[inner..q]);
            p = past_close(css, q);
            continue;
        }
        if starts_ci(rest, b":host") {
            let after = p + 5;
            if starts_ci(&css[after..], b"-context(") {
                let q = paren_body_end(css, after + 9);
                out.extend_from_slice(marker);
                p = past_close(css, q);
                continue;
            }
            let next = css.get(after).copied().unwrap_or(0);
            if next == b'(' {
                let inner = after + 1;
                let q = paren_body_end(css, inner);
                out.extend_from_slice(marker);
                out.extend_from_slice(&css[inner..q]);
                p = past_close(css, q);
                continue;
            }
            if !is_ident(next) && next != b'-' {
                out.extend_from_slice(marker);
                p = after;
                continue;
            }
        }
        out.push(css[p]);
        p += 1;
    }
    out
}

fn first_compound_len(s: &[u8]) -> usize {
    let mut depth = 0;
    for (i, &c) in s.iter().enumerate() {
        match c {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= i32::from(depth > 0),
            _ if depth == 0 && (is_ws(c) || matches!(c, b'>' | b'+' | b'~' | b',')) => return i,
            _ => {}
        }
    }
    s.len()
}

fn word_at(s: &[u8], word: &[u8], clen: usize) -> bool {
    clen >= word.len()
        && s[..word.len()].eq_ignore_ascii_case(word)
        && (clen == word.len() || !is_ident(s[word.len()]))
}

fn first_compound_targets_root(s: &[u8], clen: usize) -> bool {
    word_at(s, b"html", clen) || word_at(s, b":root", clen)
}

fn compound_simple_len(s: &[u8], clen: usize) -> usize {
    let mut depth = 0;
    for i in 0..clen {
        match s[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= i32::from(depth > 0),
            b':' if depth == 0 => {
                if i + 1 < clen && s[i + 1] == b':' {
                    return i;
                }
                let legacy = LEGACY_PSEUDO_ELEMENTS.iter().any(|name| {
                    let end = i + 1 + name.len();
                    end <= clen
                        && s[i + 1..end].eq_ignore_ascii_case(name)
                        && (end == clen || !is_ident(s[end]))
                });
                if legacy {
                    return i;
                }
            }
            _ => {}
        }
    }
    clen
}

fn first_compound_may_be_root(s: &[u8], clen: usize) -> bool {
    clen > 0 && matches!(s[0], b'*' | b'.' | b'#' | b'[' | b':')
}

fn append_marked_compound(out: &mut Vec<u8>, s: &[u8], simple: usize, marker: &[u8]) {
    out.extend_from_slice(&s[..simple]);
    out.extend_from_slice(marker);
    out.extend_from_slice(&s[simple..]);
}

fn scope_one_selector(out: &mut Vec<u8>, sel: &[u8], marker: &[u8], frame_scope: bool) {
    let start = sel.iter().position(|&c| !is_ws(c)).unwrap_or(sel.len());
    let end = sel
        .iter()
        .rposition(|&c| !is_ws(c))
        .map_or(start, |i| i + 1);
    if end <= start {
        return;
    }
    let s = &sel[start..end];
    let clen = first_compound_len(s);
    let simple = compound_simple_len(s, clen);
    if contains(s, b":host") || contains(s, b"::slotted") {
        out.extend_from_slice(&rewrite_host_selectors(s, marker));
    } else if first_compound_targets_root(s, clen) {
        append_marked_compound(out, s, simple, marker);
    } else {
        out.extend_from_slice(marker);
        out.push(b' ');
        out.extend_from_slice(s);
        if frame_scope && first_compound_may_be_root(s, clen) {
            out.extend_from_slice(b", ");
            append_marked_compound(out, s, simple, marker);
        }
    }
}

fn scope_selector_list(out: &mut Vec<u8>, s: &[u8], marker: &[u8], frame_scope: bool) {
    let selend = s.len();
    let mut quote = 0u8;
    let (mut paren, mut bracket) = (0u32, 0u32);
    let mut first = true;
    let mut segstart = 0;
    let mut q = 0;
    while q <= selend {
        if q == selend || (quote == 0 && paren == 0 && bracket == 0 && s[q] == b',') {
            if !first {
                out.extend_from_slice(b", ");
            }
            first = false;
            scope_one_selector(out, &s[segstart..q], marker, frame_scope);
            segstart = q + 1;
            if q == selend {
                break;
            }
        } else if quote != 0 {
            if s[q] == b'\\' && q + 1 < selend {
                q += 1;
            } else if s[q] == quote {
                quote = 0;
            }
        } else {
            match s[q] {
                b'\\' if q + 1 < selend => q += 1,
                b'"' | b'\'' => quote = s[q],
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                _ => {}
            }
        }
        q += 1;
    }
}

struct Scoper<'a> {
    s: &'a [u8],
    marker: &'a [u8],
    frame_scope: bool,
    out: Vec<u8>,
}

impl Scoper<'_> {
    fn at_rule(&mut self, prelude: usize, end: usize, depth: i32) -> usize {
        let s = self.s;
        let (seg, term) = scan_until(s, prelude, end, b"{;}");
        if term != b'{' {
            self.out.extend_from_slice(&s[prelude..seg]);
            if term == b';' && seg < end {
                self.out.push(b';');
                return seg + 1;
            }
            return seg;
        }
        let block_end = skip_to_block_end(s, seg, end);
        if GROUP_RULES.iter().any(|kw| starts_ci(&s[prelude..], kw)) {
            self.out.extend_from_slice(&s[prelude..seg]);
            self.out.push(b'{');
            let body = seg + 1;
            self.rule_list(body, block_body_end(s, body, block_end), depth + 1);
            self.out.push(b'}');
        } else {
            self.out.extend_from_slice(&s[prelude..block_end]);
        }
        block_end
    }

    fn rule_list(&mut self, mut p: usize, end: usize, depth: i32) {
        let s = self.s;
        if depth >= MAX_AT_NESTING {
            self.out.extend_from_slice(&s[p..end]);
            return;
        }
        while p < end {
            while p < end && is_ws(s[p]) {
                p += 1;
            }
            if p >= end {
                break;
            }
            if p + 1 < end && s[p] == b'/' && s[p + 1] == b'*' {
                p += 2;
                while p + 1 < end && !(s[p] == b'*' && s[p + 1] == b'/') {
                    p += 1;
                }
                if p + 1 < end {
                    p += 2;
                }
                continue;
            }
            if s[p] == b'}' {
                p += 1;
                continue;
            }
            if s[p] == b'@' {
                p = self.at_rule(p, end, depth);
                continue;
            }
            let (seg, term) = scan_until(s, p, end, b"{;}");
            if term != b'{' {
                p = if seg < end { seg + 1 } else { end };
                continue;
            }
            let block_end = skip_to_block_end(s, seg, end);
            scope_selector_list(&mut self.out, &s[p..seg], self.marker, self.frame_scope);
            self.out.push(b'{');
            let body = seg + 1;
            self.out
                .extend_from_slice(&s[body..block_body_end(s, body, block_end)]);
            self.out.push(b'}');
            p = block_end;
        }
    }
}

pub(crate) fn host_marker(host_id: &[u8]) -> Vec<u8> {
    [b"[data-nd-host=\"".as_slice(), host_id, b"\"]"].concat()
}

pub(crate) fn scope_sheet(flat_css: &[u8], host_id: &[u8], frame_scope: bool) -> Vec<u8> {
    let marker = host_marker(host_id);
    let mut scoper = Scoper {
        s: flat_css,
        marker: &marker,
        frame_scope,
        out: Vec::with_capacity(flat_css.len() * 2),
    };
    scoper.rule_list(0, flat_css.len(), 0);
    scoper.out
}
