//! Southstar — document factories, DOMImplementation, importNode/adoptNode/cloneNode, name validation, namespace lookups and the hyperlink URL-part accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod adopt;
mod factories;
mod ffi;
mod hyperlink;
mod implementation;
mod names;
mod namespaces;

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Attributes, Scope, Value};

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
