//! Southstar — struct ns_node, the borrowed node handle over it and the dom.h calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

const NODE_DOCUMENT: c_uint = 0;
const NODE_DOCTYPE: c_uint = 1;
const NODE_ELEMENT: c_uint = 2;
const NODE_TEXT: c_uint = 3;
const NODE_COMMENT: c_uint = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Document,
    Doctype,
    Element,
    Text,
    Comment,
    Other,
}

#[repr(C)]
pub struct NsAttr {
    name: *const c_char,
    value: *const c_char,
    _namespace_uri: *const c_char,
    _prefix: *const c_char,
    _local_name: *const c_char,
    next: *const NsAttr,
    _value_len: c_uint,
    _flags: u8,
}

#[repr(C)]
pub struct NsNode {
    kind: c_uint,
    name: *const c_char,
    text: *const c_char,
    text_len: u32,
    attrs: *const NsAttr,
    parent: *const NsNode,
    first_child: *const NsNode,
    last_child: *const NsNode,
    _prev_sibling: *const NsNode,
    next_sibling: *const NsNode,
    _js_wrapper: *mut c_void,
    _js_invalidate: *mut c_void,
    _backing: *mut c_void,
    _backing_free: *mut c_void,
    _id_index: *mut c_void,
    _class_index: *mut c_void,
    _tag_index: *mut c_void,
    _class_set: *mut c_void,
    _attr_bloom: u64,
    _attr_gen: u32,
    flags: u32,
    src_line: c_int,
    src_col: c_int,
    tpl_content: *mut NsNode,
}

mod controls;
mod select;
mod serialize;
mod tree;

unsafe extern "C" {
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_node_find_by_id(root: *const NsNode, id: *const c_char) -> *mut NsNode;
    fn ns_node_new_text(text: *mut c_char) -> *mut NsNode;
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_node_remove(child: *mut NsNode);
    fn ns_node_free(node: *mut NsNode);
    fn ns_doc_id_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode);
    fn ns_doc_class_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode);
    fn ns_doc_tag_index_subtree_removed(doc: *mut NsNode, root: *mut NsNode);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Node<'a> {
    node: NonNull<NsNode>,
    tree: PhantomData<&'a NsNode>,
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

impl<'a> Node<'a> {
    pub unsafe fn from_ptr(node: *const NsNode) -> Option<Self> {
        NonNull::new(node.cast_mut()).map(|node| Node {
            node,
            tree: PhantomData,
        })
    }

    pub fn as_ptr(self) -> *const NsNode {
        self.node.as_ptr()
    }

    pub fn ptr_or_null(node: Option<Self>) -> *const NsNode {
        node.map_or(core::ptr::null(), Node::as_ptr)
    }

