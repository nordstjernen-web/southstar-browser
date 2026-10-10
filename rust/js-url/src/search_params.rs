//! Southstar — the URLSearchParams interface and the query lists URL objects own.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::Js;
use crate::{JsResult, arg, with_page};

const HELPER_SOURCE: &str = include_str!("search_params_helper.js");

const INTERFACE_SOURCE: &str = "(function(){\
 try { new URLSearchParams(''); } catch(e) {}\
 try { Object.defineProperty(globalThis, 'URLSearchParams', { enumerable: false }); } catch(e) {}\
 try { Object.defineProperty(URLSearchParams, 'prototype', { writable: false }); } catch(e) {}\
})()";

fn helper(scope: &mut Scope<'_>) -> Option<Value> {
    let js = Js::of(scope);
    if js.is_null() {
        return None;
    }
    if let Some(helper) = with_page(js, |page| page.search_params_helper.clone()) {
        return Some(helper);
    }
    let helper = scope
        .eval_native_script(HELPER_SOURCE, "<usp-helper>")
        .ok()?;
    with_page(js, |page| page.search_params_helper = Some(helper.clone()));
    Some(helper)
}

fn make(scope: &mut Scope<'_>, init: Value, proto: Value) -> JsResult {
    match helper(scope) {
        Some(helper) => scope.call(&helper, &Value::undefined(), &[init, proto]),
        None => Ok(scope.new_object()),
    }
}

pub(crate) fn from_query(scope: &mut Scope<'_>, query: &[u8]) -> Value {
    let init = scope.string_from_bytes(query);
    make(scope, init, Value::undefined()).unwrap_or_else(|_| scope.new_object())
}

pub(crate) fn usp_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !this.is_object() {
        return Err(scope.type_error(
            "Failed to construct 'URLSearchParams': Please use the 'new' operator, this DOM \
             object constructor cannot be called as a function.",
        ));
    }
    let proto = crate::get(scope, this, "prototype");
    make(scope, arg(args, 0), proto)
}

pub(crate) fn install_interface(scope: &mut Scope<'_>) {
    let _ = scope.eval_native_script(INTERFACE_SOURCE, "<usp-iface>");
}
