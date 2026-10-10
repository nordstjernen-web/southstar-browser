//! Southstar — the WebCrypto bindings: the crypto object, getRandomValues, randomUUID, crypto.subtle and CryptoKey.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod algorithm;
mod base64url;
mod ffi;
mod install;
mod jwk;
mod key;
mod random;
mod subtle;

use southstar_js_engine::{Attributes, Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const HIDDEN_WRITABLE: Attributes = Attributes::METHOD;

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    set(scope, object, key, value);
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn c_string(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    scope.to_string(value).ok()
}

pub(crate) fn error_with_code(
    scope: &mut Scope<'_>,
    name: &str,
    message: &str,
    code: i32,
) -> Value {
    let error = scope.new_error();
    let name = scope.string(name);
    let _ = scope.define(&error, "name", name, HIDDEN_WRITABLE);
    let message = scope.string(message);
    let _ = scope.define(&error, "message", message, HIDDEN_WRITABLE);
    let _ = scope.define(&error, "code", Value::int(code), HIDDEN_WRITABLE);
    error
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let error = error_with_code(scope, name, message, code);
    let global = scope.global();
    let ctor = get(scope, &global, "DOMException");
    if ctor.is_object() {
        let proto = get(scope, &ctor, "prototype");
        if proto.is_object() {
            let _ = scope.set_prototype(&error, &proto);
        }
    }
    error
}

fn dom_error_name(message: &str) -> Option<(&str, &str)> {
    let name_len = message.find(':').unwrap_or(message.len());
    let name = &message[..name_len];
    let valid = name_len > 5
        && name_len < 64
        && name.ends_with("Error")
        && name.bytes().all(|b| b.is_ascii_alphanumeric());
    if !valid {
        return None;
    }
    let rest = message.get(name_len + 1..).unwrap_or(message);
    Some((name, rest.trim_start_matches(' ')))
}

pub(crate) fn rejection(scope: &mut Scope<'_>, message: &str) -> Value {
    let error = scope.new_error();
    match dom_error_name(message) {
        Some((name, rest)) => {
            set_str(scope, &error, "name", name);
            set_str(scope, &error, "message", rest);
        }
        None => set_str(scope, &error, "message", message),
    }
    error
}
