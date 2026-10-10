//! Southstar — the URL interface: construction through the lexbor-backed ns_url_* helpers, canParse, parse and the prototype accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js, UrlParts};
use crate::{JsResult, arg, is_nullish, set, set_str, with_page};

const HELPER_SOURCE: &str = include_str!("url_helper.js");

const INTERFACE_SOURCE: &str = "(function(){\
 try { new URL('http://e/'); } catch(e) {}\
 try { Object.defineProperty(globalThis, 'URL', { enumerable: false }); } catch(e) {}\
 try { Object.defineProperty(URL, 'prototype', { writable: false }); } catch(e) {}\
})()";

fn set_parts(scope: &mut Scope<'_>, object: &Value, parts: &UrlParts) {
    set_str(scope, object, "href", &parts.href);
    set_str(scope, object, "protocol", &parts.protocol);
    set_str(scope, object, "host", &parts.host);
    set_str(scope, object, "hostname", &parts.hostname);
    set_str(scope, object, "port", &parts.port);
    set_str(scope, object, "origin", &parts.origin);
    set_str(scope, object, "pathname", &parts.pathname);
    set_str(scope, object, "search", &parts.search);
    set_str(scope, object, "hash", &parts.hash);
    set_str(scope, object, "username", &parts.username);
    set_str(scope, object, "password", &parts.password);
}

fn parts_object(scope: &mut Scope<'_>, href: &[u8]) -> Value {
    let Some(parts) = ffi::url_parts(href) else {
        return Value::null();
    };
    let object = scope.new_object();
    set_parts(scope, &object, &parts);
    object
}

fn helper_parts(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(href) = args.first() else {
        return Ok(Value::null());
    };
    match scope.to_bytes(href) {
        Ok(href) => Ok(parts_object(scope, &href)),
        Err(_) => Ok(Value::null()),
    }
}

fn helper_set(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 3 {
        return Ok(Value::null());
    }
    let href = scope.to_bytes(&args[0]);
    let component = scope.to_bytes(&args[1]);
    let value = scope.to_bytes(&args[2]);
    let (Ok(href), Ok(component), Ok(value)) = (href, component, value) else {
        return Ok(Value::null());
    };
    Ok(match ffi::url_set_component(&href, &component, &value) {
        Some(next) => parts_object(scope, &next),
        None => Value::null(),
    })
}

fn resolve_args(scope: &mut Scope<'_>, args: &[Value], raw: &[u8]) -> JsResult<Vec<u8>> {
    let base = arg(args, 1);
    if args.len() >= 2 && !is_nullish(&base) {
        let resolved = match scope.to_bytes(&base) {
            Ok(base) => ffi::url_resolve(Some(&base), raw),
            Err(_) => None,
        };
        return resolved.ok_or_else(|| scope.type_error("URL: invalid url"));
    }
    ffi::url_resolve(None, raw)
        .ok_or_else(|| scope.type_error("URL: invalid or relative URL requires a base"))
}

fn data_object(scope: &mut Scope<'_>, resolved: &[u8]) -> Value {
    let object = scope.new_object();
    match ffi::url_parts(resolved) {
        Some(parts) => {
            set_parts(scope, &object, &parts);
            let query = parts.search.get(1..).unwrap_or_default();
            let params = crate::search_params::from_query(scope, query);
            set(scope, &object, "searchParams", params);
        }
        None => {
            set_str(scope, &object, "href", resolved);
            for key in ["protocol", "host", "hostname", "port"] {
                set_str(scope, &object, key, b"");
            }
            set_str(scope, &object, "origin", b"null");
            set_str(scope, &object, "pathname", b"/");
            for key in ["search", "hash", "username", "password"] {
                set_str(scope, &object, key, b"");
            }
            let params = crate::search_params::from_query(scope, b"");
            set(scope, &object, "searchParams", params);
        }
    }
    object
}

fn helper(scope: &mut Scope<'_>, js: Js) -> Option<Value> {
    if let Some(helper) = with_page(js, |page| page.url_helper.clone()) {
        return Some(helper);
    }
    let factory = scope
        .eval_native_script(HELPER_SOURCE, "<url-helper>")
        .ok()?;
    let parts = scope.function("parts", 1, helper_parts);
    let set = scope.function("set", 3, helper_set);
    let helper = scope
        .call(&factory, &Value::undefined(), &[parts, set])
        .ok()?;
    with_page(js, |page| page.url_helper = Some(helper.clone()));
    Some(helper)
}

fn wrap_instance(
    scope: &mut Scope<'_>,
    helper: &Value,
    object: Value,
    new_target: &Value,
) -> Value {
    let proto = if new_target.is_object() {
        crate::get(scope, new_target, "prototype")
    } else {
        Value::undefined()
    };
    scope
        .call(helper, &Value::undefined(), &[object.clone(), proto])
        .unwrap_or(object)
}

fn construct(scope: &mut Scope<'_>, new_target: &Value, args: &[Value]) -> JsResult {
    let Some(input) = args.first() else {
        return Err(scope.type_error("URL: requires a url string"));
    };
    let Ok(raw) = scope.to_bytes(input) else {
        return Err(scope.type_error("URL: invalid url argument"));
    };
    let resolved = resolve_args(scope, args, &raw)?;
    let object = data_object(scope, &resolved);
    let js = Js::of(scope);
    if js.is_null() {
        return Ok(object);
    }
    match helper(scope, js) {
        Some(helper) => Ok(wrap_instance(scope, &helper, object, new_target)),
        None => Ok(object),
    }
}

pub(crate) fn url_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !this.is_object() {
        return Err(scope.type_error(
            "Failed to construct 'URL': Please use the 'new' operator, this DOM object \
             constructor cannot be called as a function.",
        ));
    }
    construct(scope, this, args)
}

pub(crate) fn can_parse(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("URL.canParse: 1 argument required"));
    }
    Ok(Value::boolean(
        construct(scope, &Value::undefined(), args).is_ok(),
    ))
}

pub(crate) fn parse_static(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("URL.parse: 1 argument required"));
    }
    Ok(construct(scope, &Value::undefined(), args).unwrap_or_else(|_| Value::null()))
}

pub(crate) fn install_interface(scope: &mut Scope<'_>) {
    let _ = scope.eval_native_script(INTERFACE_SOURCE, "<url-iface>");
}
