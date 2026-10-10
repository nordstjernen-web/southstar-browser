//! Southstar — reflected HTML IDL attributes: string, URL, enumerated, boolean and integer accessors over element content attributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod numeric;
mod strings;
mod tables;

use core::ffi::CStr;

use southstar_dom::{Kind, Node};
use southstar_js_engine::Value;

use tables::Keywords;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) fn raw_name(node: Element) -> Option<&'static [u8]> {
    node.name().map(CStr::to_bytes)
}

pub(crate) fn raw_name_is(node: Element, tag: &[u8]) -> bool {
    raw_name(node) == Some(tag)
}

pub(crate) fn is_named(node: Element, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

pub(crate) fn name_is_any_of(node: Element, tags: &[&[u8]]) -> bool {
    node.element_name()
        .is_some_and(|name| tags.iter().any(|tag| name.eq_ignore_ascii_case(tag)))
}

pub(crate) fn is_custom_element(node: Element) -> bool {
    node.kind() == Kind::Element
        && node
            .name()
            .is_some_and(|name| name.to_bytes().contains(&b'-'))
}

pub(crate) fn keyword_match(keywords: &[&'static CStr], value: &[u8]) -> Option<&'static CStr> {
    keywords
        .iter()
        .copied()
        .find(|kw| kw.to_bytes().eq_ignore_ascii_case(value))
}

pub(crate) fn normalize(attr: &[u8], value: Option<&[u8]>) -> Option<&'static CStr> {
    let def: &Keywords = tables::NORMALIZED
        .iter()
        .find(|d| d.attr.to_bytes().eq_ignore_ascii_case(attr))?;
    match value {
        None => def.missing,
        Some(v) => keyword_match(def.keywords, v).or(def.invalid),
    }
}
