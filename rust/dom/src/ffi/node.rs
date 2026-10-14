//! Southstar — the C ABI of node creation, naming, flags, linking, freeing and template contents.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{GBoolean, GStr};

use super::node;
use crate::NsNode;
use crate::{Kind, Node};

fn taken_bytes(s: *mut c_char) -> Vec<u8> {
    // SAFETY: the C callers hand over null or a NUL-terminated string from g_malloc, which `GStr` frees.
    unsafe { GStr::take(s) }.map_or_else(Vec::new, |s| s.to_bytes().to_vec())
}

fn new_node(kind: Kind, text: Option<&[u8]>) -> *mut NsNode {
    let node = Node::alloc(kind);
    if let Some(text) = text {
        node.set_text(text);
    }
    node.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_document() -> *mut NsNode {
    new_node(Kind::Document, None)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_element(name: *mut c_char) -> *mut NsNode {
    let element = crate::node::new_element(&taken_bytes(name));
    element.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_text(text: *mut c_char) -> *mut NsNode {
    new_node(Kind::Text, Some(&taken_bytes(text)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_comment(text: *mut c_char) -> *mut NsNode {
    new_node(Kind::Comment, Some(&taken_bytes(text)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_set_name_owned(n: *mut NsNode, name: *mut c_char) {
    let name = taken_bytes(name);
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(n) = unsafe { node(n) } {
        n.set_name(&name);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_add_flags(n: *mut NsNode, flags: u32) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(n) = unsafe { node(n) } {
        n.add_flags(flags);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_mark_doctype(n: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(n) = unsafe { node(n) } {
        n.set_kind(Kind::Doctype);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_free(n: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(n) = unsafe { node(n) } {
        n.free_tree();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode) {
    // SAFETY: the C callers pass null or nodes of this thread's store.
    if let (Some(parent), Some(child)) = (unsafe { node(parent) }, unsafe { node(child) }) {
        parent.append(child);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_remove(child: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(child) = unsafe { node(child) } {
        child.detach();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_next_in_subtree(
    n: *const NsNode,
    root: *const NsNode,
    descend: GBoolean,
) -> *mut NsNode {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let Some(n) = (unsafe { node(n) }) else {
        return ptr::null_mut();
    };
    // SAFETY: as above, for the subtree root.
    let root = unsafe { node(root) };
    crate::index::next_in_subtree(n, root, descend != 0).map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_template_content_get(tpl: *mut NsNode) -> *mut NsNode {
    // SAFETY: the C callers pass null or a node of this thread's store.
    unsafe { node(tpl) }.map_or(ptr::null_mut(), |tpl| {
        crate::node::template_content(tpl).as_mut_ptr()
    })
}
