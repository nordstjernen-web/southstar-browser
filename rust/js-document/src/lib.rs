//! Southstar — the Document property bindings: cookie, title, dir, body and head, the legacy colours, readyState, referrer, compatMode and the document's element getters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod cookie;
mod elements;
mod ffi;
mod props;

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const FLAG_QUIRKS: u32 = 1 << 5;

pub(crate) fn document_for(scope: &Scope<'_>, this: &Value) -> Option<Element> {
    if let Some(node) = ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Document) {
        return Some(node);
    }
    ffi::current_document(ffi::js_of(scope))
}

pub(crate) fn element_named(node: Node<'_>, tag: &str) -> bool {
    node.element_name() == Some(tag.as_bytes())
}

pub(crate) fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |nul| &bytes[..nul])
}
