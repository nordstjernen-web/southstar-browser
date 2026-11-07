//! Southstar — MessagePort, MessageChannel and BroadcastChannel, and the dedicated, shared and service workers that carry messages between runtimes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod broadcast;
pub(crate) mod ffi;
mod ports;
mod scope;
mod service;
mod shared;
mod worker;

pub use ffi::install_runtime_hooks;

pub(crate) use southstar_js_engine::props::{
    arg, bind, bind_if_not_callable, get, get_index, interface_prototype, noop,
    number_length as length, set, set_str, truthy,
};
use southstar_js_engine::{Attributes, Scope, Value};

pub(crate) type JsResult = Result<Value, Value>;

pub(crate) const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) const PLAIN: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn set_index(scope: &mut Scope<'_>, array: &Value, index: u32, value: Value) {
    let _ = scope.set_index(array, index, value);
}

pub(crate) fn push(scope: &mut Scope<'_>, array: &Value, value: Value) {
    let at = length(scope, array);
    set_index(scope, array, at, value);
}

pub(crate) fn text(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    scope.to_string(value).ok()
}
