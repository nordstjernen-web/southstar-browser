//! Southstar — the DOM and window interface objects: prototype hierarchy, instanceof, toStringTag, unscopables, member shapes, the global's shape, PDF plugins, window.chrome and the window's compatibility members.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod chrome;
mod compat;
mod ffi;
mod global;
mod hierarchy;
mod instof;
mod sources;

use southstar_js_engine::{Attributes, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const DATA_PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn proto_of(scope: &mut Scope<'_>, global: &Value, ctor_name: &str) -> Value {
    match scope.get(global, ctor_name) {
        Ok(ctor) if ctor.is_object() => scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined()),
        _ => Value::undefined(),
    }
}

pub(crate) fn chain_proto(scope: &mut Scope<'_>, global: &Value, child_ctor: &str, parent: &Value) {
    if !parent.is_object() {
        return;
    }
    let proto = proto_of(scope, global, child_ctor);
    if proto.is_object() {
        let _ = scope.set_prototype(&proto, parent);
    }
}

pub(crate) fn well_known_symbol(scope: &mut Scope<'_>, name: &str) -> Option<Value> {
    let global = scope.global();
    let symbol = scope.get(&global, "Symbol").ok()?;
    if !symbol.is_object() {
        return None;
    }
    scope.get(&symbol, name).ok()
}

pub fn define_element_unscopables(scope: &mut Scope<'_>, proto: &Value) {
    let Some(key) = well_known_symbol(scope, "unscopables") else {
        return;
    };
    let unscopables = scope.new_object_with_proto(&Value::null());
    for name in [
        "before",
        "after",
        "replaceWith",
        "remove",
        "prepend",
        "append",
        "replaceChildren",
    ] {
        let _ = scope.define(&unscopables, name, Value::boolean(true), DATA_PROPERTY);
    }
    let _ = scope.define_with_key(proto, &key, unscopables, Attributes::CONFIGURABLE);
}

fn shapes_source(members: &[(&str, &str)], body: &str) -> String {
    let mut source = String::from("(function(G){  var W = {");
    for (interface, names) in members {
        source.push_str(&format!("  {interface}: '{names}',"));
    }
    source.push_str("  };");
    source.push_str(body);
    source
}

fn run_native_script(scope: &mut Scope<'_>, source: &str, name: &str) {
    let _ = scope.eval_native_script(source, name);
}

pub fn install_node_shapes(scope: &mut Scope<'_>) {
    let source = shapes_source(sources::NODE_SHAPES_MEMBERS, sources::NODE_SHAPES_BODY);
    run_native_script(scope, &source, "<node-shapes>");
}

pub fn install_element_shapes(scope: &mut Scope<'_>) {
    let source = shapes_source(
        sources::ELEMENT_SHAPES_MEMBERS,
        sources::ELEMENT_SHAPES_BODY,
    );
    run_native_script(scope, &source, "<element-shapes>");
}

pub fn install_pdf_plugins(scope: &mut Scope<'_>) {
    run_native_script(scope, sources::PDF_PLUGINS, "<pdf-plugins>");
}

pub fn link_interface_ctors(scope: &mut Scope<'_>) {
    run_native_script(
        scope,
        sources::INTERFACE_CTOR_LINKS,
        "<interface-ctor-links>",
    );
}

pub use chrome::install as install_window_chrome;
pub use global::{
    install_namespace_object, install_navigator_shape, install_web_api_shapes,
    lock_global_prototypes, shape_window_global,
};
pub use hierarchy::install as install_dom_hierarchy;
pub use instof::{install_has_instance, install_to_string_tag};
