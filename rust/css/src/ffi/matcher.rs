//! Southstar — the C ABI of selector matching: a selector or a rule's selector tested against an element, the hover, active, focus and fullscreen nodes and the match scope the matcher reads, nth-position batches, the :has() memo and the defined custom elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_void};
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

fn store(slot: &'static Slot, node: Option<Node<'_>>) -> *const NsNode {
    let previous = load(slot);
    slot.set(node.map(Node::downgrade));
    Node::ptr_or_null(previous)
}

fn node_at<'a>(node: *const NsNode) -> Option<Node<'a>> {
    // SAFETY: the C callers pass null or a node of this thread's store.
    unsafe { Node::from_ptr(node) }
}

pub(crate) fn match_scope<'a>() -> Option<Node<'a>> {
    load(&MATCH_SCOPE)
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
pub extern "C" fn ns_css_set_match_scope(scope: *const NsNode) -> *const NsNode {
    store(&MATCH_SCOPE, node_at(scope))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_focus_node(node: *const NsNode) -> *const NsNode {
    store(&FOCUS, node_at(node))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_focus_visible_node(node: *const NsNode) {
    store(&FOCUS_VISIBLE, node_at(node));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_hover_node(node: *const NsNode) -> *const NsNode {
    store(&HOVER, node_at(node))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_active_node(node: *const NsNode) -> *const NsNode {
    store(&ACTIVE, node_at(node))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_fullscreen_node(node: *const NsNode) -> *const NsNode {
    store(&FULLSCREEN, node_at(node))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_focus_node() -> *const NsNode {
    Node::ptr_or_null(focus_node())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_hover_node() -> *const NsNode {
    Node::ptr_or_null(hover_node())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_active_node() -> *const NsNode {
    Node::ptr_or_null(active_node())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_fullscreen_node() -> *const NsNode {
    Node::ptr_or_null(fullscreen_node())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_selector_batch_begin() {
    matcher::batch_begin();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_selector_batch_end() {
    matcher::batch_end();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_has_memo_begin() {
    matcher::has_memo_begin();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_has_memo_end() {
    matcher::has_memo_end();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_register_defined_element(tag: *const c_char) {
    if !tag.is_null() {
        matcher::register_defined_element(unsafe { CStr::from_ptr(tag) }.to_bytes());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_clear_defined_elements() {
    matcher::clear_defined_elements();
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
