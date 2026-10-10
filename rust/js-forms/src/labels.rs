//! Southstar — labelable elements and the label to control association.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{children, index};
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult, MAX_DEPTH, ffi, named, type_is, type_of};

pub(crate) fn is_labelable(node: Element) -> bool {
    match node.element_name() {
        Some(b"input") => type_of(node).is_none() || !type_is(node, "hidden"),
        Some(b"button" | b"meter" | b"output" | b"progress" | b"select" | b"textarea") => true,
        _ => false,
    }
}

pub(crate) fn first_labelable_descendant(node: Element, depth: i32) -> Option<Element> {
    if depth >= MAX_DEPTH {
        return None;
    }
    children(node).find_map(|child| {
        if is_labelable(child) {
            Some(child)
        } else {
            first_labelable_descendant(child, depth + 1)
        }
    })
}

pub(crate) fn associated_control(label: Element) -> Option<Element> {
    match label.attr(c"for") {
        Some(id) => index::find_by_id(label.root(), id).filter(|t| is_labelable(*t)),
        None => first_labelable_descendant(label, 0),
    }
}

pub(crate) fn control_in_document(label: Element, doc: Option<Element>) -> Option<Element> {
    let by_id = label
        .attr(c"for")
        .filter(|id| !id.is_empty())
        .zip(doc)
        .and_then(|(id, doc)| index::find_by_id(doc, id))
        .filter(|t| is_labelable(*t));
    by_id.or_else(|| first_labelable_descendant(label, 0))
}

pub(crate) fn control(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(label) = ffi::element(this).filter(|l| named(*l, "label")) else {
        return Ok(Value::null());
    };
    match associated_control(label) {
        Some(target) => Ok(ffi::wrap(scope, Some(target))),
        None => Ok(Value::null()),
    }
}
