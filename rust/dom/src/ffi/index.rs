//! Southstar — the C ABI of the document indexes, document order and the tag, id and fragment lookups declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;

use southstar_glib::GPtrArray;

use super::{Node, NsNode, c_str};
use crate::index;

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn mut_ptr(found: Option<Node>) -> *mut NsNode {
    found.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

unsafe fn with_doc_and_root(doc: *mut NsNode, root: *mut NsNode, f: fn(Node, Node)) {
    if let (Some(doc), Some(root)) = (unsafe { node(doc) }, unsafe { node(root) }) {
        f(doc, root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_find_first_element(
    root: *const NsNode,
    tag: *const c_char,
) -> *mut NsNode {
    let (Some(root), Some(tag)) = (unsafe { node(root) }, c_str(tag)) else {
        return ptr::null_mut();
    };
    mut_ptr(index::find_first_element(root, tag))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_find_by_id(root: *const NsNode, id: *const c_char) -> *mut NsNode {
    let (Some(root), Some(id)) = (unsafe { node(root) }, c_str(id)) else {
        return ptr::null_mut();
    };
    mut_ptr(index::find_by_id(root, id))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_find_fragment_target(
    root: *const NsNode,
    frag: *const c_char,
) -> *mut NsNode {
    let (Some(root), Some(frag)) = (unsafe { node(root) }, c_str(frag)) else {
        return ptr::null_mut();
    };
    mut_ptr(index::find_fragment_target(root, frag))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_document_order_cmp(a: *const NsNode, b: *const NsNode) -> c_int {
    index::document_order_cmp(unsafe { node(a) }, unsafe { node(b) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_build(doc: *mut NsNode) {
    if let Some(doc) = unsafe { node(doc) } {
        index::id_build(doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_register(
    doc: *mut NsNode,
    id: *const c_char,
    n: *mut NsNode,
) {
    if let (Some(doc), Some(id), Some(n)) = (unsafe { node(doc) }, c_str(id), unsafe { node(n) }) {
        index::id_register(doc, id, n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_unregister(
    doc: *mut NsNode,
    id: *const c_char,
    n: *const NsNode,
) {
    if let (Some(doc), Some(id)) = (unsafe { node(doc) }, c_str(id)) {
        index::id_unregister(doc, id, unsafe { node(n) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_subtree_added(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::id_subtree_added) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::id_subtree_removed) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_build(doc: *mut NsNode) {
    if let Some(doc) = unsafe { node(doc) } {
        index::class_build(doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_register(
    doc: *mut NsNode,
    class_attr: *const c_char,
    n: *mut NsNode,
) {
    if let (Some(doc), Some(class_attr), Some(n)) =
        (unsafe { node(doc) }, c_str(class_attr), unsafe { node(n) })
    {
        index::class_register(doc, class_attr, n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_unregister(
    doc: *mut NsNode,
    class_attr: *const c_char,
    n: *mut NsNode,
) {
    if let (Some(doc), Some(class_attr), Some(n)) =
        (unsafe { node(doc) }, c_str(class_attr), unsafe { node(n) })
    {
        index::class_unregister(doc, class_attr, n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_subtree_added(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::class_subtree_added) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::class_subtree_removed) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_lookup(
    doc: *const NsNode,
    cls: *const c_char,
) -> *mut GPtrArray {
    let (Some(doc), Some(cls)) = (unsafe { node(doc) }, c_str(cls)) else {
        return ptr::null_mut();
    };
    index::class_lookup(doc, cls, |nodes| nodes.as_ptr()).unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_tag_index_build(doc: *mut NsNode) {
    if let Some(doc) = unsafe { node(doc) } {
        index::tag_build(doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_tag_index_subtree_added(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::tag_subtree_added) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_tag_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode) {
    unsafe { with_doc_and_root(doc, root, index::tag_subtree_removed) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_tag_index_lookup(
    doc: *const NsNode,
    tag: *const c_char,
) -> *mut GPtrArray {
    let (Some(doc), Some(tag)) = (unsafe { node(doc) }, c_str(tag)) else {
        return ptr::null_mut();
    };
    index::tag_lookup(doc, tag, |nodes| nodes.as_ptr()).unwrap_or(ptr::null_mut())
}
