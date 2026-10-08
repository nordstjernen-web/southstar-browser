//! Southstar — the leading fields of struct ns_node and the dom.h readers behind the borrowed node handle.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_glib::GStr;

const NODE_ELEMENT: c_uint = 2;
const NODE_TEXT: c_uint = 3;

#[repr(C)]
pub struct NsNode {
    kind: c_uint,
    name: *const c_char,
    text: *const c_char,
    _text_len: u32,
    _attrs: *const c_void,
    parent: *const NsNode,
    first_child: *const NsNode,
    _last_child: *const NsNode,
    _prev_sibling: *const NsNode,
    next_sibling: *const NsNode,
}

unsafe extern "C" {
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_node_collect_text(root: *const NsNode) -> *mut c_char;
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

    pub fn collect_text(self) -> Option<GStr> {
        unsafe { GStr::take(ns_node_collect_text(self.as_ptr())) }
    }
}
