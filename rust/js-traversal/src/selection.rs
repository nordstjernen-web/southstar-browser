//! Southstar — window.getSelection and document.getSelection over the shell's current text selection.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, js_of};
use crate::{JsResult, bind, noop, put};

const SELECTION_METHODS: &[(&str, u32)] = &[
    ("removeAllRanges", 0),
    ("addRange", 1),
    ("removeRange", 1),
    ("collapse", 2),
    ("collapseToStart", 0),
    ("collapseToEnd", 0),
    ("empty", 0),
    ("setBaseAndExtent", 4),
    ("extend", 2),
    ("selectAllChildren", 1),
    ("modify", 3),
    ("setPosition", 2),
    ("deleteFromDocument", 0),
];

const RANGE_STUBS: &[&str] = &[
    "setStart",
    "setEnd",
    "setStartBefore",
    "setStartAfter",
    "setEndBefore",
    "setEndAfter",
    "selectNode",
    "selectNodeContents",
    "collapse",
    "cloneRange",
    "deleteContents",
    "extractContents",
    "insertNode",
    "surroundContents",
    "detach",
    "compareBoundaryPoints",
    "intersectsNode",
    "isPointInRange",
    "comparePoint",
    "createContextualFragment",
];

pub(crate) fn has_range(scope: &mut Scope<'_>) -> bool {
    ffi::selection_state(js_of(scope)).has_range
}

pub(crate) fn to_string(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let text = ffi::selection_state(js_of(scope)).text;
    Ok(scope.string_from_bytes(&text))
}

pub(crate) fn bounding_client_rect(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let state = ffi::selection_state(js_of(scope));
    let rect = if state.has_range {
        state.rect
    } else {
        [0.0; 4]
    };
    Ok(ffi::dom_rect(scope, rect))
}

pub(crate) fn client_rects(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let list = scope.new_array();
    if has_range(scope) {
        let rect = bounding_client_rect(scope, this, args)?;
        let _ = scope.set_index(&list, 0, rect);
    }
    Ok(list)
}

pub(crate) fn clone_contents(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let text = ffi::selection_state(js_of(scope)).text;
    let fragment = scope.new_object();
    put(scope, &fragment, "nodeType", Value::int(11));
    let name = scope.string("#document-fragment");
    put(scope, &fragment, "nodeName", name);
    let text = scope.string_from_bytes(&text);
    put(scope, &fragment, "textContent", text);
    let children = scope.new_array();
    put(scope, &fragment, "childNodes", children);
    Ok(fragment)
}

fn make_range(scope: &mut Scope<'_>) -> Value {
    let collapsed = !has_range(scope);
    let range = scope.new_object();
    put(scope, &range, "collapsed", Value::boolean(collapsed));
    put(scope, &range, "startContainer", Value::null());
    put(scope, &range, "endContainer", Value::null());
    put(scope, &range, "startOffset", Value::int(0));
    put(scope, &range, "endOffset", Value::int(0));
    put(scope, &range, "commonAncestorContainer", Value::null());
    bind(scope, &range, "toString", 0, to_string);
    bind(scope, &range, "cloneContents", 0, clone_contents);
    bind(
        scope,
        &range,
        "getBoundingClientRect",
        0,
        bounding_client_rect,
    );
    bind(scope, &range, "getClientRects", 0, client_rects);
    for name in RANGE_STUBS {
        bind(scope, &range, name, 0, noop);
    }
    range
}

fn range_at(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(make_range(scope))
}

fn always_false(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(false))
}

pub(crate) fn get_selection(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let has = has_range(scope);
    let selection = scope.new_object();
    put(scope, &selection, "anchorNode", Value::null());
    put(scope, &selection, "focusNode", Value::null());
    put(scope, &selection, "anchorOffset", Value::int(0));
    put(scope, &selection, "focusOffset", Value::int(0));
    put(scope, &selection, "isCollapsed", Value::boolean(!has));
    put(scope, &selection, "rangeCount", Value::int(i32::from(has)));
    let kind = scope.string(if has { "Range" } else { "None" });
    put(scope, &selection, "type", kind);
    bind(scope, &selection, "toString", 0, to_string);
    bind(scope, &selection, "getRangeAt", 1, range_at);
    for &(name, arity) in SELECTION_METHODS {
        bind(scope, &selection, name, arity, noop);
    }
    bind(scope, &selection, "containsNode", 2, always_false);
    Ok(selection)
}
