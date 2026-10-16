//! Southstar — which elements inline collection lays out as atomic inline boxes, and the frame fallback content it skips.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind as NodeKind, MAX_DEPTH, Node, children};
use southstar_layout_content::controls::{
    is_atomic_inline_container, is_block_level, keyword_is, length_or,
};
use southstar_style::{Display, Kind, PropId, StyleRef, StyleTable, ValueRef, display_of};

const BOX_NORMAL: u8 = 0;
const INTERNAL_NONE: u8 = 0;
const INNER_FLEX: u8 = 3;
const INNER_GRID: u8 = 4;

fn is_flex_container(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == INNER_FLEX
}

fn is_grid_container(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == INNER_GRID
}

fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn button_has_replaced_child(n: Node<'_>, depth: i32) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    children(n).any(|c| {
        matches!(
            c.element_name(),
            Some(b"svg" | b"img" | b"audio" | b"video")
        ) || (c.is_element() && button_has_replaced_child(c, depth + 1))
    })
}

pub fn style_has_atomic_inline_box(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    if is_length_or_calc(s.get(PropId::Width)) || is_length_or_calc(s.get(PropId::Height)) {
        return true;
    }
    if s.get(PropId::BackgroundImage)
        .is_some_and(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
    {
        return true;
    }
    if [
        PropId::BorderRadius,
        PropId::BorderTopLeftRadius,
        PropId::BorderTopRightRadius,
        PropId::BorderBottomRightRadius,
        PropId::BorderBottomLeftRadius,
    ]
    .iter()
    .any(|&p| s.get(p).is_some())
    {
        return true;
    }
    let sides = [
        (PropId::BorderTopWidth, PropId::BorderTopStyle),
        (PropId::BorderRightWidth, PropId::BorderRightStyle),
        (PropId::BorderBottomWidth, PropId::BorderBottomStyle),
        (PropId::BorderLeftWidth, PropId::BorderLeftStyle),
    ];
    for (w, st) in sides {
        if length_or(s.get(w), 0.0) <= 0.0 {
            continue;
        }
        let st = s.get(st);
        if st.is_some() && !keyword_is(st, c"none") && !keyword_is(st, c"hidden") {
            return true;
        }
    }
    s.get(PropId::BackgroundColor)
        .and_then(ValueRef::color)
        .is_some_and(|c| c[3] > 0)
}

pub fn is_inline_level_replaced(n: Option<Node<'_>>, styles: StyleTable) -> bool {
    let Some(n) = n else {
        return false;
    };
    if !matches!(
        n.element_name(),
        Some(b"img" | b"svg" | b"audio" | b"video" | b"math" | b"canvas")
    ) {
        return false;
    }
    !is_block_level(display_of(styles.get(n)))
}

pub fn is_atomic_inline(n: Option<Node<'_>>, styles: StyleTable) -> bool {
    let Some(n) = n else {
        return false;
    };
    let Some(name) = n.element_name() else {
        return false;
    };
    let s = styles.get(n);
    match name {
        b"button" => {
            is_atomic_inline_container(display_of(s))
                || style_has_atomic_inline_box(s)
                || button_has_replaced_child(n, 0)
                || is_length_or_calc(s.and_then(|s| s.get(PropId::Width)))
                || is_length_or_calc(s.and_then(|s| s.get(PropId::Height)))
        }
        b"a" | b"label" | b"summary" => {
            let d = display_of(s);
            is_atomic_inline_container(d) || is_flex_container(d) || is_grid_container(d)
        }
        b"img" | b"svg" | b"audio" | b"video" | b"math" | b"canvas" => {
            is_inline_level_replaced(Some(n), styles)
        }
        b"input" | b"textarea" | b"select" | b"progress" | b"meter" => {
            let atomic_display = is_atomic_inline_container(display_of(s));
            if atomic_display && name == b"input" {
                let checkable = n.attr(c"type").is_some_and(|t| {
                    let t = t.to_bytes();
                    t.eq_ignore_ascii_case(b"radio") || t.eq_ignore_ascii_case(b"checkbox")
                });
                if checkable {
                    let styled = keyword_is(s.and_then(|s| s.get(PropId::Appearance)), c"none")
                        || is_length_or_calc(s.and_then(|s| s.get(PropId::Width)))
                        || is_length_or_calc(s.and_then(|s| s.get(PropId::Height)));
                    if !styled {
                        return false;
                    }
                }
            }
            atomic_display
        }
        b"br" | b"wbr" => false,
        _ => s.is_some_and(|s| is_atomic_inline_container(display_of(Some(s)))),
    }
}

pub fn node_is_frame_fallback(n: Node<'_>) -> bool {
    if n.kind() == NodeKind::Document {
        return false;
    }
    n.parent()
        .and_then(Node::element_name)
        .is_some_and(|p| p == b"iframe" || p == b"frame")
}
