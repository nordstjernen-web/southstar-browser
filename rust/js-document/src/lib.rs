//! Southstar — the Document bindings: cookie, title, dir, body and head, the legacy colours, readyState, referrer, compatMode, the element getters, document.open/write/close and the realm documents of frames and DOMImplementation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod base_url;
pub mod cookie;
pub mod elements;
pub(crate) mod ffi;
pub mod links;
pub mod props;
mod realm;
pub mod view;
pub mod write;

pub use ffi::install_runtime_hooks;
pub use ffi::{doc_base_url, seed_cookies_from_jar};

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value};

pub use ffi::nsr_document_define_doctype_getter;
pub use ffi::nsr_document_lift_methods_to_proto;
pub use ffi::nsr_make_realm_document;
pub use ffi::nsr_make_synth_xml_document;

pub use ffi::nsr_document_close;
pub use ffi::nsr_document_open;
pub use ffi::nsr_document_write;
pub use ffi::nsr_document_writeln;

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