    fn get(self) -> &'a NsNode {
        unsafe { self.node.as_ref() }
    }

    fn link(link: *const NsNode) -> Option<Self> {
        unsafe { Self::from_ptr(link) }
    }

    pub fn is_element(self) -> bool {
        self.get().kind == NODE_ELEMENT
    }

    pub fn is_text(self) -> bool {
        self.get().kind == NODE_TEXT
    }

    pub fn element_name(self) -> Option<&'a [u8]> {
        let node = self.get();
        if node.kind == NODE_ELEMENT {
            c_str(node.name).map(CStr::to_bytes)
        } else {
            None
        }
    }

    pub fn name(self) -> Option<&'a CStr> {
        c_str(self.get().name)
    }

    pub fn text(self) -> Option<&'a CStr> {
        c_str(self.get().text)
    }

    pub fn parent(self) -> Option<Self> {
        Self::link(self.get().parent)
    }

    pub fn first_child(self) -> Option<Self> {
        Self::link(self.get().first_child)
    }

    pub fn next_sibling(self) -> Option<Self> {
        Self::link(self.get().next_sibling)
    }

    pub fn attr(self, name: &CStr) -> Option<&'a CStr> {
        c_str(unsafe { ns_element_get_attr(self.as_ptr(), name.as_ptr()) })
    }

    pub fn flags(self) -> u32 {
        self.get().flags
    }

    pub fn tpl_content(self) -> Option<Self> {
        Self::link(self.get().tpl_content)
    }

    pub fn add_flags(self, flags: u32) {
        unsafe { (*self.node.as_ptr()).flags |= flags };
    }

    pub fn set_source_position(self, line: c_int, col: c_int) {
        unsafe {
            (*self.node.as_ptr()).src_line = line;
            (*self.node.as_ptr()).src_col = col;
        }
    }

    pub fn take_tpl_content(self) -> Option<Self> {
        let content = self.tpl_content();
        unsafe { (*self.node.as_ptr()).tpl_content = core::ptr::null_mut() };
        content
    }

    pub fn as_mut_ptr(self) -> *mut NsNode {
        self.node.as_ptr()
    }

    pub fn kind(self) -> Kind {
        match self.get().kind {
            NODE_DOCUMENT => Kind::Document,
            NODE_DOCTYPE => Kind::Doctype,
            NODE_ELEMENT => Kind::Element,
            NODE_TEXT => Kind::Text,
            NODE_COMMENT => Kind::Comment,
            _ => Kind::Other,
        }
    }

    pub fn last_child(self) -> Option<Self> {
        Self::link(self.get().last_child)
    }

    pub fn text_len(self) -> u32 {
        self.get().text_len
    }

    pub fn text_with_len(self) -> Option<&'a [u8]> {
        let node = self.get();
        (!node.text.is_null()).then(|| unsafe {
            core::slice::from_raw_parts(node.text.cast::<u8>(), node.text_len as usize)
        })
    }

    pub fn attrs(self) -> impl Iterator<Item = Attr<'a>> {
        core::iter::successors(Attr::link(self.get().attrs), |attr| {
            Attr::link(attr.get().next)
        })
    }

    pub fn root(self) -> Self {
        crate::tree::root(self)
    }
}

#[derive(Clone, Copy)]
pub struct Attr<'a> {
    attr: NonNull<NsAttr>,
    node: PhantomData<&'a NsAttr>,
}

impl<'a> Attr<'a> {
    fn link(attr: *const NsAttr) -> Option<Self> {
        NonNull::new(attr.cast_mut()).map(|attr| Attr {
            attr,
            node: PhantomData,
        })
    }

    fn get(self) -> &'a NsAttr {
        unsafe { self.attr.as_ref() }
    }

    pub fn name(self) -> Option<&'a CStr> {
        c_str(self.get().name)
    }

    pub fn value(self) -> Option<&'a CStr> {
        c_str(self.get().value)
    }
}

pub fn set_attr(node: Node, name: &CStr, value: &CStr) {
    unsafe { ns_element_set_attr(node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub fn remove_attr(node: Node, name: &CStr) {
    unsafe { ns_element_remove_attr(node.as_mut_ptr(), name.as_ptr()) };
}

pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_find_by_id(root.as_ptr(), id.as_ptr())) }
}

pub fn detach(node: Node) {
    unsafe { ns_node_remove(node.as_mut_ptr()) };
}

pub fn free(node: Node) {
    unsafe { ns_node_free(node.as_mut_ptr()) };
}

pub fn append_text(parent: Node, text: &CStr) {
    unsafe {
        let child = ns_node_new_text(southstar_glib::g_strdup(text.as_ptr()));
        ns_node_append_child(parent.as_mut_ptr(), child);
    }
}

pub fn index_subtree_removed(doc: Node, root: Node) {
    unsafe {
        ns_doc_id_index_subtree_removed(doc.as_mut_ptr(), root.as_mut_ptr());
        ns_doc_class_index_subtree_removed(doc.as_mut_ptr(), root.as_mut_ptr());
        ns_doc_tag_index_subtree_removed(doc.as_mut_ptr(), root.as_mut_ptr());
    }
}
