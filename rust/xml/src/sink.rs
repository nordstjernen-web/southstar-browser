//! Southstar — the XML parser's tree builder over rust/dom, and the parse entry point that hands back an owned Tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::attrs::{self, NsName};
use southstar_dom::{Kind, Node, Tree, node};

use crate::Dom;

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&c| c == 0)
        .map_or(bytes, |nul| &bytes[..nul])
}

fn c_string(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

struct DomBuilder;

impl Dom for DomBuilder {
    type Node = Node<'static>;

    fn document(&mut self) -> Node<'static> {
        Tree::new(Kind::Document).into_root()
    }

    fn element(&mut self, name: Option<&[u8]>) -> Node<'static> {
        node::new_element(until_nul(name.unwrap_or_default()))
    }

    fn text(&mut self, text: &[u8]) -> Node<'static> {
        node::new_text(Kind::Text, until_nul(text))
    }

    fn comment(&mut self, text: &[u8]) -> Node<'static> {
        node::new_text(Kind::Comment, until_nul(text))
    }

    fn set_name(&mut self, node: Node<'static>, name: &[u8]) {
        node.set_name(until_nul(name));
    }

    fn add_flags(&mut self, node: Node<'static>, flags: u32) {
        node.add_flags(flags);
    }

    fn mark_doctype(&mut self, node: Node<'static>) {
        node.set_kind(Kind::Doctype);
    }

    fn set_attr(&mut self, element: Node<'static>, name: &[u8], value: &[u8]) {
        attrs::set_attribute(element, &c_string(name), Some(until_nul(value)));
    }

    fn set_attr_ns(
        &mut self,
        element: Node<'static>,
        namespace: Option<&[u8]>,
        prefix: Option<&[u8]>,
        local: &[u8],
        qualified: &[u8],
        value: &[u8],
    ) {
        let (namespace, prefix) = (namespace.map(c_string), prefix.map(c_string));
        let (local, qualified, value) = (c_string(local), c_string(qualified), c_string(value));
        let qname = NsName {
            namespace_uri: namespace.as_deref(),
            prefix: prefix.as_deref(),
            local_name: &local,
            name: Some(&qualified),
        };
        if element.is_element() {
            attrs::set_ns(element, qname, Some(&value));
        }
    }

    fn append(&mut self, parent: Node<'static>, child: Node<'static>) {
        parent.append(child);
    }

    fn free(&mut self, node: Node<'static>) {
        node.free_tree();
    }
}

pub fn parse_document(input: &[u8]) -> Result<Tree, (i32, i32)> {
    crate::parse(&mut DomBuilder, input).map(Tree::adopt)
}
