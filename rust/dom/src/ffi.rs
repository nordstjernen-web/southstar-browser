//! Southstar — struct ns_node, the borrowed node handle over it and the dom.h calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_glib::GHashTable;

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
    name: *mut c_char,
    value: *mut c_char,
    namespace_uri: *mut c_char,
    prefix: *mut c_char,
    local_name: *mut c_char,
    next: *mut NsAttr,
    value_len: c_uint,
    flags: u8,
}

type Invalidator = Option<unsafe extern "C" fn(node: *mut NsNode)>;
type BackingFree = Option<unsafe extern "C" fn(backing: *mut c_void)>;

#[repr(C)]
pub struct NsNode {
    kind: c_uint,
    name: *mut c_char,
    text: *mut c_char,
    text_len: u32,
    attrs: *mut NsAttr,
    parent: *mut NsNode,
    first_child: *mut NsNode,
    last_child: *mut NsNode,
    prev_sibling: *mut NsNode,
    next_sibling: *mut NsNode,
    _js_wrapper: *mut c_void,
    js_invalidate: Invalidator,
    backing: *mut c_void,
    backing_free: BackingFree,
    id_index: *mut GHashTable,
    class_index: *mut GHashTable,
    tag_index: *mut GHashTable,
    class_set: *mut c_void,
    attr_bloom: u64,
    attr_gen: u32,
    flags: u32,
    src_line: c_int,
    src_col: c_int,
    tpl_content: *mut NsNode,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<NsNode>() == 176 && size_of::<NsAttr>() == 56);

mod attrs;
mod controls;
mod index;
mod memory;
mod node;
mod select;
mod serialize;
mod tables;
mod tree;

pub use memory::{
    ATTR_NAME_LOWER, ATTR_OWN_NAME, ATTR_OWN_VALUE, NewAttr, dup, dup_bytes, dup_with_len,
    return_if_fail, value_dup,
};
pub use tables::{BucketTable, IdTable, NodeArray, NodeSet};

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

    pub fn prev_sibling(self) -> Option<Self> {
        Self::link(self.get().prev_sibling)
    }

    pub fn next_sibling(self) -> Option<Self> {
        Self::link(self.get().next_sibling)
    }

    pub fn attr(self, name: &CStr) -> Option<&'a CStr> {
        crate::attrs::get(self, name)
    }

    pub fn kind_raw(self) -> c_uint {
        self.get().kind
    }

    pub fn flags(self) -> u32 {
        self.get().flags
    }

    pub fn attr_gen(self) -> u32 {
        self.get().attr_gen
    }

    pub fn tpl_content(self) -> Option<Self> {
        Self::link(self.get().tpl_content)
    }

    pub fn add_flags(self, flags: u32) {
        unsafe { (*self.node.as_ptr()).flags |= flags };
    }

    pub fn remove_flags(self, flags: u32) {
        unsafe { (*self.node.as_ptr()).flags &= !flags };
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
    pub unsafe fn from_ptr(attr: *const NsAttr) -> Option<Self> {
        Self::link(attr)
    }

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

    pub fn value_ptr(self) -> *const c_char {
        self.get().value
    }

    pub fn value_len(self) -> c_uint {
        self.get().value_len
    }

    pub fn namespace_uri(self) -> Option<&'a CStr> {
        c_str(self.get().namespace_uri)
    }

    pub fn prefix(self) -> Option<&'a CStr> {
        c_str(self.get().prefix)
    }

    pub fn local_name(self) -> Option<&'a CStr> {
        c_str(self.get().local_name)
    }

    pub fn flags(self) -> u8 {
        self.get().flags
    }

    pub fn name_first_byte(self) -> Option<u8> {
        let name = self.get().name;
        (!name.is_null()).then(|| unsafe { *name.cast::<u8>() })
    }
}

pub fn set_attr(node: Node, name: &CStr, value: &CStr) {
    unsafe { attrs::ns_element_set_attr(node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub fn remove_attr(node: Node, name: &CStr) {
    unsafe { attrs::ns_element_remove_attr(node.as_mut_ptr(), name.as_ptr()) };
}

pub fn detach(node: Node) {
    node.detach();
}

pub fn free(node: Node) {
    node.free_tree();
}

pub fn append_text(parent: Node, text: &CStr) {
    let child = crate::node::new_text(
        crate::node::KIND_TEXT,
        dup(text),
        text.to_bytes().len() as u32,
    );
    parent.append(child);
}
