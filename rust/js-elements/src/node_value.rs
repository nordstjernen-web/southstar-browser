//! Southstar — nodeValue, data and length on the node wrapper, and whether an element takes part in the rendered tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::Kind;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, ffi};

fn is_character_data(node: Element) -> bool {
    matches!(node.kind(), Kind::Text | Kind::Comment)
}

fn name_ignore_case(node: Element, tag: &[u8]) -> bool {
    node.name()
        .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag))
}

fn is_object_element(node: Option<Element>) -> bool {
    node.is_some_and(|n| n.is_element() && name_ignore_case(n, b"object"))
}

fn utf8_sequence_length(lead: u8) -> usize {
    match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        0xF8..=0xFB => 5,
        0xFC..=0xFD => 6,
        _ => 1,
    }
}

fn utf16_length(text: &CStr) -> i32 {
    let bytes = text.to_bytes();
    let mut units = 0;
    let mut index = 0;
    while index < bytes.len() {
        let lead = bytes[index];
        units += if (0xF0..0xFE).contains(&lead) { 2 } else { 1 };
        index += utf8_sequence_length(lead);
    }
    units
}

fn c_string_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

pub fn text_length(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::int(0));
    };
    if is_character_data(node) {
        return Ok(Value::int(node.text().map_or(0, utf16_length)));
    }
    if name_ignore_case(node, b"select") {
        return crate::element::select_length(scope, this);
    }
    if name_ignore_case(node, b"form") {
        let controls = ffi::form_controls_snapshot(scope, this)?;
        return scope.get(&controls, "length");
    }
    Ok(Value::undefined())
}

pub fn node_value(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    match ffi::unwrap_node(this) {
        Some(node) if is_character_data(node) => {
            Ok(scope.string_from_bytes(node.text_with_len().unwrap_or_default()))
        }
        _ => Ok(Value::null()),
    }
}

pub fn set_node_value(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let Some(node) = ffi::unwrap_node(this).filter(|&n| is_character_data(n)) else {
        return Ok(());
    };
    let text = if value.is_null() {
        Vec::new()
    } else {
        scope.to_bytes(value)?
    };
    let old = node.text().map_or_else(Vec::new, |t| t.to_bytes().to_vec());
    ffi::replace_text(node, &text);
    if let Some(js) = ffi::js_of(scope) {
        ffi::mark_mutated(js);
        ffi::record_character_data(js, node, &old);
    }
    Ok(())
}

pub fn data(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let node = ffi::unwrap_node(this);
    let Some(object) = node.filter(|&n| is_object_element(Some(n))) else {
        return node_value(scope, this);
    };
    let Some(value) = object.attr(c"data") else {
        return Ok(scope.string(""));
    };
    if let Some(resolved) = ffi::resolve_against_document_base(ffi::js_of(scope), value) {
        return Ok(scope.string_from_bytes(&resolved));
    }
    Ok(scope.string_from_bytes(value.to_bytes()))
}

pub fn set_data(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let node = ffi::unwrap_node(this);
    if let Some(object) = node.filter(|&n| is_object_element(Some(n))) {
        let bytes = scope.to_bytes(value)?;
        let text = c_string_prefix(&bytes);
        let js = ffi::js_of(scope);
        let changed = object
            .attr(c"data")
            .is_none_or(|old| old.to_bytes() != text);
        let text = CString::new(text).unwrap_or_default();
        ffi::set_attr_recorded(js, object, c"data", &text);
        if changed {
            ffi::schedule_iframe_load(js, object);
        }
        return Ok(());
    }
    if node.is_some_and(Element::is_element) {
        let attributes = Attributes {
            writable: true,
            enumerable: true,
            configurable: true,
        };
        scope.define(this, "data", value.clone(), attributes)?;
        return Ok(());
    }
    set_node_value(scope, this, value)
}

pub(crate) fn is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool {
    let Some(node) = ffi::unwrap_node(value) else {
        return false;
    };
    let Some(doc) = ffi::js_of(scope).and_then(ffi::current_document) else {
        return false;
    };
    let mut cursor = Some(node);
    while let Some(current) = cursor.filter(|&c| c != doc) {
        if !ffi::is_shadow_root(current)
            && let Some(parent) = current.parent()
            && ffi::has_shadow_child(parent)
            && !ffi::has_assigned_slot(current)
        {
            return false;
        }
        cursor = current.parent();
    }
    cursor == Some(doc)
}
