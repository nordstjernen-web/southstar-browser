//! Southstar — attribute bindings: DOMTokenList, the dataset DOMStringMap, NamedNodeMap and Attr nodes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod attr;
mod dataset;
mod ffi;
mod named_map;
mod token_list;

use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const SYNTAX_ERR: i32 = 12;
pub(crate) const INVALID_CHARACTER_ERR: i32 = 5;
pub(crate) const NOT_FOUND_ERR: i32 = 8;
pub(crate) const INUSE_ATTRIBUTE_ERR: i32 = 10;

pub(crate) fn same_node(a: Element, b: Element) -> bool {
    a.as_ptr() == b.as_ptr()
}

pub(crate) fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn string_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<CString> {
    scope.to_bytes(value).map(|bytes| c_string(&bytes))
}

pub(crate) fn optional_string_arg(
    scope: &mut Scope<'_>,
    value: &Value,
) -> JsResult<Option<CString>> {
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        string_arg(scope, value).map(Some)
    }
}
