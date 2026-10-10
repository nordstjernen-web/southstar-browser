//! Southstar — the order hit testing visits boxes in: z-index stack levels, tree order within a level, and the styles that take a box out of hit testing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use southstar_dom::Node;
use southstar_dom::index::document_order_cmp;
use southstar_layout::{BoxKind, BoxRef, children};
use southstar_style::{Kind, PropId, StyleRef};

use crate::ffi;
use crate::geometry::position_keyword;

pub fn node_is_form_hit_target(n: Option<Node<'_>>) -> bool {
    n.filter(|n| n.is_element())
        .and_then(Node::name)
        .is_some_and(|name| {
            matches!(
                name.to_bytes(),
                b"button" | b"input" | b"select" | b"textarea"
            )
        })
}

pub fn style_blocks_hit_testing(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    s.get(PropId::PointerEvents)
        .is_some_and(|v| v.is_keyword(c"none"))
        || s.keyword_of(PropId::Visibility)
            .is_some_and(|v| v == c"hidden" || v == c"collapse")
}

pub fn box_blocks_hit_testing(b: BoxRef<'_>) -> bool {
    style_blocks_hit_testing(ffi::style_of(b))
}

pub fn positioned_z_index(b: BoxRef<'_>) -> Option<i32> {
    let s = ffi::style_of(b);
    let kw = position_keyword(s)?;
    if !matches!(
        kw.to_bytes(),
        b"relative" | b"absolute" | b"fixed" | b"sticky"
    ) {
        return None;
    }
    Some(
        s.and_then(|s| s.get(PropId::ZIndex))
            .and_then(|v| v.length())
            .map_or(0, |(v, _)| v as i32),
    )
}

fn stack_key(b: BoxRef<'_>) -> i32 {
    positioned_z_index(b).unwrap_or(0)
}

pub fn tree_order_cmp(a: BoxRef<'_>, b: BoxRef<'_>, order_a: usize, order_b: usize) -> Ordering {
    if let (Some(da), Some(db)) = (ffi::dom_of(a), ffi::dom_of(b))
        && da != db
    {
        let c = document_order_cmp(Some(da), Some(db));
        if c != 0 {
            return c.cmp(&0);
        }
    }
    order_a.cmp(&order_b)
}

pub fn merge_sort<T: Copy>(items: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering) {
    if items.len() < 2 {
        return;
    }
    let mid = items.len() / 2;
    merge_sort(&mut items[..mid], cmp);
    merge_sort(&mut items[mid..], cmp);
    let (left, right) = items.split_at(mid);
    let mut merged = Vec::with_capacity(items.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if cmp(&right[j], &left[i]) == Ordering::Less {
            merged.push(right[j]);
            j += 1;
        } else {
            merged.push(left[i]);
            i += 1;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    items.copy_from_slice(&merged);
}

pub fn children_stacked(parent: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let mut entries: Vec<(BoxRef<'_>, i32, usize)> = children(parent)
        .enumerate()
        .map(|(order, c)| (c, stack_key(c), order))
        .collect();
    if entries.iter().all(|&(_, key, _)| key == 0) {
        return entries.into_iter().map(|(c, _, _)| c).collect();
    }
    merge_sort(&mut entries, &|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| tree_order_cmp(a.0, b.0, a.2, b.2))
    });
    entries.into_iter().map(|(c, _, _)| c).collect()
}

pub fn svg_yields_hit(b: BoxRef<'_>) -> bool {
    if b.kind() != BoxKind::Svg {
        return false;
    }
    let Some(s) = ffi::style_of(b) else {
        return true;
    };
    if s.get(PropId::PointerEvents)
        .is_some_and(|v| v.is_keyword(c"all"))
    {
        return false;
    }
    if s.get(PropId::BackgroundColor)
        .and_then(|v| v.color())
        .is_some_and(|c| c[3] > 0)
    {
        return false;
    }
    if s.get(PropId::BackgroundImage)
        .is_some_and(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
    {
        return false;
    }
    let br = b.border();
    br.top <= 0.0 && br.right <= 0.0 && br.bottom <= 0.0 && br.left <= 0.0
}
