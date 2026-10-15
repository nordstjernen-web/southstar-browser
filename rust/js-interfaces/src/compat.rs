//! Southstar — the window's legacy and compatibility members: SVGLength and XPathResult constants, the bar props, captureEvents, external, clientInformation and webkitURL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi;

const SVG_LENGTH_TYPES: [(&str, i32); 11] = [
    ("SVG_LENGTHTYPE_UNKNOWN", 0),
    ("SVG_LENGTHTYPE_NUMBER", 1),
    ("SVG_LENGTHTYPE_PERCENTAGE", 2),
    ("SVG_LENGTHTYPE_EMS", 3),
    ("SVG_LENGTHTYPE_EXS", 4),
    ("SVG_LENGTHTYPE_PX", 5),
    ("SVG_LENGTHTYPE_CM", 6),
    ("SVG_LENGTHTYPE_MM", 7),
    ("SVG_LENGTHTYPE_IN", 8),
    ("SVG_LENGTHTYPE_PT", 9),
    ("SVG_LENGTHTYPE_PC", 10),
];

const XPATH_RESULT_TYPES: [(&str, i32); 10] = [
    ("ANY_TYPE", 0),
    ("NUMBER_TYPE", 1),
    ("STRING_TYPE", 2),
    ("BOOLEAN_TYPE", 3),
    ("UNORDERED_NODE_ITERATOR_TYPE", 4),
    ("ORDERED_NODE_ITERATOR_TYPE", 5),
    ("UNORDERED_NODE_SNAPSHOT_TYPE", 6),
    ("ORDERED_NODE_SNAPSHOT_TYPE", 7),
    ("ANY_UNORDERED_NODE_TYPE", 8),
    ("FIRST_ORDERED_NODE_TYPE", 9),
];

const BAR_PROPS: [&str; 6] = [
    "locationbar",
    "menubar",
    "personalbar",
    "scrollbars",
    "statusbar",
    "toolbar",
];

const CONSTANT: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

fn returns_null(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::null())
}

fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn set_if_missing(scope: &mut Scope<'_>, global: &Value, name: &str, value: Value) {
    if !scope.has_property(global, name).unwrap_or(false) {
        let _ = scope.set(global, name, value);
    }
}

fn function_if_missing(scope: &mut Scope<'_>, global: &Value, name: &str) {
    if !scope.has_property(global, name).unwrap_or(false) {
        let function = scope.function(name, 0, noop);
        let _ = scope.set(global, name, function);
    }
}

fn install_svg_length(scope: &mut Scope<'_>, global: &Value) {
    let ctor = get(scope, global, "SVGLength");
    if !ctor.is_object() {
        return;
    }
    let proto = get(scope, &ctor, "prototype");
    for (name, value) in SVG_LENGTH_TYPES {
        let _ = scope.set(&ctor, name, Value::int(value));
        if proto.is_object() {
            let _ = scope.set(&proto, name, Value::int(value));
        }
    }
}

fn install_xpath_result(scope: &mut Scope<'_>, global: &Value) {
    let ctor = get(scope, global, "XPathResult");
    let proto = if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    if !proto.is_object() {
        return;
    }
    for (name, arity) in [("iterateNext", 0), ("snapshotItem", 1)] {
        let function = scope.function(name, arity, returns_null);
        let _ = scope.set(&proto, name, function);
    }
    for (name, value) in XPATH_RESULT_TYPES {
        let _ = scope.define(&ctor, name, Value::int(value), CONSTANT);
        let _ = scope.define(&proto, name, Value::int(value), CONSTANT);
    }
}

pub fn install_window_compat(scope: &mut Scope<'_>, global: &Value) {
    install_svg_length(scope, global);
    install_xpath_result(scope, global);
    ffi::install_event_handler_props(scope, global);
    for name in BAR_PROPS {
        let bar = scope.new_object();
        let _ = scope.set(&bar, "visible", Value::boolean(true));
        set_if_missing(scope, global, name, bar);
    }
    set_if_missing(scope, global, "crossOriginIsolated", Value::boolean(false));
    ffi::hide_shared_array_buffer(scope, global);
    set_if_missing(scope, global, "frameElement", Value::null());
    ffi::install_intl(scope, global);
    ffi::install_temporal(scope, global);
    function_if_missing(scope, global, "captureEvents");
    function_if_missing(scope, global, "releaseEvents");
    let external = scope.new_object();
    set_if_missing(scope, global, "external", external);
    let navigator = get(scope, global, "navigator");
    set_if_missing(scope, global, "clientInformation", navigator);
    let url = get(scope, global, "URL");
    let _ = scope.define(global, "webkitURL", url, Attributes::METHOD);
}
