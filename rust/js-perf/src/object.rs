//! Southstar — the window's performance object: the Performance prototype, performance.timing and performance.navigation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js, NavigationTiming};
use crate::{adopt_global_prototype, bind, bind_to_json, set, timeline};

const GETTER: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) fn own_data_props_to_json(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let out = scope.new_object();
    let Ok(keys) = scope.own_enumerable_keys(this) else {
        return Ok(out);
    };
    for key in keys {
        let Ok(value) = scope.get_key(this, &key) else {
            continue;
        };
        if scope.is_function(&value) {
            continue;
        }
        let _ = scope.define_entry(&out, &key, value);
    }
    Ok(out)
}

fn to_json(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let out = scope.new_object();
    let origin = scope
        .get(this, "timeOrigin")
        .unwrap_or_else(|_| Value::undefined());
    set(scope, &out, "timeOrigin", origin);
    for key in ["timing", "navigation"] {
        let value = scope.get(this, key).unwrap_or_else(|_| Value::undefined());
        if value.is_object() {
            set(scope, &out, key, value);
        }
    }
    Ok(out)
}

fn noop(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn timing_get(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    crate::object_get(scope, this, 0)
}

fn navigation_get(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    crate::object_get(scope, this, 1)
}

fn event_counts_get(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    crate::object_get(scope, this, 2)
}

fn prototype_of(scope: &mut Scope<'_>, global: &Value, interface: &str) -> Value {
    let ctor = scope
        .get(global, interface)
        .unwrap_or_else(|_| Value::undefined());
    if ctor.is_object() {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    }
}

pub(crate) fn extend_event_target(scope: &mut Scope<'_>, global: &Value, performance: &Value) {
    let prototype = prototype_of(scope, global, "EventTarget");
    if prototype.is_object() {
        let _ = scope.set_prototype(performance, &prototype);
    }
}

fn define_getter(scope: &mut Scope<'_>, object: &Value, key: &str, f: NativeFn) {
    let getter = scope.function(&format!("get {key}"), 0, f);
    let _ = scope.define_accessor(object, key, Some(&getter), None, GETTER);
}

fn install_prototype(scope: &mut Scope<'_>, global: &Value) {
    let proto = prototype_of(scope, global, "Performance");
    if !proto.is_object() {
        return;
    }
    extend_event_target(scope, global, &proto);
    let methods: [(&str, u32, NativeFn); 11] = [
        ("now", 0, crate::performance_now),
        ("mark", 1, crate::mark),
        ("measure", 3, crate::measure),
        ("clearMarks", 1, crate::clear_marks),
        ("clearMeasures", 1, crate::clear_measures),
        ("getEntries", 0, crate::get_entries),
        ("getEntriesByName", 2, crate::get_entries_by_name),
        ("getEntriesByType", 1, crate::get_entries_by_type),
        ("clearResourceTimings", 0, crate::clear_resource_timings),
        ("setResourceTimingBufferSize", 1, noop),
        ("toJSON", 0, to_json),
    ];
    for (name, arity, f) in methods {
        bind(scope, &proto, name, arity, f);
    }
    define_getter(scope, &proto, "timeOrigin", crate::time_origin_get);
    define_getter(scope, &proto, "timing", timing_get);
    define_getter(scope, &proto, "navigation", navigation_get);
    define_getter(scope, &proto, "eventCounts", event_counts_get);
    let _ = scope.define_to_string_tag(&proto, "Performance");
}

fn timing_fields(timing: Option<&NavigationTiming>) -> [(&'static str, f64, bool); 21] {
    let t = timing.copied().unwrap_or_default();
    [
        ("navigationStart", 0.0, true),
        ("unloadEventStart", 0.0, false),
        ("unloadEventEnd", 0.0, false),
        ("redirectStart", 0.0, false),
        ("redirectEnd", 0.0, false),
        ("fetchStart", 0.0, true),
        ("domainLookupStart", 0.0, true),
        ("domainLookupEnd", t.domain_lookup_end_ms, true),
        ("connectStart", t.connect_start_ms, true),
        ("connectEnd", t.connect_end_ms, true),
        (
            "secureConnectionStart",
            t.secure_connection_start_ms,
            timing.is_some() && t.secure_connection_start_ms > 0.0,
        ),
        ("requestStart", t.request_start_ms, true),
        ("responseStart", t.response_start_ms, true),
        ("responseEnd", t.response_end_ms, true),
        ("domLoading", 0.0, false),
        ("domInteractive", 0.0, false),
        ("domContentLoadedEventStart", 0.0, false),
        ("domContentLoadedEventEnd", 0.0, false),
        ("domComplete", 0.0, false),
        ("loadEventStart", 0.0, false),
        ("loadEventEnd", 0.0, false),
    ]
}

pub(crate) fn make_performance_object(
    scope: &mut Scope<'_>,
    js: Js,
    include_memory: bool,
) -> Value {
    let global = scope.global();
    install_prototype(scope, &global);
    let performance = crate::new_performance_object(scope);
    adopt_global_prototype(scope, &performance, "Performance");
    let origin_real_ms = timeline::time_origin_real_ms(js, ffi::realm_of(scope));
    let navigation_timing = ffi::navigation_timing(js);

    let timing = scope.new_object();
    for (key, relative_ms, present) in timing_fields(navigation_timing.as_ref()) {
        let value = if present && !js.is_null() {
            (origin_real_ms + relative_ms).floor() as i64
        } else {
            0
        };
        set(scope, &timing, key, Value::int64(value));
    }
    bind_to_json(scope, &timing);
    adopt_global_prototype(scope, &timing, "PerformanceTiming");

    let navigation = scope.new_object();
    set(scope, &navigation, "type", Value::int(0));
    set(scope, &navigation, "redirectCount", Value::int(0));
    bind_to_json(scope, &navigation);
    adopt_global_prototype(scope, &navigation, "PerformanceNavigation");
    let event_counts = scope.new_object();
    crate::set_performance_objects(scope, &performance, [timing, navigation, event_counts]);

    if include_memory {
        define_getter(scope, &performance, "memory", crate::memory_get);
    }
    performance
}
