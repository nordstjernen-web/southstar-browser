//! Southstar — declarations waiting on var() or attr(): the order they are expanded in and whether a substituted value may stand, so each becomes matched declarations or falls back to unset.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use southstar_dom::Node;

use crate::attr_fn;
use crate::scan::strip_important;

pub(crate) struct Pending {
    pub(crate) origin: i32,
    pub(crate) specificity: (i32, i32, i32),
    pub(crate) sheet_index: i32,
    pub(crate) layer_order: i32,
    pub(crate) scope_order: i32,
    pub(crate) source_order: i32,
    pub(crate) important: bool,
    pub(crate) inline_style: bool,
}

fn ordered(a: i32, b: i32, reversed: bool) -> Ordering {
    if reversed { b.cmp(&a) } else { a.cmp(&b) }
}

fn pending_order(a: &Pending, b: &Pending) -> Ordering {
    a.important
        .cmp(&b.important)
        .then_with(|| ordered(a.origin, b.origin, a.important))
        .then(a.inline_style.cmp(&b.inline_style))
        .then_with(|| ordered(a.layer_order, b.layer_order, a.important))
        .then(a.specificity.cmp(&b.specificity))
        .then(a.scope_order.cmp(&b.scope_order))
        .then(a.sheet_index.cmp(&b.sheet_index))
        .then(a.source_order.cmp(&b.source_order))
}

pub(crate) fn order(list: &[Pending]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..list.len()).collect();
    order.sort_by(|&a, &b| pending_order(&list[a], &list[b]));
    order
}

pub(crate) fn is_custom(name: &[u8]) -> bool {
    name.starts_with(b"--")
}

pub(crate) fn substituted_value(
    raw: &[u8],
    custom: bool,
    expand_vars: impl FnOnce(&[u8]) -> Option<Vec<u8>>,
    node: Option<Node<'_>>,
) -> Option<Vec<u8>> {
    let mut text = expand_vars(raw)?;
    if text.windows(5).any(|w| w == b"attr(") {
        let mut tainted = false;
        text = attr_fn::substitute(&text, node, 0, &mut tainted)?;
        if tainted && !custom {
            return None;
        }
    }
    (!strip_important(&text).1).then_some(text)
}
