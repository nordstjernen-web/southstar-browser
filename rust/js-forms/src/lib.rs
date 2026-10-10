//! Southstar — the forms bindings: form-control values, selection, checkedness, select options, labels, FormData, constraint validation and submission.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod checkable;
mod ffi;
mod form_data;
mod labels;
mod select;
mod selection;
mod submit;
mod validity;
mod value;

use core::ffi::CStr;

use southstar_dom::{Kind, Node, children, serialize};
use southstar_js_engine::{Scope, Value};

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const MAX_DEPTH: i32 = 512;
const LISTED: [&str; 7] = [
    "input", "select", "textarea", "button", "fieldset", "output", "object",
];
const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";

pub(crate) fn named(node: Element, tag: &str) -> bool {
    node.element_name() == Some(tag.as_bytes())
}

pub(crate) fn name_is(node: Element, tag: &str) -> bool {
    node.element_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(tag.as_bytes()))
}

pub(crate) fn text_is(text: Option<&CStr>, wanted: &str) -> bool {
    text.is_some_and(|t| t.to_bytes().eq_ignore_ascii_case(wanted.as_bytes()))
}

pub(crate) fn text_is_any(text: Option<&CStr>, wanted: &[&str]) -> bool {
    wanted.iter().any(|w| text_is(text, w))
}

pub(crate) fn type_of(node: Element) -> Option<&'static CStr> {
    node.attr(c"type")
}

pub(crate) fn non_empty(text: Option<&CStr>) -> Option<&CStr> {
    text.filter(|t| !t.is_empty())
}

pub(crate) fn until_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    bytes
}

pub(crate) fn hidden_child(node: Element) -> bool {
    serialize::is_embedded_doc(node)
        || (node.kind() == Kind::Element && node.attr(SHADOW_ATTR).is_some())
}

pub(crate) fn form_owner(control: Element) -> Option<Element> {
    southstar_dom::controls::form_owner(control)
}

fn is_image_input(node: Element) -> bool {
    name_is(node, "input") && text_is(type_of(node), "image")
}

fn collect_listed(
    form: Element,
    scan: Element,
    include_image: bool,
    depth: i32,
    out: &mut Vec<Element>,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(scan) {
        if hidden_child(child) {
            continue;
        }
        if LISTED.iter().any(|tag| name_is(child, tag))
            && form_owner(child) == Some(form)
            && (include_image || !is_image_input(child))
        {
            out.push(child);
        }
        collect_listed(form, child, include_image, depth + 1, out);
    }
}

pub(crate) fn listed_controls(form: Element, include_image: bool) -> Vec<Element> {
    let mut out = Vec::new();
    collect_listed(form, form.root(), include_image, 0, &mut out);
    out
}

pub(crate) fn uint32(scope: &mut Scope<'_>, value: &Value) -> u32 {
    let number = scope.to_number(value).unwrap_or(0.0);
    if !number.is_finite() {
        return 0;
    }
    number.trunc().rem_euclid(4_294_967_296.0) as u32
}

pub(crate) fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let len = scope
        .get(array, "length")
        .unwrap_or_else(|_| Value::undefined());
    uint32(scope, &len)
}

pub(crate) fn index(scope: &mut Scope<'_>, object: &Value, i: u32) -> Value {
    scope
        .get_index(object, i)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn c_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.to_bytes(value).ok().map(until_nul)
}

pub(crate) fn to_uint32(scope: &mut Scope<'_>, value: &Value) -> JsResult<u32> {
    let number = scope.to_number(value)?;
    if !number.is_finite() {
        return Ok(0);
    }
    Ok(number.trunc().rem_euclid(4_294_967_296.0) as u32)
}

pub(crate) fn attr_bytes(node: Element, name: &CStr) -> Option<&'static [u8]> {
    node.attr(name).map(CStr::to_bytes)
}

pub(crate) fn type_is(node: Element, wanted: &str) -> bool {
    text_is(type_of(node), wanted)
}

pub(crate) fn parent_select(option: Element) -> Option<Element> {
    let mut parent = option.parent()?;
    if named(parent, "optgroup") {
        parent = parent.parent()?;
    }
    named(parent, "select").then_some(parent)
}
