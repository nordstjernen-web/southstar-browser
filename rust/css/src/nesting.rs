//! Southstar — CSS nesting flattened before a style sheet is parsed: nested style rules joined to their parents through :is() and &, nested group rules lifted around their parents, within a selector budget that stops nesting from growing a sheet exponentially.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::scan::{
    is_ws, scan_until, skip_comment, skip_to_block_end, skip_ws_comments, strip, trim_range,
};

const NEST_MAX_DEPTH: i32 = 128;
const NEST_SELECTOR_BUDGET: usize = 16 * 1024 * 1024;

const GROUP_RULES: [&[u8]; 5] = [b"@media", b"@supports", b"@container", b"@layer", b"@scope"];

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn is_group_rule(prelude: &[u8]) -> bool {
    GROUP_RULES.iter().any(|kw| starts_ci(prelude, kw))
}

pub(crate) fn block_body_end(s: &[u8], body_start: usize, block_end: usize) -> usize {
    if block_end > body_start && s[block_end - 1] == b'}' {
        block_end - 1
    } else {
        block_end
    }
}

pub(crate) fn skip_invalid_qualified_rule(
    s: &[u8],
    mut p: usize,
    end: usize,
    nested: bool,
) -> usize {
    while p < end {
        let (seg, term) = scan_until(s, p, end, b"{;}");
        if term == b'{' {
            return skip_to_block_end(s, seg, end);
        }
        if term == 0 || seg >= end {
            return end;
        }
        if term == b'}' && nested {
            return seg;
        }
        p = seg + 1;
    }
    end
}

fn trim_selector(sel: &[u8]) -> Vec<u8> {
    let start = sel.iter().position(|&c| !is_ws(c)).unwrap_or(sel.len());
    let sel = &sel[start..];
    let mut n = sel.len();
    while n > 0 && is_ws(sel[n - 1]) {
        let backslashes = sel[..n - 1]
            .iter()
            .rev()
            .take_while(|&&c| c == b'\\')
            .count();
        if backslashes % 2 == 1 {
            break;
        }
        n -= 1;
    }
    sel[..n].to_vec()
}

fn append_nested_selector(out: &mut Vec<u8>, part: &[u8], parent: &[u8]) -> bool {
    let end = part.len();
    let mut p = 0;
    let mut quote = 0u8;
    let mut bracket = 0u32;
    let mut replaced = false;
    while p < end {
        let c = part[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                out.extend_from_slice(&part[p..p + 2]);
                p += 2;
                continue;
            }
            out.push(c);
            if c == quote {
                quote = 0;
            }
            p += 1;
            continue;
        }
        if c == b'/' && p + 1 < end && part[p + 1] == b'*' {
            let q = skip_comment(part, p, end);
            out.extend_from_slice(&part[p..q]);
            p = q;
            continue;
        }
        if c == b'\\' && p + 1 < end {
            out.extend_from_slice(&part[p..p + 2]);
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            out.push(c);
            p += 1;
            continue;
        }
        if c == b'[' {
            bracket += 1;
        } else if c == b']' && bracket > 0 {
            bracket -= 1;
        }
        if c == b'&' && bracket == 0 {
            out.extend_from_slice(parent);
            replaced = true;
            p += 1;
            continue;
        }
        out.push(c);
        p += 1;
    }
    replaced
}

fn combine_selectors(parent: &[u8], child: &[u8], max_len: usize) -> Option<Vec<u8>> {
    let pc = strip(parent);
    let cc = strip(child);
    let per_parent = pc.len() + 6;
    let is_parent = [b":is(".as_slice(), pc, b")"].concat();
    let mut out = Vec::new();
    let end = cc.len();
    let mut p = 0;
    while p < end {
        let (seg, term) = scan_until(cc, p, end, b",");
        let part = trim_range(cc, p, seg);
        let next = if term == b',' { seg + 1 } else { seg };
        if part.is_empty() {
            p = next;
            continue;
        }
        let amps = part.iter().filter(|&&c| c == b'&').count();
        let room = max_len.wrapping_sub(out.len());
        if part.len() + 2 > room || amps >= (room - part.len() - 2) / per_parent {
            return None;
        }
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        let mut piece = Vec::new();
        if append_nested_selector(&mut piece, part, &is_parent) {
            out.extend_from_slice(&piece);
        } else {
            out.extend_from_slice(&is_parent);
            out.push(b' ');
            out.extend_from_slice(part);
        }
        p = next;
    }
    Some(out)
}

fn take_budget(budget: &mut usize, sel: &[u8]) -> bool {
    if *budget == 0 {
        return false;
    }
    if sel.len() > *budget {
        *budget = 0;
        return false;
    }
    *budget -= sel.len();
    true
}

fn flush_decls(out: &mut Vec<u8>, sel: &[u8], decls: &mut Vec<u8>, budget: &mut usize) {
    if decls.is_empty() {
        return;
    }
    if take_budget(budget, sel) {
        out.extend_from_slice(sel);
        out.push(b'{');
        out.extend_from_slice(decls);
        out.push(b'}');
    }
    decls.clear();
}

fn skip_raw_comment(s: &[u8], p: usize, end: usize) -> usize {
    let mut p = p + 2;
    while p + 1 < end && !(s[p] == b'*' && s[p + 1] == b'/') {
        p += 1;
    }
    if p + 1 < end { p + 2 } else { p }
}

fn body_has_nested_rule(s: &[u8], mut p: usize, end: usize) -> bool {
    while p < end {
        while p < end && is_ws(s[p]) {
            p += 1;
        }
        if p >= end {
            break;
        }
        if p + 1 < end && s[p] == b'/' && s[p + 1] == b'*' {
            p = skip_raw_comment(s, p, end);
            continue;
        }
        let (seg_end, term) = scan_until(s, p, end, b"{;}");
        if term == b'{' {
            return true;
        }
        if term == 0 {
            break;
        }
        p = seg_end + 1;
    }
    false
}

