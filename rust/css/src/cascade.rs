//! Southstar — applying an element's matched declarations: cascade order, revert, revert-layer and revert-rule rolled back to the declaration they reveal, and the display and overflow fixups a computed style needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use crate::display::{self, Display};
use crate::prop::Prop;

const ORIGIN_PRESENTATIONAL: i32 = 1;
const ORIGIN_AUTHOR: i32 = 2;
const LAYER_NONE: i32 = i32::MAX;

pub(crate) const CURRENTCOLOR_PROPS: [Prop; 13] = [
    Prop::BackgroundColor,
    Prop::BorderTopColor,
    Prop::BorderRightColor,
    Prop::BorderBottomColor,
    Prop::BorderLeftColor,
    Prop::OutlineColor,
    Prop::TextDecorationColor,
    Prop::ColumnRuleColor,
    Prop::AccentColor,
    Prop::CaretColor,
    Prop::Fill,
    Prop::Stroke,
    Prop::StopColor,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Revert {
    Origin,
    Layer,
    Rule,
}

pub(crate) struct Entry {
    pub(crate) origin: i32,
    pub(crate) specificity: (i32, i32, i32),
    pub(crate) sheet_index: i32,
    pub(crate) layer_order: i32,
    pub(crate) scope_order: i32,
    pub(crate) source_order: i32,
    pub(crate) decl_order: i32,
    pub(crate) important: bool,
    pub(crate) inline_style: bool,
    pub(crate) rule: usize,
    pub(crate) prop: usize,
    pub(crate) revert: Option<Revert>,
}

fn ordered(a: i32, b: i32, reversed: bool) -> Ordering {
    if reversed { b.cmp(&a) } else { a.cmp(&b) }
}

pub(crate) fn cascade_order(a: &Entry, b: &Entry) -> Ordering {
    a.important
        .cmp(&b.important)
        .then_with(|| ordered(a.origin, b.origin, a.important))
        .then(a.inline_style.cmp(&b.inline_style))
        .then_with(|| ordered(a.layer_order, b.layer_order, a.important))
        .then(a.specificity.cmp(&b.specificity))
        .then(a.scope_order.cmp(&b.scope_order))
        .then(a.sheet_index.cmp(&b.sheet_index))
        .then(a.source_order.cmp(&b.source_order))
        .then(a.decl_order.cmp(&b.decl_order))
}

fn same_revert_origin(rollback: i32, candidate: i32) -> bool {
    if rollback == ORIGIN_AUTHOR {
        candidate == ORIGIN_AUTHOR || candidate == ORIGIN_PRESENTATIONAL
    } else {
        rollback == candidate
    }
}

fn hidden_by(rollback: &Entry, kind: Revert, prev: &Entry) -> bool {
    match kind {
        Revert::Rule => prev.rule == rollback.rule,
        Revert::Layer => {
            prev.origin == rollback.origin
                && if rollback.inline_style {
                    prev.inline_style
                } else if rollback.layer_order == LAYER_NONE {
                    prev.layer_order == LAYER_NONE
                } else {
                    prev.layer_order >= rollback.layer_order
                }
        }
        Revert::Origin => same_revert_origin(rollback.origin, prev.origin),
    }
}

fn rollback(entries: &[Entry], order: &[usize], before: usize, from: usize) -> Option<usize> {
    let current = &entries[order[from]];
    let kind = current.revert?;
    for j in (0..before).rev() {
        let prev = &entries[order[j]];
        if prev.prop != current.prop || hidden_by(current, kind, prev) {
            continue;
        }
        if prev.revert.is_some() {
            return rollback(entries, order, j, j);
        }
        return Some(order[j]);
    }
    None
}

pub(crate) fn winners(entries: &[Entry], prop_count: usize) -> Vec<Option<Option<usize>>> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| cascade_order(&entries[a], &entries[b]));
    let mut won = vec![None; prop_count];
    for (i, &e) in order.iter().enumerate() {
        let entry = &entries[e];
        let Some(slot) = won.get_mut(entry.prop) else {
            continue;
        };
        *slot = Some(if entry.revert.is_some() {
            rollback(entries, &order, i, i)
        } else {
            Some(e)
        });
    }
    won
}

pub(crate) enum OverflowFix {
    XAuto,
    YAuto,
}

pub(crate) fn overflow_fix(x: Option<&[u8]>, y: Option<&[u8]>) -> Option<OverflowFix> {
    let visible = |k: Option<&[u8]>| k.is_none_or(|k| k == b"visible");
    let scrolls = |k: Option<&[u8]>| k.is_some_and(|k| k != b"visible" && k != b"clip");
    if visible(x) && scrolls(y) {
        Some(OverflowFix::XAuto)
    } else if visible(y) && scrolls(x) {
        Some(OverflowFix::YAuto)
    } else {
        None
    }
}

pub(crate) struct DisplayContext {
    pub(crate) out_of_flow: bool,
    pub(crate) layout_parent: Display,
    pub(crate) is_root: bool,
}

fn inner_is(d: Display, inner: u8) -> bool {
    d.box_ == display::BOX_NORMAL && d.internal == display::INTERNAL_NONE && d.inner == inner
}

pub(crate) fn blockify(d: Display, cx: &DisplayContext) -> Display {
    if d.box_ == display::BOX_NONE {
        return d;
    }
    if cx.is_root {
        let mut d = d;
        if d.box_ == display::BOX_CONTENTS {
            d.box_ = display::BOX_NORMAL;
            d.inner = display::INNER_FLOW;
        }
        return display::blockified(d);
    }
    if d.box_ != display::BOX_NORMAL {
        return d;
    }
    let parent = cx.layout_parent;
    if cx.out_of_flow
        || inner_is(parent, display::INNER_FLEX)
        || inner_is(parent, display::INNER_GRID)
    {
        return display::blockified(d);
    }
    d
}
