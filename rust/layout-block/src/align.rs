//! Southstar — legacy centering and right-alignment of block children (<center>, align= and -webkit-center), and align-content in block containers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxRef, Style};
use southstar_layout_float::{
    float_side_of, kw_scrolls, overflow_axis_keyword, style_is_grid_container,
};
use southstar_layout_geom::{keyword_or, style_is_absolute_or_fixed, style_is_flex_container};
use southstar_style::{PropId, ValueRef};

use crate::dispatch::shift_box_tree;
use crate::ffi::{dom_of, style_of, style_ref};

pub fn length_is_auto(v: Option<ValueRef<'_>>) -> bool {
    v.and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes() == b"auto")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Legacy {
    Center,
    Right,
}

fn legacy_block_align(c: BoxRef<'_>, inherited: *const Style) -> Option<Legacy> {
    let ta = style_ref(inherited)?
        .keyword_of(PropId::TextAlign)?
        .to_bytes();
    match ta {
        b"-webkit-center" | b"-moz-center" => return Some(Legacy::Center),
        b"-webkit-right" | b"-moz-right" => return Some(Legacy::Right),
        b"center" | b"right" => {}
        _ => return None,
    }
    let legacy = if ta == b"center" {
        Legacy::Center
    } else {
        Legacy::Right
    };
    let mut p = c.parent();
    while let Some(anc) = p {
        p = anc.parent();
        if let Some(s) = style_of(anc) {
            let pt = s.keyword_of(PropId::TextAlign);
            if pt.map(|t| t.to_bytes()) != Some(ta) {
                return None;
            }
        }
        let Some(dom) = dom_of(anc).filter(|d| d.is_element()) else {
            continue;
        };
        if dom.element_name() == Some(b"center".as_slice()) {
            return Some(legacy);
        }
        if dom
            .attr(c"align")
            .is_some_and(|al| al.to_bytes().eq_ignore_ascii_case(ta))
        {
            return Some(legacy);
        }
    }
    None
}

pub fn legacy_align_block_child(
    c: BoxRef<'_>,
    avail_x: f64,
    avail_w: f64,
    inherited: *const Style,
) {
    if !matches!(c.kind(), BoxKind::Block | BoxKind::Table) {
        return;
    }
    if style_is_absolute_or_fixed(style_of(c)) || float_side_of(style_of(c)) >= 0 {
        return;
    }
    let style = style_of(c);
    let ml = style.and_then(|s| s.get(PropId::MarginLeft));
    let mr = style.and_then(|s| s.get(PropId::MarginRight));
    let (m, p, bd) = (c.margin(), c.padding(), c.border());
    if length_is_auto(ml) || length_is_auto(mr) || m.left != 0.0 || m.right != 0.0 {
        return;
    }
    let outer = c.content_width() + p.left + p.right + bd.left + bd.right;
    if outer >= avail_w - 0.5 {
        return;
    }
    let Some(legacy) = legacy_block_align(c, inherited) else {
        return;
    };
    let target_x = match legacy {
        Legacy::Center => avail_x + (avail_w - outer) / 2.0,
        Legacy::Right => avail_x + avail_w - outer,
    };
    shift_box_tree(c, target_x - c.x(), 0.0);
}

pub fn block_align_content_shift(b: BoxRef<'_>, free_space: f64) -> f64 {
    let Some(style) = style_of(b) else {
        return 0.0;
    };
    if free_space == 0.0 {
        return 0.0;
    }
    if style_is_flex_container(style_of(b)) || style_is_grid_container(Some(style)) {
        return 0.0;
    }
    let mut acont = keyword_or(style_of(b), PropId::AlignContent, c"normal").to_bytes();
    let unsafe_align = acont.starts_with(b"unsafe ");
    if unsafe_align {
        acont = &acont[b"unsafe ".len()..];
    } else if acont.starts_with(b"safe ") {
        acont = &acont[b"safe ".len()..];
    }
    if free_space < 0.0
        && (!unsafe_align || kw_scrolls(overflow_axis_keyword(style_of(b), PropId::OverflowY)))
    {
        return 0.0;
    }
    match acont {
        b"center" | b"space-around" | b"space-evenly" => free_space / 2.0,
        b"end" | b"flex-end" => free_space,
        _ => 0.0,
    }
}
