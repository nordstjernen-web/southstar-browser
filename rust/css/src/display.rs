//! Southstar — the display property: its keywords and multi-keyword forms parsed into the ns_display css.h declares, their canonical spelling, blockification, and the overflow-clip-margin value.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::ffi;
use crate::math::math_canonical;
use crate::scan::{is_ws, split_ws_limit, split_ws_paren, trim_range};
use crate::time::starts_math_fn;
use crate::units::{self, NUMBER, PERCENT, number_text};

pub(crate) const BOX_NORMAL: u8 = 0;
pub(crate) const BOX_NONE: u8 = 1;
pub(crate) const BOX_CONTENTS: u8 = 2;

pub(crate) const OUTER_INLINE: u8 = 0;
const OUTER_BLOCK: u8 = 1;
const OUTER_RUN_IN: u8 = 2;

pub(crate) const INNER_FLOW: u8 = 0;
pub(crate) const INNER_FLOW_ROOT: u8 = 1;
const INNER_TABLE: u8 = 2;
pub(crate) const INNER_FLEX: u8 = 3;
pub(crate) const INNER_GRID: u8 = 4;
const INNER_RUBY: u8 = 5;

pub(crate) const INTERNAL_NONE: u8 = 0;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Display {
    pub box_: u8,
    pub outer: u8,
    pub inner: u8,
    pub internal: u8,
    pub list_item: u8,
}

const _: () = assert!(core::mem::size_of::<Display>() == 5);

const fn d(box_: u8, outer: u8, inner: u8, internal: u8, list_item: u8) -> Display {
    Display {
        box_,
        outer,
        inner,
        internal,
        list_item,
    }
}

const KEYWORDS: [(&[u8], Display); 28] = [
    (b"none", d(BOX_NONE, OUTER_BLOCK, 0, 0, 0)),
    (b"contents", d(BOX_CONTENTS, OUTER_BLOCK, 0, 0, 0)),
    (b"block", d(0, OUTER_BLOCK, 0, 0, 0)),
    (b"flow", d(0, OUTER_BLOCK, 0, 0, 0)),
    (b"flow-root", d(0, OUTER_BLOCK, INNER_FLOW_ROOT, 0, 0)),
    (b"table", d(0, OUTER_BLOCK, INNER_TABLE, 0, 0)),
    (b"flex", d(0, OUTER_BLOCK, INNER_FLEX, 0, 0)),
    (b"-webkit-box", d(0, OUTER_BLOCK, INNER_FLEX, 0, 0)),
    (b"grid", d(0, OUTER_BLOCK, INNER_GRID, 0, 0)),
    (b"list-item", d(0, OUTER_BLOCK, 0, 0, 1)),
    (b"inline", d(0, OUTER_INLINE, 0, 0, 0)),
    (b"inline-block", d(0, OUTER_INLINE, INNER_FLOW_ROOT, 0, 0)),
    (b"inline-table", d(0, OUTER_INLINE, INNER_TABLE, 0, 0)),
    (b"inline-flex", d(0, OUTER_INLINE, INNER_FLEX, 0, 0)),
    (b"-webkit-inline-box", d(0, OUTER_INLINE, INNER_FLEX, 0, 0)),
    (b"inline-grid", d(0, OUTER_INLINE, INNER_GRID, 0, 0)),
    (b"ruby", d(0, OUTER_INLINE, INNER_RUBY, 0, 0)),
    (b"run-in", d(0, OUTER_RUN_IN, 0, 0, 0)),
    (b"table-row-group", d(0, OUTER_BLOCK, 0, 1, 0)),
    (b"table-header-group", d(0, OUTER_BLOCK, 0, 2, 0)),
    (b"table-footer-group", d(0, OUTER_BLOCK, 0, 3, 0)),
    (b"table-row", d(0, OUTER_BLOCK, 0, 4, 0)),
    (b"table-cell", d(0, OUTER_BLOCK, 0, 5, 0)),
    (b"table-column-group", d(0, OUTER_BLOCK, 0, 6, 0)),
    (b"table-column", d(0, OUTER_BLOCK, 0, 7, 0)),
    (b"table-caption", d(0, OUTER_BLOCK, 0, 8, 0)),
    (b"ruby-base", d(0, OUTER_INLINE, 0, 9, 0)),
    (b"ruby-text", d(0, OUTER_INLINE, 0, 10, 0)),
];

