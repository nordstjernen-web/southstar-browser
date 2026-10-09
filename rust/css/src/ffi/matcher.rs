//! Southstar — the C ABI of selector matching: a selector or a rule's selector tested against an element, the hover, active, focus and fullscreen nodes and the match scope the matcher reads, nth-position batches, the :has() memo and the defined custom elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use super::selector_view::{RuleRef, SelectorRef};
use crate::matcher;

static MATCH_SCOPE: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static FOCUS: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static FOCUS_VISIBLE: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static HOVER: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static ACTIVE: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static FULLSCREEN: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());

fn load<'a>(slot: &AtomicPtr<NsNode>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(slot.load(Ordering::Relaxed)) }
}

fn swap(slot: &AtomicPtr<NsNode>, node: *const NsNode) -> *const NsNode {
    slot.swap(node.cast_mut(), Ordering::Relaxed)
}

pub(crate) fn match_scope<'a>() -> Option<Node<'a>> {
    load(&MATCH_SCOPE)
}

pub(crate) fn set_match_scope(scope: Option<Node<'_>>) {
    MATCH_SCOPE.store(Node::ptr_or_null(scope).cast_mut(), Ordering::Relaxed);
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
    swap(&MATCH_SCOPE, scope)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_match_scope() -> *const NsNode {
    MATCH_SCOPE.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_focus_node(node: *const NsNode) -> *const NsNode {
    swap(&FOCUS, node)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_focus_visible_node(node: *const NsNode) {
    swap(&FOCUS_VISIBLE, node);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_hover_node(node: *const NsNode) -> *const NsNode {
    swap(&HOVER, node)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_active_node(node: *const NsNode) -> *const NsNode {
    swap(&ACTIVE, node)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_fullscreen_node(node: *const NsNode) -> *const NsNode {
    swap(&FULLSCREEN, node)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_focus_node() -> *const NsNode {
    FOCUS.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_hover_node() -> *const NsNode {
    HOVER.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_active_node() -> *const NsNode {
    ACTIVE.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_fullscreen_node() -> *const NsNode {
    FULLSCREEN.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_forget_node(node: *const NsNode) {
    for slot in [&FOCUS, &FOCUS_VISIBLE, &HOVER, &ACTIVE, &FULLSCREEN] {
        let _ = slot.compare_exchange(
            node.cast_mut(),
            ptr::null_mut(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_rule_selector_matches(
    rule: *const c_void,
    sel: *const c_void,
    el: *const NsNode,
    pe: c_uint,
    scope_order: *mut c_int,
) -> GBoolean {
    let order = unsafe { Node::from_ptr(el) }.and_then(|el| {
        matcher::rule_matches(
            unsafe { RuleRef::from_ptr(rule) },
            unsafe { SelectorRef::from_ptr(sel) },
            el,
            pe,
        )
    });
    if !scope_order.is_null() {
        unsafe { *scope_order = order.unwrap_or(0) };
    }
    glib::boolean(order.is_some())
}
