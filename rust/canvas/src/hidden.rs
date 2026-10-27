//! Southstar — the hidden-state objects behind every canvas interface: a kind, a null-prototype state object and the canvas node it belongs to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::Cell;

use southstar_js_engine::{Scope, Trace, Value};

use crate::ffi::state::Node;

pub(crate) const KIND_CTX2D: i32 = 0;
pub(crate) const KIND_OFFSCREEN_CTX2D: i32 = 1;
pub(crate) const KIND_GRADIENT: i32 = 2;
pub(crate) const KIND_PATTERN: i32 = 3;
pub(crate) const KIND_IMAGEDATA: i32 = 4;
pub(crate) const KIND_TEXTMETRICS: i32 = 5;
pub(crate) const KIND_OFFSCREEN: i32 = 6;

pub(crate) struct Hidden {
    pub kind: i32,
    pub state: Value,
    pub node: Cell<Node>,
}

impl Trace for Hidden {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.state);
    }
}

pub(crate) fn new(realm: &mut Scope<'_>, kind: i32, proto: &Value) -> Value {
    let state = realm.new_object_with_proto(&Value::null());
    let hidden = Hidden {
        kind,
        state,
        node: Cell::new(Node::NONE),
    };
    let proto = proto.is_object().then_some(proto);
    realm.new_traced_host_object(proto, hidden)
}

pub(crate) fn kind_of(value: &Value) -> Option<i32> {
    crate::ffi::with_hidden(value, |h| h.kind)
}

pub(crate) fn is(value: &Value, kind: i32) -> bool {
    kind_of(value) == Some(kind)
}

pub(crate) fn is_ctx2d(value: &Value) -> bool {
    matches!(kind_of(value), Some(KIND_CTX2D | KIND_OFFSCREEN_CTX2D))
}

pub(crate) fn node(value: &Value) -> Node {
    crate::ffi::with_hidden(value, |h| h.node.get()).unwrap_or(Node::NONE)
}

pub(crate) fn set_node(value: &Value, node: Node) {
    crate::ffi::with_hidden(value, |h| h.node.set(node));
}

pub(crate) fn state(value: &Value) -> Option<Value> {
    crate::ffi::with_hidden(value, |h| h.state.clone())
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, name: &str) -> Result<Value, Value> {
    match state(object) {
        Some(state) => scope.get(&state, name),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, name: &str, value: Value) {
    if let Some(state) = state(object) {
        let _ = scope.set(&state, name, value);
    }
}
