//! Southstar — node creation, names and text, string ownership, template contents and cloning.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};

use crate::attrs::{self, NsName};
use crate::serialize::is_embedded_doc;
use crate::{Kind, MAX_DEPTH, Node, Tree, children};

pub const KIND_DOCUMENT: c_uint = 0;
pub const KIND_DOCTYPE: c_uint = 1;
pub const KIND_ELEMENT: c_uint = 2;
pub const KIND_TEXT: c_uint = 3;
pub const KIND_COMMENT: c_uint = 4;

pub(crate) const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;
const FLAG_SVG_NS: u32 = 1 << 7;
const FLAG_FOREIGN_NS: u32 = 1 << 9;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;
const FLAG_KEEP_CASE: u32 = 1 << 12;
const FLAG_INPUT_INDETERMINATE: u32 = 1 << 18;

pub fn new_element(name: &[u8]) -> Node<'static> {
    let node = Node::alloc(Kind::Element);
    node.set_name(name);
    node
}

pub fn new_text(kind: Kind, text: &[u8]) -> Node<'static> {
    let node = Node::alloc(kind);
    node.set_text(text);
    node
}

pub fn template_content(tpl: Node) -> Node {
    if let Some(content) = tpl.tpl_content() {
        return content;
    }
    let content = Node::alloc(Kind::Document);
    content.add_flags(FLAG_FRAGMENT | FLAG_TEMPLATE_CONTENT);
    tpl.set_tpl_content(Some(content));
    content
}

fn name_or_empty(src: Node<'_>) -> &[u8] {
    src.name().map_or(b"", CStr::to_bytes)
}

fn copy_attrs(src: Node, out: Node) {
    if !out.is_element() {
        return;
    }
    for attr in src.attrs() {
        let qname = NsName {
            namespace_uri: attr.namespace_uri(),
            prefix: attr.prefix(),
            local_name: attrs::local_name(attr),
            name: attr.name(),
        };
        attrs::set_ns(out, qname, Some(attr.value().unwrap_or(c"")));
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
            copy_attrs(src, out);
            out
        }
        Kind::Text => new_text(Kind::Text, src.text_with_len().unwrap_or(b"")),
        Kind::Doctype => {
            let out = new_element(name_or_empty(src));
            copy_attrs(src, out);
            out.set_kind(Kind::Doctype);
            out
        }
        Kind::Document | Kind::Comment => {
            let out = Node::alloc(src.kind());
            if let Some(text) = src.text_with_len() {
                out.set_text(text);
            }
            if let Some(name) = src.name() {
                out.set_name(name.to_bytes());
            }
            copy_attrs(src, out);
            out
        }
        Kind::Other => return None,
    };
    out.add_flags(src.flags() & (FLAG_FRAGMENT | FLAG_CDATA | FLAG_PI));
    Some(out)
}

pub fn clone(src: Node, deep: bool, depth: i32) -> Option<Tree> {
    if depth >= MAX_DEPTH {
        return None;
    }
    let out = Tree::adopt(clone_shallow(src)?);
    if deep {
        for child in children(src).filter(|c| !is_embedded_doc(*c)) {
            if let Some(copy) = clone(child, true, depth + 1) {
                out.root().append(copy.into_root());
            }
        }
        if let Some(content) = src.tpl_content() {
            out.root()
                .set_tpl_content(clone(content, true, depth + 1).map(Tree::into_root));
        }
    }
    Some(out)
}
