//! Southstar — document factories, the window's node constructors, DOMParser, CloseWatcher, DOMImplementation, importNode/adoptNode/cloneNode, name validation, namespace lookups and the hyperlink URL-part accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod adopt;
mod constructors;
pub mod factories;
pub(crate) mod ffi;
pub mod hyperlink;
pub mod implementation;
pub mod links;
mod names;
pub mod namespaces;

pub use names::{valid_attr_name, valid_element_local_name};

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Attributes, Scope, Value};

pub use ffi::nsr_doc_wrapper_is_xml;
pub use ffi::nsr_validate_attr_ns;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const FLAG_FRAGMENT: u32 = 1 << 2;
pub(crate) const FLAG_CDATA: u32 = 1 << 10;
pub(crate) const FLAG_KEEP_CASE: u32 = 1 << 12;
pub(crate) const FLAG_NOT_PARSER_INSERTED: u32 = 1 << 13;
pub(crate) const FLAG_XML_DOC: u32 = 1 << 14;

pub(crate) const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) const PLAIN: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn is_document(node: Element) -> bool {
    node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT == 0
}

pub(crate) fn is_nullish(value: &Value) -> bool {
    value.is_null() || value.is_undefined()
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn optional_text(scope: &mut Scope<'_>, value: &Value) -> JsResult<Option<Vec<u8>>> {
    if is_nullish(value) {
        return Ok(None);
    }
    scope.to_bytes(value).map(Some)
}

pub(crate) fn c_text(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    let mut bytes = scope.to_bytes(value)?;
    let end = ffi::c_prefix(&bytes).len();
    bytes.truncate(end);
    Ok(bytes)
}

pub(crate) fn split_prefix(qname: &[u8]) -> (Option<&[u8]>, &[u8]) {
    match qname.iter().position(|&b| b == b':') {
        Some(colon) => (Some(&qname[..colon]), &qname[colon + 1..]),
        None => (None, qname),
    }
}

pub fn install_runtime_hooks() {
    southstar_js_runtime::api::set_dom_factory_hooks(southstar_js_runtime::api::DomFactoryHooks {
        anchor_resolved_href: crate::ffi::anchor_resolved_href,
    });
    southstar_js_document::links::set_js_dom_factory_links(
        southstar_js_document::links::JsDomFactoryLinks {
            document_adopt_node: crate::ffi::nsr_document_adopt_node,
            document_import_node: crate::ffi::nsr_document_import_node,
            document_create_event: crate::ffi::nsr_document_createEvent,
            document_create_attribute_n_s: crate::ffi::nsr_document_createAttributeNS,
            document_create_attribute: crate::ffi::nsr_document_createAttribute,
            document_create_processing_instruction:
                crate::ffi::nsr_document_createProcessingInstruction,
            document_create_c_d_a_t_a_section: crate::ffi::nsr_document_createCDATASection,
            document_create_document_fragment: crate::ffi::nsr_document_createDocumentFragment,
            document_create_comment: crate::ffi::nsr_document_createComment,
            document_create_text_node: crate::ffi::nsr_document_createTextNode,
            document_create_element_n_s: crate::ffi::nsr_document_createElementNS,
            document_create_element: crate::ffi::nsr_document_createElement,
            document_implementation: crate::ffi::nsr_document_implementation,
        },
    );
    southstar_js_runtime::links::set_js_dom_factory_links(
        southstar_js_runtime::links::JsDomFactoryLinks {
            window_close_watcher_ctor: crate::ffi::nsr_window_close_watcher_ctor,
            dom_parser_parse_from_string: crate::ffi::nsr_dom_parser_parseFromString,
            window_dom_parser_ctor: crate::ffi::nsr_window_dom_parser_ctor,
            window_option_ctor: crate::ffi::nsr_window_option_ctor,
            window_comment_ctor: crate::ffi::nsr_window_comment_ctor,
            window_text_ctor: crate::ffi::nsr_window_text_ctor,
            window_document_fragment_ctor: crate::ffi::nsr_window_document_fragment_ctor,
            window_audio_ctor: crate::ffi::nsr_window_audio_ctor,
            window_image_ctor: crate::ffi::nsr_window_image_ctor,
            attr_clone_node: crate::ffi::nsr_attr_cloneNode,
            tag_owner_document: crate::ffi::nsr_tag_owner_document,
        },
    );
}
