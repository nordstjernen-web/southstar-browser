//! Southstar — shadow-tree boundaries in the node tree: shadow roots, the DOM standard's retarget algorithm and a relatedTarget a listener may see.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{Kind, MAX_DEPTH, Node, ancestors_and_self};

pub fn is_shadow_root(node: Node<'_>) -> bool {
    node.kind() == Kind::Element && node.attr(c"data-nd-shadow-root").is_some()
}

fn shadow_root_of(node: Node<'_>) -> Option<Node<'_>> {
    ancestors_and_self(node).find(|&n| is_shadow_root(n))
}

fn inclusive_ancestor(ancestor: Node<'_>, node: Node<'_>) -> bool {
    ancestors_and_self(node).any(|n| n == ancestor)
}

fn owner_document(node: Node<'_>) -> Option<Node<'_>> {
    ancestors_and_self(node).find(|n| n.kind() == Kind::Document)
}

pub fn retarget<'a>(target: Node<'a>, against: Node<'_>) -> Node<'a> {
    let mut visible = target;
    for _ in 0..MAX_DEPTH {
        let Some(root) = shadow_root_of(visible) else {
            return visible;
        };
        if inclusive_ancestor(root, against) {
            return visible;
        }
        let Some(host) = root.parent() else {
            return visible;
        };
        visible = host;
    }
    visible
}

pub fn related_target<'a>(target: Node<'_>, related: Node<'a>) -> Option<Node<'a>> {
    let document = owner_document(target);
    let mut node = related;
    for _ in 0..MAX_DEPTH {
        let own = owner_document(node);
        if own == document {
            return Some(retarget(node, target));
        }
        node = own?.parent()?;
    }
    None
}

pub fn in_shadow_tree(node: Node<'_>) -> bool {
    shadow_root_of(node).is_some()
}
