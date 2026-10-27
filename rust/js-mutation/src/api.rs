//! Southstar — the Rust API other binding crates record DOM changes, edit attributes and manage orphans through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;

use crate::ffi::{self, Js, NsJs, Page};
use crate::{Element, attribute_maps, attrs, change, orphans, record, tree};

pub use crate::attrs::QualifiedName;

pub fn index_child_change(
    js: *mut NsJs,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
) {
    record::index_child_changes(Js::of(js), parent, added.as_slice(), removed.as_slice());
}

pub fn record_child_change(
    js: *mut NsJs,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    index_child_change(js, parent, added, removed);
    ffi::mark_childlist_dirty(parent, added);
    ffi::emit_child(Js::of(js), parent, added, removed, previous, next);
}

pub fn record_child_changes(
    js: *mut NsJs,
    parent: Element,
    added: &[Element],
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    record::index_child_changes(Js::of(js), parent, added, removed);
    ffi::mark_childlist_dirty(parent, added.first().copied());
    ffi::emit_child_list(Js::of(js), parent, Some(added), removed, previous, next);
}

pub fn record_attr_change(js: *mut NsJs, target: Element, name: &CStr, old: Option<&CStr>) {
    record::attribute_changed(Js::of(js), Some(target), Some(name), None, old);
}

pub fn record_character_data(js: *mut NsJs, target: Element, old: &[u8]) {
    let old = attrs::c_copy(old);
    record::character_data_changed(Js::of(js), Some(target), Some(&old));
}

pub fn set_attr_recorded(js: *mut NsJs, node: Element, name: &CStr, value: &[u8]) {
    attrs::set_recorded(Js::of(js), node, name, value);
}

pub fn set_attr_ns_recorded(js: *mut NsJs, node: Element, qname: &QualifiedName, value: &CStr) {
    attrs::set_ns_recorded(Js::of(js), node, qname, value);
}

pub fn remove_attr_recorded(js: *mut NsJs, node: Element, name: &CStr) {
    attrs::remove_recorded(Js::of(js), node, name);
}

pub fn remove_attr_ns_recorded(
    js: *mut NsJs,
    node: Element,
    namespace_uri: Option<&CStr>,
    local_name: &CStr,
) {
    attrs::remove_ns_recorded(Js::of(js), node, namespace_uri, local_name);
}

pub fn track_orphan(js: *mut NsJs, node: Node<'_>) {
    if let Some(js) = Js::of(js) {
        orphans::track(js, node);
    }
}

pub fn unorphan_node(js: *mut NsJs, node: Element) {
    if let Some(js) = Js::of(js) {
        orphans::untrack(js, node);
    }
}

pub fn orphan_node(js: *mut NsJs, node: Element) {
    orphans::orphan(Js::of(js), node);
}

pub fn clear_children(js: *mut NsJs, node: Element) {
    orphans::clear_children(Js::of(js), node);
}

pub fn orphan_children(js: *mut NsJs, node: Element) {
    orphans::orphan_children(Js::of(js), node);
}

pub fn replace_all_recorded(js: *mut NsJs, node: Element, added: Option<Element>) {
    orphans::replace_all(Js::of(js), node, added);
}

pub fn orphans_take_all(js: *mut NsJs) -> Vec<Element> {
    Js::of(js).map(orphans::take_all).unwrap_or_default()
}

pub fn mark_scripts_already_started(root: Element) {
    tree::mark_scripts_already_started(root, 0);
}

pub fn forget_pending_change(js: *mut NsJs) {
    if let Some(js) = Js::of(js) {
        change::forget(js);
    }
}

pub fn note_user_edit(js: Page, el: Node<'_>, value_before: &CStr) {
    if let Some(el) = el.downgrade().get() {
        change::note_user_edit(Js::of_page(js), el, value_before);
    }
}

pub fn commit_change(js: Page, el: Node<'_>) {
    if let Some(el) = el.downgrade().get() {
        change::commit(Js::of_page(js), el);
    }
}

pub fn attribute_maps_release_all(js: *mut NsJs) {
    if let Some(js) = Js::of(js) {
        attribute_maps::release_all(js);
    }
}
