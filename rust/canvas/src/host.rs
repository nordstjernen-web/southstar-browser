//! Southstar — what WebGL and WebGPU take from the canvas: the canvas's realm, its interface prototypes, hidden-state objects and the surface of a drawImage() source.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::WeakNode;
use southstar_js_engine::{ContextHandle, Scope, Value};

use crate::ffi::state::Node;

pub fn canvas_realm(scope: &Scope<'_>, canvas: Option<WeakNode>) -> ContextHandle {
    crate::ffi::canvas_realm(scope, Node::from_key(canvas))
}

pub fn api_proto(scope: &mut Scope<'_>, canvas: Option<WeakNode>, iface: &str) -> Value {
    let realm = canvas_realm(scope, canvas);
    crate::ffi::in_realm(scope, realm, |realm| crate::api::api_proto(realm, iface))
}

pub fn api_interface(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    ctor: Value,
    parent: Option<&str>,
) -> Value {
    crate::api::interface(scope, global, name, ctor, parent)
}

pub fn new_hidden(
    scope: &mut Scope<'_>,
    canvas: Option<WeakNode>,
    kind: i32,
    iface: &str,
) -> Value {
    let realm = canvas_realm(scope, canvas);
    crate::ffi::new_in_realm(scope, realm, kind, iface)
}

pub fn hidden_is(value: &Value, kind: i32) -> bool {
    crate::hidden::is(value, kind)
}

pub fn hidden_get(scope: &mut Scope<'_>, value: &Value, name: &str) -> Result<Value, Value> {
    crate::hidden::get(scope, value, name)
}

pub fn hidden_set(scope: &mut Scope<'_>, value: &Value, name: &str, field: Value) {
    crate::hidden::set(scope, value, name, field);
}

pub fn drawimage_source_surface(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<(southstar_cairo::Surface, i32, i32)>, Value> {
    crate::ffi::drawimage_source_surface(scope, src)
}
