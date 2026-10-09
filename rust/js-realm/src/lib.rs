//! Southstar — ShadowRealm: a fresh global realm whose evaluate() hands back only primitives and wrapped callables.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::rc::Rc;

use southstar_js_engine::{Realm, Scope, Value};

const BOUNDARY_ERROR: &str =
    "ShadowRealm wrapped function: only primitives and callables may cross the realm boundary";

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    if matches!(scope.has_property(global, "ShadowRealm"), Ok(true)) {
        return;
    }
    let ctor = scope.constructor("ShadowRealm", 0, construct);
    let proto = scope.new_object();
    let evaluate_fn = scope.function("evaluate", 1, evaluate);
    let _ = scope.set(&proto, "evaluate", evaluate_fn);
    let import_fn = scope.function("importValue", 2, import_value);
    let _ = scope.set(&proto, "importValue", import_fn);
    let _ = scope.set_constructor(&ctor, &proto);
    let _ = scope.set(global, "ShadowRealm", ctor);
}

fn construct(scope: &mut Scope<'_>, new_target: &Value, _args: &[Value]) -> Result<Value, Value> {
    let Some(realm) = scope.new_detached_realm() else {
        return Err(scope.range_error("out of memory"));
    };
    let proto = scope
        .get(new_target, "prototype")
        .ok()
        .filter(Value::is_object);
    Ok(scope.new_host_object(proto.as_ref(), Rc::new(realm)))
}

fn this_realm(scope: &mut Scope<'_>, this: &Value) -> Result<Rc<Realm>, Value> {
    scope
        .host_data::<Rc<Realm>>(this)
        .ok_or_else(|| scope.type_error("not a ShadowRealm"))
}

fn evaluate(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let realm = this_realm(scope, this)?;
    let Some(source) = args.first().filter(|source| source.is_string()) else {
        return Err(scope.type_error("ShadowRealm.prototype.evaluate expects a string"));
    };
    let source = scope.to_string(source)?;
    let result = scope.in_realm(&realm, |child| {
        child
            .eval_script(&source, "<shadowrealm>")
            .map_err(|exception| child.to_string(&exception).ok())
    });
    match result {
        Ok(value) => wrap(scope, this, value),
        Err(message) => {
            Err(scope.type_error(message.as_deref().unwrap_or("ShadowRealm evaluate threw")))
        }
    }
}

fn wrap(scope: &mut Scope<'_>, realm_obj: &Value, value: Value) -> Result<Value, Value> {
    if !value.is_object() {
        return Ok(value);
    }
    if scope.is_function(&value) {
        return Ok(scope.bound_function("", 0, wrapped_call, &[value, realm_obj.clone()]));
    }
    Err(scope.type_error("ShadowRealm: evaluation result must be a primitive or a callable"))
}

fn wrapped_call(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let [target, realm_obj] = data else {
        return Ok(Value::undefined());
    };
    if args
        .iter()
        .any(|arg| arg.is_object() && !scope.is_function(arg))
    {
        return Err(scope.type_error(BOUNDARY_ERROR));
    }
    match scope.call(target, &Value::undefined(), args) {
        Ok(value) => wrap(scope, realm_obj, value),
        Err(exception) => {
            let message = scope.to_string(&exception).ok();
            Err(scope.type_error(message.as_deref().unwrap_or("ShadowRealm callable threw")))
        }
    }
}

fn import_value(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    this_realm(scope, this)?;
    let error = scope.new_error();
    let message = scope.string("ShadowRealm.prototype.importValue is not supported");
    scope.set(&error, "message", message)?;
    scope.rejected_promise(&error)
}
