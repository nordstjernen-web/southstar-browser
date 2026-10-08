//! Southstar — tree roots and containment, the active modal and inertness, and details opened for a fragment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::ffi::{Node, NsNode};
use crate::{MAX_DEPTH, ancestors_and_self, children};

static ACTIVE_MODAL: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());

pub fn root(node: Node<'_>) -> Node<'_> {
    let mut root = node;
    let mut depth = 0;
    while let Some(parent) = root.parent() {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;
        root = parent;
    }
    root
}

pub fn contains(ancestor: *const NsNode, node: Node) -> bool {
    ancestors_and_self(node).any(|n| ptr::eq(n.as_ptr(), ancestor))
}

pub fn set_active_modal(modal: *const NsNode) {
    ACTIVE_MODAL.store(modal.cast_mut(), Ordering::Relaxed);
}

pub fn active_modal() -> *const NsNode {
    ACTIVE_MODAL.load(Ordering::Relaxed)
}

pub fn effectively_inert(el: Node) -> bool {
    if !el.is_element() {
        return false;
    }
    if ancestors_and_self(el).any(|n| n.is_element() && n.attr(c"inert").is_some()) {
        return true;
    }
    let modal = active_modal();
    !modal.is_null() && !ptr::eq(el.as_ptr(), modal) && !contains(modal, el)
}

pub fn hidden_until_found(el: Node) -> bool {
    el.is_element()
        && el
            .attr(c"hidden")
            .is_some_and(|h| h.to_bytes().eq_ignore_ascii_case(b"until-found"))
}

pub fn details_fragment_needs_open(details: Node, target: Node) -> bool {
    if details.element_name() != Some(b"details")
        || target == details
        || details.attr(c"open").is_some()
    {
        return false;
    }
    let mut child = Some(target);
    let mut steps = 0;
    while let Some(c) = child {
        if c.parent() == Some(details) || steps >= MAX_DEPTH {
            break;
        }
        steps += 1;
        child = c.parent();
    }
    let Some(child) = child.filter(|c| c.parent() == Some(details)) else {
        return false;
    };
    children(details)
        .find(|c| c.element_name() == Some(b"summary"))
        .is_none_or(|summary| summary != child)
}