const INTERNAL_NAMES: [&[u8]; 11] = [
    b"",
    b"table-row-group",
    b"table-header-group",
    b"table-footer-group",
    b"table-row",
    b"table-cell",
    b"table-column-group",
    b"table-column",
    b"table-caption",
    b"ruby-base",
    b"ruby-text",
];
const BLOCK_NAMES: [&[u8]; 6] = [
    b"block",
    b"flow-root",
    b"table",
    b"flex",
    b"grid",
    b"block ruby",
];
const INLINE_NAMES: [&[u8]; 6] = [
    b"inline",
    b"inline-block",
    b"inline-table",
    b"inline-flex",
    b"inline-grid",
    b"ruby",
];
const INNER_NAMES: [&[u8]; 6] = [b"flow", b"flow-root", b"table", b"flex", b"grid", b"ruby"];

fn outer_from_token(tok: &[u8]) -> Option<u8> {
    match tok {
        b"block" => Some(OUTER_BLOCK),
        b"inline" => Some(OUTER_INLINE),
        b"run-in" => Some(OUTER_RUN_IN),
        _ => None,
    }
}

fn inner_from_token(tok: &[u8]) -> Option<u8> {
    INNER_NAMES
        .iter()
        .position(|name| *name == tok)
        .map(|i| i as u8)
}

pub(crate) fn parse(lowered: &[u8]) -> Option<Display> {
    if let Some((_, display)) = KEYWORDS.iter().find(|(name, _)| *name == lowered) {
        return Some(*display);
    }
    let tokens = split_ws_limit(lowered, 5);
    if !(2..=3).contains(&tokens.len()) {
        return None;
    }
    let mut out = Display::default();
    let (mut have_outer, mut have_inner) = (false, false);
    for tok in tokens {
        if tok == b"list-item" {
            if out.list_item != 0 {
                return None;
            }
            out.list_item = 1;
        } else if let Some(slot) = outer_from_token(tok) {
            if have_outer {
                return None;
            }
            have_outer = true;
            out.outer = slot;
        } else if let Some(slot) = inner_from_token(tok) {
            if have_inner {
                return None;
            }
            have_inner = true;
            out.inner = slot;
        } else {
            return None;
        }
    }
    if out.list_item != 0 && out.inner != INNER_FLOW && out.inner != INNER_FLOW_ROOT {
        return None;
    }
    if !have_outer {
        out.outer = if out.inner == INNER_RUBY {
            OUTER_INLINE
        } else {
            OUTER_BLOCK
        };
    }
    Some(out)
}

