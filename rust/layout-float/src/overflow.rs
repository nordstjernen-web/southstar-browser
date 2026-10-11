//! Southstar — float and clear sides, overflow keywords, clipping and scroll containers, block formatting context roots and multicol column hosts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef};
use southstar_style::{Display, PropId, StyleRef, display_of};

use crate::ffi::{
    dom_of, overflow_axis_keyword, style_is_absolute_or_fixed, style_is_flex_container, style_of,
};

const BOX_NORMAL: u8 = 0;
const OUTER_BLOCK: u8 = 1;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_TABLE_CAPTION: u8 = 8;

fn keyword(s: StyleRef<'_>, prop: PropId) -> Option<&[u8]> {
    s.get(prop)?.keyword_text().map(CStr::to_bytes)
}

pub fn float_side_of(s: Option<StyleRef<'_>>) -> i32 {
    match s.and_then(|s| keyword(s, PropId::Float)) {
        Some(b"left") => 0,
        Some(b"right") => 1,
        _ => -1,
    }
}

pub fn clear_kind_of(s: Option<StyleRef<'_>>) -> i32 {
    match s.and_then(|s| keyword(s, PropId::Clear)) {
        Some(b"left") => 1,
        Some(b"right") => 2,
        Some(b"both") => 3,
        _ => 0,
    }
}

fn keyword_in(ov: Option<&CStr>, words: &[&str]) -> bool {
    ov.is_some_and(|ov| {
        words
            .iter()
            .any(|w| ov.to_bytes().eq_ignore_ascii_case(w.as_bytes()))
    })
}

fn kw_clips(ov: Option<&CStr>) -> bool {
    keyword_in(ov, &["hidden", "clip", "auto", "scroll"])
}

pub fn kw_scrolls(ov: Option<&CStr>) -> bool {
    keyword_in(ov, &["auto", "scroll"])
}

fn kw_scroll_container(ov: Option<&CStr>) -> bool {
    kw_clips(ov) && !keyword_in(ov, &["clip"])
}

pub fn box_clips_children(b: BoxRef<'_>) -> bool {
    if b.style().is_null() {
        return false;
    }
    if !matches!(
        b.kind(),
        BoxKind::Block | BoxKind::TableCaption | BoxKind::TableCell
    ) {
        return false;
    }
    kw_clips(overflow_axis_keyword(style_of(b), PropId::OverflowX))
        || kw_clips(overflow_axis_keyword(style_of(b), PropId::OverflowY))
}

pub fn box_is_scroll_container(b: BoxRef<'_>) -> bool {
    if !box_clips_children(b) {
        return false;
    }
    kw_scroll_container(overflow_axis_keyword(style_of(b), PropId::OverflowX))
        || kw_scroll_container(overflow_axis_keyword(style_of(b), PropId::OverflowY))
}

fn box_padding_contains(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let x0 = b.x() + m.left + bd.left;
    let y0 = b.y() + m.top + bd.top;
    let x1 = x0 + b.content_width() + p.left + p.right;
    let y1 = y0 + b.content_height() + p.top + p.bottom;
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

pub fn clips_out_point(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    box_clips_children(b) && !box_padding_contains(b, x, y)
}

pub fn overflow_establishes_bfc(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    [PropId::Overflow, PropId::OverflowX, PropId::OverflowY]
        .into_iter()
        .filter_map(|p| keyword(s, p))
        .any(|kw| matches!(kw, b"hidden" | b"auto" | b"scroll"))
}

fn style_is_multicol(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    if s.get(PropId::ColumnCount)
        .and_then(|v| v.length())
        .is_some_and(|(v, _)| v >= 2.0)
    {
        return true;
    }
    s.get(PropId::ColumnWidth)
        .and_then(|v| v.length())
        .is_some_and(|(v, _)| v > 0.0)
}

fn display_is_block_level(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.outer == OUTER_BLOCK
        && d.inner != INNER_RUBY
        && (d.internal == INTERNAL_NONE || d.internal == INTERNAL_TABLE_CAPTION)
}

pub fn style_is_grid_container(s: Option<StyleRef<'_>>) -> bool {
    display_of(s).is_grid_container()
}

fn is_document_root_element(dom: Node<'_>) -> bool {
    dom.is_element() && !dom.parent().is_some_and(Node::is_element)
}

pub fn box_establishes_bfc(b: BoxRef<'_>) -> bool {
    let Some(parent) = b.parent() else {
        return true;
    };
    if dom_of(b).is_some_and(is_document_root_element) {
        return true;
    }
    if matches!(
        b.kind(),
        BoxKind::TableCell | BoxKind::Table | BoxKind::TableCaption
    ) {
        return true;
    }
    let style = style_of(b);
    if float_side_of(style) >= 0
        || style_is_absolute_or_fixed(b.style())
        || style_is_multicol(style)
        || style_is_flex_container(b.style())
        || style_is_grid_container(style)
    {
        return true;
    }
    if style_is_flex_container(parent.style()) || style_is_grid_container(style_of(parent)) {
        return true;
    }
    if b.is_rendered_legend()
        || dom_of(b).is_some_and(|d| d.element_name() == Some(b"fieldset".as_slice()))
    {
        return true;
    }
    if style
        .and_then(|s| keyword(s, PropId::Display))
        .is_some_and(|d| matches!(d, b"flow-root" | b"inline-block" | b"inline-table"))
    {
        return true;
    }
    display_is_block_level(display_of(style)) && overflow_establishes_bfc(style)
}

fn is_column_candidate(c: BoxRef<'_>) -> bool {
    !style_is_absolute_or_fixed(c.style()) && float_side_of(style_of(c)) < 0
}

pub fn multicol_distributable_children(b: BoxRef<'_>) -> i32 {
    let mut n = 0;
    let mut c = b.first_child();
    while let Some(child) = c {
        c = child.next_sibling();
        if !matches!(child.kind(), BoxKind::Block | BoxKind::Table) || !is_column_candidate(child) {
            continue;
        }
        n += 1;
        if n >= 2 {
            break;
        }
    }
    n
}

pub fn multicol_column_host(b: BoxRef<'_>) -> Option<BoxRef<'_>> {
    let mut only = None;
    let mut c = b.first_child();
    while let Some(child) = c {
        c = child.next_sibling();
        if child.kind() != BoxKind::Block || !is_column_candidate(child) {
            continue;
        }
        if only.is_some() {
            return None;
        }
        only = Some(child);
    }
    only.filter(|&o| multicol_distributable_children(o) >= 2)
}
