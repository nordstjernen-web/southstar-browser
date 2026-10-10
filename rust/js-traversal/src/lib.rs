//! Southstar — Range, Selection, TreeWalker and NodeIterator: the native range and selection objects and filtered DOM traversal.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod iterator;
mod range;
mod selection;
mod walker;

use southstar_dom::Node;
use southstar_dom::serialize::is_embedded_doc;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const READ_ONLY: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn put(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    put(scope, object, name, function);
}

pub(crate) fn noop(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn hidden_child(node: Element) -> bool {
    is_embedded_doc(node) || (node.is_element() && node.attr(c"data-nd-shadow-root").is_some())
}

pub(crate) fn is_inclusive_ancestor(ancestor: Element, node: Element) -> bool {
    southstar_dom::ancestors_and_self(node).any(|a| a == ancestor)
}

pub(crate) fn set_instance_proto(scope: &mut Scope<'_>, object: &Value, constructor_name: &str) {
    let global = scope.global();
    let constructor = get(scope, &global, constructor_name);
    if !constructor.is_object() {
        return;
    }
    let prototype = get(scope, &constructor, "prototype");
    if prototype.is_object() {
        let _ = scope.set_prototype(object, &prototype);
    }
}
