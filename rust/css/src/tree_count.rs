//! Southstar — the tree-counting functions sibling-index() and sibling-count(), replaced by the element's position among its element siblings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, children};

use crate::scan::is_ident;

const INDEX: &[u8] = b"sibling-index(";
const COUNT: &[u8] = b"sibling-count(";

fn find_call(text: &[u8], from: usize) -> Option<(usize, usize, bool)> {
    let mut at = from;
    while at + INDEX.len() <= text.len() {
        let rest = &text[at..];
        let index = rest[..INDEX.len()].eq_ignore_ascii_case(INDEX);
        let count = rest[..COUNT.len()].eq_ignore_ascii_case(COUNT);
        if (index || count) && (at == 0 || !is_ident(text[at - 1])) {
            let mut close = at + INDEX.len();
            while text.get(close).is_some_and(u8::is_ascii_whitespace) {
                close += 1;
            }
            if text.get(close) == Some(&b')') {
                return Some((at, close + 1, index));
            }
        }
        at += 1;
    }
    None
}

pub(crate) fn uses_tree_counting(text: &[u8]) -> bool {
    find_call(text, 0).is_some()
}

fn position(node: Option<Node<'_>>) -> (usize, usize) {
    let Some(node) = node.filter(|n| n.is_element()) else {
        return (1, 1);
    };
    let Some(parent) = node.parent() else {
        return (1, 1);
    };
    let mut index = 0;
    let mut count = 0;
    for sibling in children(parent).filter(|c| c.is_element()) {
        count += 1;
        if sibling == node {
            index = count;
        }
    }
    (index, count)
}

pub(crate) fn substitute(text: &[u8], node: Option<Node<'_>>) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut from = 0;
    let mut place = None;
    while let Some((start, end, index)) = find_call(text, from) {
        let (i, c) = *place.get_or_insert_with(|| position(node));
        out.extend_from_slice(&text[from..start]);
        out.extend_from_slice((if index { i } else { c }).to_string().as_bytes());
        from = end;
    }
    out.extend_from_slice(&text[from..]);
    out
}
