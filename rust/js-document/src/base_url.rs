//! Southstar — a document's base URL: the first usable <base href> the page's base-uri policy allows, else the document's own URL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CStr;

use southstar_dom::Kind;

use crate::Element;
use crate::ffi::{self, Js};

fn base_elements(doc: Element) -> Vec<Element> {
    southstar_dom::index::tag_lookup(doc, c"base", |nodes| {
        (0..nodes.len()).map(|i| nodes.get(i)).collect()
    })
    .unwrap_or_default()
}

fn usable_base(js: Js, href: &CStr, current_url: Option<&CStr>) -> Option<Vec<u8>> {
    if let Some(current) = current_url.filter(|url| !url.is_empty())
        && let Some(resolved) = ffi::url_resolve(current, href)
    {
        return ffi::base_uri_allowed(js, &resolved, current_url).then_some(resolved);
    }
    ffi::base_uri_allowed(js, href.to_bytes(), current_url).then(|| href.to_bytes().to_vec())
}

pub(crate) fn document_base_url(
    js: Js,
    doc: Option<Element>,
    current_url: Option<&CStr>,
) -> Option<Vec<u8>> {
    let from_base = doc.into_iter().flat_map(base_elements).find_map(|base| {
        let href = base.attr(c"href").filter(|href| !href.is_empty())?;
        usable_base(js, href, current_url)
    });
    from_base.or_else(|| {
        current_url
            .filter(|url| !url.is_empty())
            .map(|url| url.to_bytes().to_vec())
    })
}

pub(crate) fn page_base_url(js: Js) -> Option<Vec<u8>> {
    if js.is_null() {
        return None;
    }
    let current_url = ffi::current_url_c(js);
    document_base_url(js, ffi::current_document(js), current_url)
}

pub(crate) fn node_base_url(js: Js, node: Option<Element>) -> Option<Vec<u8>> {
    let doc = node.and_then(|node| {
        southstar_dom::ancestors_and_self(node).find(|n| n.kind() == Kind::Document)
    });
    let fallback = ffi::node_doc_base(js, node);
    document_base_url(js, doc, fallback)
}