struct Flattener<'a> {
    s: &'a [u8],
    out: Vec<u8>,
    budget: usize,
}

impl Flattener<'_> {
    fn rule_list(&mut self, mut p: usize, end: usize, depth: i32) {
        if depth > NEST_MAX_DEPTH {
            return;
        }
        let s = self.s;
        while p < end {
            p = skip_ws_comments(s, p, end);
            if p >= end {
                break;
            }
            if p + 4 <= end && &s[p..p + 4] == b"<!--" {
                p += 4;
                continue;
            }
            if p + 3 <= end && &s[p..p + 3] == b"-->" {
                p += 3;
                continue;
            }
            if s[p] == b'}' {
                p = skip_invalid_qualified_rule(s, p + 1, end, false);
                continue;
            }
            let (seg_end, term) = scan_until(s, p, end, b"{;}");
            if s[p] == b'@' {
                let prelude = p;
                if term == b'{' {
                    let block_end = skip_to_block_end(s, seg_end, end);
                    if is_group_rule(&s[prelude..end]) {
                        self.out.extend_from_slice(&s[prelude..seg_end]);
                        self.out.push(b'{');
                        let body_s = seg_end + 1;
                        self.rule_list(body_s, block_body_end(s, body_s, block_end), depth + 1);
                        self.out.push(b'}');
                    } else {
                        self.out.extend_from_slice(&s[prelude..block_end]);
                    }
                    p = block_end;
                } else {
                    self.out.extend_from_slice(&s[prelude..seg_end]);
                    if term == b';' && seg_end < end {
                        self.out.push(b';');
                        p = seg_end + 1;
                    } else {
                        p = seg_end;
                    }
                }
                continue;
            }
            if term != b'{' {
                p = skip_invalid_qualified_rule(s, p, end, false);
                continue;
            }
            let sel = trim_selector(&s[p..seg_end]);
            let body_s = seg_end + 1;
            let block_end = skip_to_block_end(s, seg_end, end);
            let body_e = block_body_end(s, body_s, block_end);
            self.style_rule(&sel, body_s, body_e, depth + 1);
            p = block_end;
        }
    }

    fn style_rule(&mut self, sel: &[u8], body_s: usize, body_e: usize, depth: i32) {
        if depth > NEST_MAX_DEPTH {
            return;
        }
        let s = self.s;
        if !body_has_nested_rule(s, body_s, body_e) {
            if take_budget(&mut self.budget, sel) {
                self.out.extend_from_slice(sel);
                self.out.push(b'{');
                self.out.extend_from_slice(&s[body_s..body_e]);
                self.out.push(b'}');
            }
            return;
        }
        let mut decls = Vec::new();
        let mut p = body_s;
        while p < body_e {
            while p < body_e && is_ws(s[p]) {
                p += 1;
            }
            if p >= body_e {
                break;
            }
            if p + 1 < body_e && s[p] == b'/' && s[p + 1] == b'*' {
                let comment_start = p;
                p = skip_raw_comment(s, p, body_e);
                decls.extend_from_slice(&s[comment_start..p]);
                continue;
            }
            let (seg_end, term) = scan_until(s, p, body_e, b"{;}");
            if term == b'{' {
                flush_decls(&mut self.out, sel, &mut decls, &mut self.budget);
                let nested = trim_selector(&s[p..seg_end]);
                let nbody_s = seg_end + 1;
                let nblock_end = skip_to_block_end(s, seg_end, body_e);
                let nbody_e = block_body_end(s, nbody_s, nblock_end);
                if nested.first() == Some(&b'@') {
                    if is_group_rule(&nested) {
                        self.out.extend_from_slice(&nested);
                        self.out.push(b'{');
                        self.style_rule(sel, nbody_s, nbody_e, depth + 1);
                        self.out.push(b'}');
                    }
                } else {
                    let combined = if self.budget != 0 {
                        combine_selectors(sel, &nested, self.budget)
                    } else {
                        None
                    };
                    match combined {
                        None => self.budget = 0,
                        Some(combined) => {
                            if take_budget(&mut self.budget, &combined) {
                                self.style_rule(&combined, nbody_s, nbody_e, depth + 1);
                            }
                        }
                    }
                }
                p = nblock_end;
            } else {
                decls.extend_from_slice(&s[p..seg_end]);
                if term == b';' {
                    decls.push(b';');
                }
                p = if seg_end < body_e {
                    seg_end + 1
                } else {
                    body_e
                };
            }
        }
        flush_decls(&mut self.out, sel, &mut decls, &mut self.budget);
    }
}

pub(crate) fn flatten(text: &[u8]) -> Vec<u8> {
    let len = text.len();
    let budget = if len <= (usize::MAX - NEST_SELECTOR_BUDGET) / 16 {
        len * 16 + NEST_SELECTOR_BUDGET
    } else {
        usize::MAX
    };
    let mut flattener = Flattener {
        s: text,
        out: Vec::with_capacity(len),
        budget,
    };
    flattener.rule_list(0, len, 0);
    flattener.out
}

pub(crate) fn has_container_units(text: &[u8]) -> bool {
    (0..text.len()).any(|p| {
        let rest = &text[p..];
        (rest[0] == b'c' || rest[0] == b'C')
            && [&b"cqw"[..], b"cqh", b"cqi", b"cqb", b"cqmin", b"cqmax"]
                .iter()
                .any(|unit| starts_ci(rest, unit))
    })
}
