//! Southstar — the C ABI of node creation, naming, text, ownership, linking, freeing and cloning declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};
use core::ptr;

use southstar_glib::GBoolean;

use super::memory::{c_strlen, free_string, return_if_fail};
use super::{BackingFree, Node, NsNode};
use crate::node::{self, KIND_COMMENT, KIND_DOCTYPE, KIND_DOCUMENT, KIND_TEXT};

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_document() -> *mut NsNode {
    Node::alloc(KIND_DOCUMENT).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_element(name: *mut c_char) -> *mut NsNode {
    node::new_element(name).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_text_len(text: *mut c_char, len: u32) -> *mut NsNode {
    node::new_text(KIND_TEXT, text, len).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_text(text: *mut c_char) -> *mut NsNode {
    node::new_text(KIND_TEXT, text, c_strlen(text)).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_comment_len(text: *mut c_char, len: u32) -> *mut NsNode {
    node::new_text(KIND_COMMENT, text, len).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_node_new_comment(text: *mut c_char) -> *mut NsNode {
    node::new_text(KIND_COMMENT, text, c_strlen(text)).as_mut_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_set_name_borrow(n: *mut NsNode, name: *const c_char) {
    if let Some(n) = unsafe { node(n) } {
        n.set_name(name.cast_mut(), false);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_set_name_owned(n: *mut NsNode, name: *mut c_char) {
    match unsafe { node(n) } {
        Some(n) => n.set_name(name, true),
        None => free_string(name),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_add_flags(n: *mut NsNode, flags: u32) {
    if let Some(n) = unsafe { node(n) } {
        n.add_flags(flags);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_mark_doctype(n: *mut NsNode) {
    if let Some(n) = unsafe { node(n) } {
        n.set_kind(KIND_DOCTYPE);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_set_text_borrow(n: *mut NsNode, text: *const c_char) {
    if let Some(n) = unsafe { node(n) } {
        n.set_text(text.cast_mut(), c_strlen(text), false);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_replace_text_len_owned(
    n: *mut NsNode,
    text: *mut c_char,
    len: u32,
) {
    match unsafe { node(n) } {
        Some(n) => n.set_text(text, len, true),
        None => free_string(text),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_replace_text_owned(n: *mut NsNode, text: *mut c_char) {
    unsafe { ns_node_replace_text_len_owned(n, text, c_strlen(text)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_own_strings_deep(n: *mut NsNode) {
    if let Some(n) = unsafe { node(n) } {
        node::own_strings_deep(n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_attach_backing(
    root: *mut NsNode,
    backing: *mut c_void,
    destroy: BackingFree,
) {
    match unsafe { node(root) } {
        Some(root) => root.attach_backing(backing, destroy),
        None => {
            if let (false, Some(destroy)) = (backing.is_null(), destroy) {
                unsafe { destroy(backing) };
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_free(n: *mut NsNode) {
    if let Some(n) = unsafe { node(n) } {
        n.free_tree();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode) {
    let Some(parent) = (unsafe { node(parent) }) else {
        return return_if_fail(c"ns_node_append_child", c"parent != NULL");
    };
    let Some(child) = (unsafe { node(child) }) else {
        return return_if_fail(c"ns_node_append_child", c"child != NULL");
    };
    parent.append(child);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_remove(child: *mut NsNode) {
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
    let Some(n) = (unsafe { node(n) }) else {
        return ptr::null_mut();
    };
    crate::index::next_in_subtree(n, unsafe { node(root) }, descend != 0)
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_clone(src: *const NsNode, deep: GBoolean) -> *mut NsNode {
    unsafe { node(src) }
        .and_then(|src| node::clone(src, deep != 0, 0))
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_template_content_get(tpl: *mut NsNode) -> *mut NsNode {
    unsafe { node(tpl) }.map_or(ptr::null_mut(), |tpl| {
        node::template_content(tpl).as_mut_ptr()
    })
}
