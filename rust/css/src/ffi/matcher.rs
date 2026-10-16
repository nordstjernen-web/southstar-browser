//! Southstar — the C ABI of selector matching: a selector or a rule's selector tested against an element, the hover, active, focus and fullscreen nodes and the match scope the matcher reads, nth-position batches, the :has() memo and the defined custom elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::c_void;
use std::thread::LocalKey;

use southstar_dom::{Node, NsNode, WeakNode};
use southstar_glib::{self as glib, GBoolean};

use super::selector_view::SelectorRef;
use crate::matcher;

thread_local! {
    static MATCH_SCOPE: Cell<Option<WeakNode>> = const { Cell::new(None) };
    static FOCUS: Cell<Option<WeakNode>> = const { Cell::new(None) };
    static FOCUS_VISIBLE: Cell<Option<WeakNode>> = const { Cell::new(None) };
    static HOVER: Cell<Option<WeakNode>> = const { Cell::new(None) };
    static ACTIVE: Cell<Option<WeakNode>> = const { Cell::new(None) };
    static FULLSCREEN: Cell<Option<WeakNode>> = const { Cell::new(None) };
}

type Slot = LocalKey<Cell<Option<WeakNode>>>;

fn load<'a>(slot: &'static Slot) -> Option<Node<'a>> {
    slot.get().and_then(WeakNode::get)
}

fn store<'a>(slot: &'static Slot, node: Option<Node<'_>>) -> Option<Node<'a>> {
    let previous = load(slot);
    slot.set(node.map(Node::downgrade));
    previous
}

pub fn set_focus_node<'a>(node: Option<Node<'_>>) -> Option<Node<'a>> {
    store(&FOCUS, node)
}

pub fn set_focus_visible_node(node: Option<Node<'_>>) {
    store(&FOCUS_VISIBLE, node);
}

pub fn set_hover_node<'a>(node: Option<Node<'_>>) -> Option<Node<'a>> {
    store(&HOVER, node)
}

pub fn set_active_node<'a>(node: Option<Node<'_>>) -> Option<Node<'a>> {
    store(&ACTIVE, node)
}

pub fn set_fullscreen_node<'a>(node: Option<Node<'_>>) -> Option<Node<'a>> {
    store(&FULLSCREEN, node)
}

pub(crate) fn match_scope<'a>() -> Option<Node<'a>> {
    load(&MATCH_SCOPE)
}

pub(crate) fn replace_match_scope(scope: Option<Node<'_>>) -> Option<WeakNode> {
    MATCH_SCOPE.replace(scope.map(Node::downgrade))
}

pub(crate) fn restore_match_scope(previous: Option<WeakNode>) {
    MATCH_SCOPE.set(previous);
}

pub(crate) fn replace_focus_node(node: Option<Node<'_>>) -> Option<WeakNode> {
    FOCUS.replace(node.map(Node::downgrade))
}

pub(crate) fn restore_focus_node(previous: Option<WeakNode>) {
    FOCUS.set(previous);
}

pub(crate) fn set_match_scope(scope: Option<Node<'_>>) {
    store(&MATCH_SCOPE, scope);
}

pub(crate) fn focus_node<'a>() -> Option<Node<'a>> {
    load(&FOCUS)
}

pub(crate) fn focus_visible_node<'a>() -> Option<Node<'a>> {
    load(&FOCUS_VISIBLE)
}

pub(crate) fn hover_node<'a>() -> Option<Node<'a>> {
    load(&HOVER)
}

pub(crate) fn active_node<'a>() -> Option<Node<'a>> {
    load(&ACTIVE)
}

pub(crate) fn fullscreen_node<'a>() -> Option<Node<'a>> {
    load(&FULLSCREEN)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_selector_matches(
    sel: *const c_void,
    el: *const NsNode,
) -> GBoolean {
    let (Some(sel), Some(el)) = (unsafe { SelectorRef::from_ptr(sel) }, unsafe {
        Node::from_ptr(el)
    }) else {
        return glib::FALSE;
    };
    glib::boolean(matcher::matches(sel, el))
}