fn name_at(names: &[&'static [u8]], i: u8) -> Vec<u8> {
    names
        .get(usize::from(i))
        .copied()
        .unwrap_or_default()
        .to_vec()
}

pub(crate) fn serialize(d: Display) -> Vec<u8> {
    if d.box_ == BOX_NONE {
        return b"none".to_vec();
    }
    if d.box_ == BOX_CONTENTS {
        return b"contents".to_vec();
    }
    if d.internal != INTERNAL_NONE {
        return name_at(&INTERNAL_NAMES, d.internal);
    }
    if d.list_item != 0 {
        let froot = d.inner == INNER_FLOW_ROOT;
        let text: &[u8] = match (d.outer, froot) {
            (OUTER_BLOCK, true) => b"flow-root list-item",
            (OUTER_BLOCK, false) => b"list-item",
            (OUTER_INLINE, true) => b"inline flow-root list-item",
            (OUTER_INLINE, false) => b"inline list-item",
            (_, true) => b"run-in flow-root list-item",
            (_, false) => b"run-in list-item",
        };
        return text.to_vec();
    }
    match d.outer {
        OUTER_BLOCK => name_at(&BLOCK_NAMES, d.inner),
        OUTER_INLINE => name_at(&INLINE_NAMES, d.inner),
        _ if d.inner == INNER_FLOW => b"run-in".to_vec(),
        _ => {
            let mut out = b"run-in ".to_vec();
            out.extend_from_slice(&name_at(&INNER_NAMES, d.inner));
            out
        }
    }
}

pub(crate) fn from_keyword(canonical: Option<&[u8]>) -> Display {
    canonical.and_then(parse).unwrap_or_default()
}

pub(crate) fn blockified(mut d: Display) -> Display {
    if d.box_ != BOX_NORMAL {
        return d;
    }
    if d.internal != INTERNAL_NONE {
        d.internal = INTERNAL_NONE;
        d.inner = INNER_FLOW;
    } else if d.outer == OUTER_INLINE && d.list_item == 0 && d.inner == INNER_FLOW_ROOT {
        d.inner = INNER_FLOW;
    }
    d.outer = OUTER_BLOCK;
    d
}

pub(crate) fn normalize(text: &[u8]) -> Option<Vec<u8>> {
    let kw = text.to_ascii_lowercase();
    let kw = &kw[..kw.iter().position(|&c| c == 0).unwrap_or(kw.len())];
    if kw == b"-webkit-box" || kw == b"-webkit-inline-box" {
        return Some(kw.to_vec());
    }
    let standard: &[u8] = match kw {
        b"-webkit-flex" | b"-ms-flexbox" => b"flex",
        b"-webkit-inline-flex" | b"-ms-inline-flexbox" => b"inline-flex",
        b"-webkit-grid" | b"-ms-grid" => b"grid",
        _ => kw,
    };
    parse(standard).map(serialize)
}

pub(crate) fn canonical(value: &[u8]) -> Option<Vec<u8>> {
    let start = value.iter().position(|&c| !is_ws(c)).unwrap_or(value.len());
    let trimmed = trim_range(value, start, value.len());
    normalize(trimmed)
}

pub(crate) fn overflow_clip_margin_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let toks = split_ws_paren(text, 3);
    if !(1..=2).contains(&toks.len()) {
        return None;
    }
    let mut box_: Option<&'static [u8]> = None;
    let mut len: Option<Vec<u8>> = None;
    for tok in toks {
        if box_.is_none() {
            let found: Option<&'static [u8]> = [&b"content-box"[..], b"padding-box", b"border-box"]
                .into_iter()
                .find(|name| tok.eq_ignore_ascii_case(name));
            if found.is_some() {
                box_ = found;
                continue;
            }
        }
        if len.is_none() {
            if starts_math_fn(tok) {
                let text = CString::new(tok).ok()?;
                len = Some(math_canonical(&text).unwrap_or_else(|| tok.to_vec()));
                continue;
            }
            let text = CString::new(tok).ok()?;
            if let Some((v, u)) = units::parse_length(&text) {
                if u != PERCENT && (u != NUMBER || v == 0.0) && v >= 0.0 {
                    len = Some(if v == 0.0 {
                        b"0px".to_vec()
                    } else {
                        let (_, end) = ffi::strtod(&text, 0);
                        let mut out = number_text(v);
                        out.extend_from_slice(&text.to_bytes()[end..].to_ascii_lowercase());
                        out
                    });
                    continue;
                }
            }
        }
        return None;
    }
    let zero = len.as_deref().is_none_or(|l| l == b"0px");
    let padding = box_.is_none_or(|b| b == b"padding-box");
    Some(if padding {
        if zero {
            b"0px".to_vec()
        } else {
            len.unwrap_or_default()
        }
    } else if zero {
        box_.unwrap_or_default().to_vec()
    } else {
        let mut out = box_.unwrap_or_default().to_vec();
        out.push(b' ');
        out.extend_from_slice(&len.unwrap_or_default());
        out
    })
}
