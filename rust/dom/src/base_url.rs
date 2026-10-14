//! Southstar — a document's base URL: the href of its first <base> element with one, resolved against the document's own URL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::{Kind, Node};
use crate::{MAX_DEPTH, ancestors_and_self, children};

fn first_base_href<'n>(n: Node<'n>, depth: i32) -> Option<&'n CStr> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if n.element_name() == Some(b"base")
        && let Some(href) = n.attr(c"href")
    {
        return Some(href);
    }
    children(n)
        .filter(|c| c.kind() != Kind::Document)
        .find_map(|c| first_base_href(c, depth + 1))
}

pub fn owning_document(n: Node<'_>) -> Option<Node<'_>> {
    ancestors_and_self(n).find(|a| a.kind() == Kind::Document)
}

pub fn document_base(doc: Node, document_url: Option<&[u8]>) -> Option<Vec<u8>> {
    let href = first_base_href(doc, 0)?;
    southstar_url::bytes::resolve(document_url, href.to_bytes())
}
