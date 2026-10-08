//! Southstar — node creation, names and text, string ownership, template contents and cloning.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};

use crate::attrs::{self, NsName};
use crate::ffi::{Kind, Node, dup, dup_bytes, dup_with_len};
use crate::serialize::is_embedded_doc;
use crate::{MAX_DEPTH, children};

pub const KIND_DOCUMENT: c_uint = 0;
pub const KIND_DOCTYPE: c_uint = 1;
pub const KIND_ELEMENT: c_uint = 2;
pub const KIND_TEXT: c_uint = 3;
pub const KIND_COMMENT: c_uint = 4;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;
const FLAG_SVG_NS: u32 = 1 << 7;
const FLAG_FOREIGN_NS: u32 = 1 << 9;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;
const FLAG_KEEP_CASE: u32 = 1 << 12;
const FLAG_INPUT_INDETERMINATE: u32 = 1 << 18;

pub fn new_element(name: *mut core::ffi::c_char) -> Node<'static> {
    let node = Node::alloc(KIND_ELEMENT);
    node.adopt_name(name);
    node
}

pub fn new_text(kind: c_uint, text: *mut core::ffi::c_char, len: u32) -> Node<'static> {
    let node = Node::alloc(kind);
    node.adopt_text(text, len);
    node
}

pub fn own_strings_deep(root: Node) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        cur.clear_class_set();
        cur.own_node_strings();
        if let Some(content) = cur.tpl_content() {
            stack.push(content);
        }
        stack.extend(children(cur));
    }
}

pub fn template_content(tpl: Node) -> Node {
    if let Some(content) = tpl.tpl_content() {
        return content;
    }
    let content = Node::alloc(KIND_DOCUMENT);
    content.add_flags(FLAG_FRAGMENT | FLAG_TEMPLATE_CONTENT);
    tpl.set_tpl_content(Some(content));
    content
}

fn name_or_empty(src: Node) -> *mut core::ffi::c_char {
    dup(src.name().unwrap_or(c""))
}

fn copy_attrs(src: Node, out: Node, function: &CStr) {
    for attr in src.attrs() {
        let qname = NsName {
            namespace_uri: attr.namespace_uri(),
            prefix: attr.prefix(),
            local_name: attrs::local_name(attr),
            name: attr.name(),
        };
        if out.is_element() {
            attrs::set_ns(out, qname, Some(attr.value().unwrap_or(c"")));
        } else {
            crate::ffi::return_if_fail(function, c"el->kind == NS_NODE_ELEMENT");
        }
    }
}

fn clone_shallow(src: Node) -> Option<Node<'static>> {
    let out = match src.kind() {
        Kind::Element => {
            let out = new_element(name_or_empty(src));
            out.add_flags(
                src.flags()
                    & (FLAG_SVG_NS | FLAG_FOREIGN_NS | FLAG_KEEP_CASE | FLAG_INPUT_INDETERMINATE),
            );
            copy_attrs(src, out, c"ns_element_set_attr_ns");
            out
        }
        Kind::Text if src.text_ptr().is_null() => {
            new_text(KIND_TEXT, dup_bytes(b""), src.text_len())
        }
        Kind::Text => new_text(
            KIND_TEXT,
            dup_with_len(src.text_ptr(), src.text_len()),
            src.text_len(),
        ),
        Kind::Doctype => {
            let out = new_element(name_or_empty(src));
            copy_attrs(src, out, c"ns_element_set_attr_ns");
            out.set_kind(KIND_DOCTYPE);
            out
        }
        Kind::Document | Kind::Comment => {
            let out = Node::alloc(if src.kind() == Kind::Document {
                KIND_DOCUMENT
            } else {
                KIND_COMMENT
            });
            if !src.text_ptr().is_null() {
                out.adopt_text(dup_with_len(src.text_ptr(), src.text_len()), src.text_len());
            }
            if let Some(name) = src.name() {
                out.adopt_name(dup(name));
            }
            copy_attrs(src, out, c"ns_element_set_attr_ns");
            out
        }
        Kind::Other => return None,
    };
    out.add_flags(src.flags() & (FLAG_FRAGMENT | FLAG_CDATA | FLAG_PI));
    Some(out)
}

pub fn clone(src: Node, deep: bool, depth: i32) -> Option<Node<'static>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    let out = clone_shallow(src)?;
    if deep {
        for child in children(src).filter(|c| !is_embedded_doc(*c)) {
            if let Some(copy) = clone(child, true, depth + 1) {
                out.append(copy);
            }
        }
        if let Some(content) = src.tpl_content() {
            out.set_tpl_content(clone(content, true, depth + 1));
        }
    }
    Some(out)
}
