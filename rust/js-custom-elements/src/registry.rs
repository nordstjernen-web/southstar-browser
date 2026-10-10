//! Southstar — the CustomElementRegistry methods and the HTMLElement constructor path for custom element classes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::reactions::{self, key_for};
use crate::{JsResult, WRITABLE_CONFIGURABLE, arg, name_valid};

fn lookup_class(scope: &mut Scope<'_>, js: Js, name: &str) -> Option<Value> {
    let page = crate::existing_page(js)?;
    let document = ffi::realm_document(scope);
    page.lookup(&key_for(js, &name.to_ascii_lowercase(), document))
}

fn capture_observed_attributes(scope: &mut Scope<'_>, class: &Value) -> JsResult<()> {
    let proto = scope.get(class, "prototype")?;
    let callback = if proto.is_object() {
        scope.get(&proto, "attributeChangedCallback")?
    } else {
        Value::undefined()
    };
    if !scope.is_function(&callback) {
        return Ok(());
    }
    let observed = scope.get(class, "observedAttributes")?;
    let names = scope.new_array();
    if observed.is_array() {
        let length = scope.get(&observed, "length")?;
        let length = scope.to_int32(&length).unwrap_or(0);
        for i in 0..length.max(0) as u32 {
            let item = scope.get_index(&observed, i)?;
            let name = scope.to_string_value(&item)?;
            let _ = scope.set_index(&names, i, name);
        }
    }
    let _ = scope.define(class, "__nd_ce_observed", names, Attributes::CONFIGURABLE);
    Ok(())
}

fn record_extends(scope: &mut Scope<'_>, class: &Value, options: &Value) -> JsResult<()> {
    let extends = scope.get(options, "extends")?;
    if !extends.is_string() {
        return Ok(());
    }
    let extends = scope.to_string(&extends)?;
    if extends.is_empty() {
        return Ok(());
    }
    if name_valid(extends.as_bytes()) {
        return Err(scope.dom_exception(
            "NotSupportedError",
            "customElements.define: cannot extend a custom element name",
        ));
    }
    let lower = scope.string(&extends.to_ascii_lowercase());
    let _ = scope.define(class, "__nd_ce_extends", lower, Attributes::CONFIGURABLE);
    Ok(())
}

pub(crate) fn define(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Err(scope.type_error("customElements.define: name and class required"));
    }
    let class = &args[1];
    if !scope.is_function(class) && !scope.is_constructor(class) {
        return Err(scope.type_error("customElements.define: class must be a constructor"));
    }
    let raw = scope.to_string(&args[0])?;
    if !name_valid(raw.as_bytes()) {
        return Err(scope.type_error(&format!("customElements.define: invalid name '{raw}'")));
    }
    let name = raw.to_ascii_lowercase();
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let options = arg(args, 2);
    if options.is_object() {
        record_extends(scope, class, &options)?;
    }
    capture_observed_attributes(scope, class)?;

    let page = crate::page(js);
    let document = ffi::realm_document(scope);
    let key = key_for(js, &name, document);
    page.insert(key.clone(), class.clone());
    ffi::register_defined_element(&name);

    if let Some(root) = document.or_else(|| ffi::current_document(js)) {
        reactions::upgrade_subtree_named(js, &page, root, &key);
    }

    let waiters = page.pending.borrow_mut().remove(&key);
    for resolve in waiters.into_iter().flatten() {
        let _ = scope.call(&resolve, &Value::undefined(), std::slice::from_ref(class));
    }
    Ok(Value::undefined())
}

pub(crate) fn get(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(name) = args.first() else {
        return Ok(Value::undefined());
    };
    let name = scope.to_string(name)?;
    let js = ffi::js_of(scope);
    Ok(lookup_class(scope, js, &name).unwrap_or_else(Value::undefined))
}

pub(crate) fn when_defined(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let (promise, resolve, reject) = scope.new_promise()?;
    let Some(name) = args.first() else {
        let _ = scope.call(&reject, &Value::undefined(), &[]);
        return Ok(promise);
    };
    let raw = scope.to_string(name)?;
    if !name_valid(raw.as_bytes()) {
        let error = scope.new_error();
        let message = scope.string("Invalid custom element name");
        let _ = scope.set(&error, "message", message);
        let _ = scope.call(&reject, &Value::undefined(), &[error]);
        return Ok(promise);
    }
    let js = ffi::js_of(scope);
    let name = raw.to_ascii_lowercase();
    if let Some(existing) = lookup_class(scope, js, &name) {
        let _ = scope.call(&resolve, &Value::undefined(), &[existing]);
        return Ok(promise);
    }
    let document = ffi::realm_document(scope);
    let key = key_for(js, &name, document);
    crate::page(js)
        .pending
        .borrow_mut()
        .entry(key)
        .or_default()
        .push(resolve);
    Ok(promise)
}

pub(crate) fn upgrade(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if let Some(root) = args.first().and_then(ffi::unwrap_node) {
        reactions::upgrade_root(ffi::js_of(scope), root);
    }
    Ok(Value::undefined())
}

pub(crate) fn get_name(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let class = arg(args, 0);
    let name = class
        .is_object()
        .then(|| crate::existing_page(ffi::js_of(scope))?.name_of(&class))
        .flatten();
    Ok(name.map_or_else(Value::null, |name| scope.string(&name)))
}

pub(crate) fn html_element_construct(scope: &mut Scope<'_>, new_target: &Value) -> Option<Value> {
    if !new_target.is_object() {
        return None;
    }
    let page = crate::existing_page(ffi::js_of(scope))?;
    let name = page.name_of(new_target)?;
    if let Some(upgrading) = page.upgrading.borrow().clone() {
        return Some(upgrading);
    }
    let element = ffi::new_orphan_element(scope, &name);
    if !element.is_object() {
        return None;
    }
    let proto = crate::get(scope, new_target, "prototype");
    if proto.is_object() {
        let _ = scope.set_prototype(&element, &proto);
    }
    let _ = scope.define(
        &element,
        "__nd_ce_class",
        new_target.clone(),
        WRITABLE_CONFIGURABLE,
    );
    Some(element)
}
