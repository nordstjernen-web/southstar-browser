//! Southstar — the StaticRange constructor: a range snapshot that does not follow DOM mutations.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi, get};

const DOCUMENT_TYPE_NODE: i32 = 10;

const ATTRIBUTE_NODE: i32 = 2;

fn is_doctype_or_attr(scope: &mut Scope<'_>, node: &Value) -> bool {
    let node_type = get(scope, node, "nodeType");
    let node_type = scope.to_int32(&node_type).unwrap_or(0);
    node_type == DOCUMENT_TYPE_NODE || node_type == ATTRIBUTE_NODE
}

pub(crate) fn construct(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let Some(init) = args.first().filter(|init| init.is_object()) else {
        return Err(scope.type_error("Failed to construct 'StaticRange': 1 argument required"));
    };
    let start = get(scope, init, "startContainer");
    let end = get(scope, init, "endContainer");
    let start_offset = get(scope, init, "startOffset");
    let end_offset = get(scope, init, "endOffset");
    if !start.is_object()
        || !end.is_object()
        || start_offset.is_undefined()
        || end_offset.is_undefined()
    {
        return Err(
            scope.type_error("Failed to construct 'StaticRange': a required member is undefined")
        );
    }
    if is_doctype_or_attr(scope, &start) || is_doctype_or_attr(scope, &end) {
        return Err(ffi::dom_exception(
            scope,
            "InvalidNodeTypeError",
            24,
            "StaticRange boundary must not be a DocumentType or Attr node",
        ));
    }
    let start_offset = scope.to_int32(&start_offset).unwrap_or(0);
    let end_offset = scope.to_int32(&end_offset).unwrap_or(0);
    let collapsed = start_offset == end_offset && start.same_object(&end);
    let range = scope.new_object();
    let _ = scope.set(&range, "startContainer", start);
    let _ = scope.set(&range, "startOffset", Value::int(start_offset));
    let _ = scope.set(&range, "endContainer", end);
    let _ = scope.set(&range, "endOffset", Value::int(end_offset));
    let _ = scope.set(&range, "collapsed", Value::boolean(collapsed));
    Ok(range)
}
