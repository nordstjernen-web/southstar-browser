//! Southstar — static NodeLists: the arrays querySelectorAll() and friends return, branded with item, namedItem, forEach, the array iterators and the NodeList tag.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::js_of;
use crate::page::{self, NodeListHelpers};
use crate::{JsResult, arg};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

const DECORATOR: &str = "(function(nl){ var Aproto = Array.prototype; function def(k, v){   Object.defineProperty(nl, k,     { value: v, writable: true, configurable: true }); } def(Symbol.iterator, Aproto[Symbol.iterator]); def('entries', Aproto.entries); def('keys',    Aproto.keys); def('values',  Aproto.values); try { Object.defineProperty(nl, Symbol.toStringTag,   { value: 'NodeList', configurable: true }); } catch(e){} return nl;})";

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = scope
        .get(array, "length")
        .unwrap_or_else(|_| Value::undefined());
    scope.to_int64(&length).map_or(0, |n| n as u32)
}

fn helpers(scope: &mut Scope<'_>) -> NodeListHelpers {
    let js = js_of(scope);
    if let Some(helpers) = page::nodelist_helpers(js) {
        return helpers;
    }
    let decorator = if js.is_null() {
        None
    } else {
        scope.eval_native_script(DECORATOR, "<nodelist>").ok()
    };
    let helpers = NodeListHelpers {
        decorator,
        item: scope.function("item", 1, item),
        named_item: scope.function("namedItem", 1, named_item),
        for_each: scope.function("forEach", 1, for_each),
    };
    if !js.is_null() {
        page::set_nodelist_helpers(js, helpers.clone());
    }
    helpers
}

pub(crate) fn finalize(scope: &mut Scope<'_>, list: &Value, length: u32) {
    let _ = scope.define(list, "__nsNodeList", Value::boolean(true), HIDDEN);
    let _ = scope.define(
        list,
        "length",
        Value::int64(length as i64),
        Attributes::CONFIGURABLE,
    );
    let helpers = helpers(scope);
    let _ = scope.define(list, "item", helpers.item, Attributes::METHOD);
    let _ = scope.define(list, "namedItem", helpers.named_item, Attributes::METHOD);
    let _ = scope.define(list, "forEach", helpers.for_each, Attributes::METHOD);
    if let Some(decorator) = helpers.decorator {
        let _ = scope.call(&decorator, &Value::undefined(), core::slice::from_ref(list));
    }
}

pub(crate) fn finish(scope: &mut Scope<'_>, array: Value) -> Value {
    let length = array_length(scope, &array);
    finalize(scope, &array, length);
    array
}

pub(crate) fn empty(scope: &mut Scope<'_>) -> Value {
    let list = scope.new_array();
    finalize(scope, &list, 0);
    list
}

fn item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::null());
    }
    let index = match scope.to_int32(&args[0]) {
        Ok(index) if index >= 0 => index as u32,
        _ => return Ok(Value::null()),
    };
    let value = scope.get_index(this, index)?;
    Ok(if value.is_undefined() {
        Value::null()
    } else {
        value
    })
}

fn string_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = scope.get(object, key).ok()?;
    scope.to_bytes(&value).ok()
}

fn named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::null());
    }
    let Ok(name) = scope.to_bytes(&args[0]) else {
        return Ok(Value::null());
    };
    if name.is_empty() {
        return Ok(Value::null());
    }
    let length = array_length(scope, this);
    for i in 0..length {
        let element = scope
            .get_index(this, i)
            .unwrap_or_else(|_| Value::undefined());
        if string_prop(scope, &element, "id").as_deref() == Some(&name[..])
            || string_prop(scope, &element, "name").as_deref() == Some(&name[..])
        {
            return Ok(element);
        }
    }
    Ok(Value::null())
}

fn for_each(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let callback = arg(args, 0);
    if !scope.is_function(&callback) {
        return Ok(Value::undefined());
    }
    let length = array_length(scope, this);
    let this_arg = arg(args, 1);
    for i in 0..length {
        let element = scope
            .get_index(this, i)
            .unwrap_or_else(|_| Value::undefined());
        scope.call(
            &callback,
            &this_arg,
            &[element, Value::int(i as i32), this.clone()],
        )?;
    }
    Ok(Value::undefined())
}
