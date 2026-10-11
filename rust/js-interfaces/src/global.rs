//! Southstar — the window global's WebIDL shape: its prototype chain and immutable prototypes, the navigator and web API singletons, and namespace objects.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::{ffi, run_native_script, sources};

const MAX_LOCKED_DEPTH: usize = 8;

pub fn lock_global_prototypes(scope: &mut Scope<'_>) {
    let mut object = scope.global();
    for _ in 0..MAX_LOCKED_DEPTH {
        if !object.is_object() {
            break;
        }
        scope.set_immutable_prototype(&object);
        match scope.get_prototype(&object) {
            Ok(next) => object = next,
            Err(_) => break,
        }
    }
}

pub fn shape_window_global(scope: &mut Scope<'_>) {
    run_native_script(scope, sources::WINDOW_GLOBAL_SHAPE, "<window-global-shape>");
}

pub fn install_web_api_shapes(scope: &mut Scope<'_>) {
    run_native_script(scope, sources::WEB_API_SHAPES, "<web-api-shapes>");
}

pub fn install_navigator_shape(scope: &mut Scope<'_>) {
    let Ok(shape) = scope.eval_native_script(sources::NAVIGATOR_SHAPE, "<navigator-shape>") else {
        return;
    };
    let others = ffi::navigator_brand(scope);
    let _ = scope.call(&shape, &Value::undefined(), &[others]);
}

pub fn install_namespace_object(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    object: Value,
    tag: &str,
) {
    let proto = scope.new_object();
    let _ = scope.set_prototype(&object, &proto);
    let tag_key = scope
        .get(global, "Symbol")
        .and_then(|symbol| scope.get(&symbol, "toStringTag"));
    if let Ok(key) = tag_key {
        let tag = scope.string(tag);
        let _ = scope.define_with_key(&object, &key, tag, Attributes::CONFIGURABLE);
    }
    let _ = scope.define(global, name, object, Attributes::METHOD);
}
