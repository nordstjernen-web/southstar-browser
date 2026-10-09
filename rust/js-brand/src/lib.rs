//! Southstar — WebIDL brand checks for the native members of every interface that inherits from Node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use southstar_js_engine::quickjs::{self, BrandMode};
use southstar_js_engine::{Scope, Value};

const LENIENT: &[&str] = &["onmouseenter", "onmouseleave", "onreadystatechange"];

const PROMISED: &[&str] = &[
    "decode",
    "exitFullscreen",
    "exitPictureInPicture",
    "hasStorageAccess",
    "play",
    "requestFullscreen",
    "requestPictureInPicture",
    "requestPointerLock",
    "requestStorageAccess",
    "scroll",
    "scrollBy",
    "scrollIntoView",
    "scrollTo",
    "setMediaKeys",
    "setSinkId",
];

const MAX_PROTOTYPE_DEPTH: usize = 64;

struct Plan {
    node_proto: Value,
    attr_proto: Option<Value>,
    node_brand: i32,
    attr_brand: i32,
    element_brand: i32,
}

impl Plan {
    fn brand_for(&self, proto: &Value) -> i32 {
        if proto.same_object(&self.node_proto) {
            self.node_brand
        } else if self
            .attr_proto
            .as_ref()
            .is_some_and(|attr| proto.same_object(attr))
        {
            self.attr_brand
        } else {
            self.element_brand
        }
    }
}

fn mode_for(name: Option<&str>) -> BrandMode {
    match name {
        Some(name) if LENIENT.contains(&name) => BrandMode::Ignore,
        Some(name) if PROMISED.contains(&name) => BrandMode::Reject,
        _ => BrandMode::Throw,
    }
}

fn interface_proto(scope: &mut Scope<'_>, global: &Value, name: &str) -> Option<Value> {
    let ctor = scope.get(global, name).ok()?;
    scope.get(&ctor, "prototype").ok().filter(Value::is_object)
}

pub fn brand_node_interfaces(scope: &mut Scope<'_>, element_class: u32, attr_class: u32) {
    let global = scope.global();
    let node_proto = interface_proto(scope, &global, "Node");
    let attr_proto = interface_proto(scope, &global, "Attr");
    let node_brand = quickjs::new_function_brand(scope, &[element_class, attr_class]);
    let attr_brand = quickjs::new_function_brand(scope, &[attr_class]);
    let element_brand = quickjs::new_function_brand(scope, &[element_class]);
    let Some(node_proto) = node_proto else {
        return;
    };
    let plan = Plan {
        node_proto,
        attr_proto,
        node_brand,
        attr_brand,
        element_brand,
    };
    let Ok(names) = scope.own_property_keys(&global, false) else {
        return;
    };
    for name in &names {
        brand_global_interface(scope, &global, name, &plan);
    }
}

fn brand_global_interface(scope: &mut Scope<'_>, global: &Value, name: &Value, plan: &Plan) {
    let Ok(Some(property)) = scope.own_property(global, name) else {
        return;
    };
    if !scope.is_function(&property.value) {
        return;
    }
    let Some(proto) = scope
        .get(&property.value, "prototype")
        .ok()
        .filter(Value::is_object)
    else {
        return;
    };
    if inherits(scope, &proto, &plan.node_proto) {
        brand_members(scope, &proto, plan.brand_for(&proto));
    }
}

fn inherits(scope: &mut Scope<'_>, proto: &Value, root: &Value) -> bool {
    let mut current = proto.clone();
    for _ in 0..MAX_PROTOTYPE_DEPTH {
        if !current.is_object() {
            return false;
        }
        if current.same_object(root) {
            return true;
        }
        match scope.get_prototype(&current) {
            Ok(next) => current = next,
            Err(_) => return false,
        }
    }
    false
}

fn brand_members(scope: &mut Scope<'_>, proto: &Value, brand: i32) {
    if brand == 0 {
        return;
    }
    let Ok(keys) = scope.own_property_keys(proto, true) else {
        return;
    };
    for key in &keys {
        brand_member(scope, proto, key, brand);
    }
}

fn brand_member(scope: &mut Scope<'_>, proto: &Value, key: &Value, brand: i32) {
    let name = if key.is_string() {
        scope.to_string(key).ok()
    } else {
        None
    };
    if name.as_deref() == Some("constructor") {
        return;
    }
    let Ok(Some(property)) = scope.own_property(proto, key) else {
        return;
    };
    let mode = mode_for(name.as_deref());
    for function in [&property.value, &property.getter, &property.setter] {
        quickjs::set_function_brand(scope, function, brand, mode);
    }
}
