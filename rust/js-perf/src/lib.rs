//! Southstar — the Performance API: performance.*, resource timing, the page and frame clocks and PerformanceObserver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod entry;
mod ffi;
mod timeline;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use southstar_js_engine::quickjs;
use southstar_js_engine::{NativeFn, Scope, Trace, Value};

use crate::entry::{Entry, Fetch, ResourceInfo, Response, relative_ms};
use crate::ffi::Js;
use crate::timeline::{Observer, ObserverState};

const SUPPORTED_ENTRY_TYPES: [&str; 5] = ["mark", "measure", "navigation", "paint", "resource"];

const DEFAULT_HEAP_LIMIT: i64 = 128 * 1024 * 1024;

const FIRST_PAINT_MS: f64 = 60.0;

const FIRST_CONTENTFUL_PAINT_MS: f64 = 65.0;

struct PerformanceData {
    realm: usize,
    objects: RefCell<[Value; 3]>,
}

#[derive(Clone)]
struct PerformanceHandle(Rc<PerformanceData>);

impl Trace for PerformanceHandle {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        if let Ok(objects) = self.0.objects.try_borrow() {
            objects.iter().for_each(visit);
        }
    }
}

#[derive(Clone)]
struct ObserverHandle(Rc<Observer>);

fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

fn text_of(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.to_bytes(value).ok().map(c_text)
}

fn realm_key(js: Js, timeline: usize) -> usize {
    if !js.is_null() && timeline == ffi::main_realm(js) {
        0
    } else {
        timeline
    }
}

fn performance_of(scope: &mut Scope<'_>, this: &Value) -> Option<Rc<PerformanceData>> {
    scope
        .host_data::<PerformanceHandle>(this)
        .map(|handle| handle.0)
}

fn observer_of(scope: &mut Scope<'_>, this: &Value) -> Option<Rc<Observer>> {
    scope
        .host_data::<ObserverHandle>(this)
        .map(|handle| handle.0)
}

fn this_realm(scope: &mut Scope<'_>, js: Js, this: &Value) -> usize {
    let realm = performance_of(scope, this).map_or(0, |data| data.realm);
    realm_key(js, realm)
}

fn illegal_invocation(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    set(scope, object, key, value);
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

fn bind_if_not_callable(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: NativeFn,
) {
    let current = scope.get(object, name).ok();
    if !current.is_some_and(|current| scope.is_function(&current)) {
        bind(scope, object, name, arity, f);
    }
}

fn bind_to_json(scope: &mut Scope<'_>, object: &Value) {
    let function = ffi::own_data_props_to_json(scope);
    set(scope, object, "toJSON", function);
}

fn adopt_global_prototype(scope: &mut Scope<'_>, object: &Value, interface: &str) {
    let global = scope.global();
    let ctor = scope
        .get(&global, interface)
        .unwrap_or_else(|_| Value::undefined());
    let proto = if ctor.is_object() {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    if proto.is_object() {
        let _ = scope.set_prototype(object, &proto);
    }
}

fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let Ok(length) = scope.get(array, "length") else {
        return 0;
    };
    scope.to_int32(&length).map_or(0, |n| n as u32)
}

fn bool_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    match scope.get(object, key) {
        Ok(value) => !value.is_undefined() && scope.to_bool(&value),
        Err(_) => false,
    }
}

fn entry_to_js(scope: &mut Scope<'_>, entry: &Entry) -> Value {
    let o = scope.new_object();
    set_str(scope, &o, "name", &entry.name);
    set_str(scope, &o, "entryType", &entry.kind);
    set(scope, &o, "startTime", Value::number(entry.start_time));
    set(scope, &o, "duration", Value::number(entry.duration));
    if entry.is(b"resource") {
        let end = entry.start_time + entry.duration;
        let initiator = entry.initiator_type.as_deref().unwrap_or(b"other");
        set_str(scope, &o, "initiatorType", initiator);
        let protocol = entry.next_hop_protocol.as_deref().unwrap_or_default();
        set_str(scope, &o, "nextHopProtocol", protocol);
        set(scope, &o, "workerStart", Value::number(0.0));
        set(scope, &o, "redirectStart", Value::number(0.0));
        set(scope, &o, "redirectEnd", Value::number(0.0));
        for (index, (name, value)) in entry.phases.named().into_iter().enumerate() {
            let value = if entry.has_timing {
                value
            } else if index >= 7 {
                end
            } else {
                entry.start_time
            };
            set(scope, &o, name, Value::number(value));
        }
        set(scope, &o, "transferSize", Value::int64(entry.transfer_size));
        set(
            scope,
            &o,
            "encodedBodySize",
            Value::int64(entry.encoded_size),
        );
        set(
            scope,
            &o,
            "decodedBodySize",
            Value::int64(entry.encoded_size),
        );
        let server_timing = scope.new_array();
        set(scope, &o, "serverTiming", server_timing);
        set(
            scope,
            &o,
            "responseStatus",
            Value::int(entry.response_status),
        );
        let blocking: &[u8] = if entry.render_blocking {
            b"blocking"
        } else {
            b"non-blocking"
        };
        set_str(scope, &o, "renderBlockingStatus", blocking);
        adopt_global_prototype(scope, &o, "PerformanceResourceTiming");
    }
    bind_to_json(scope, &o);
    o
}

fn navigation_entry(scope: &mut Scope<'_>, js: Js) -> Value {
    let o = scope.new_object();
    let url = if js.is_null() {
        Vec::new()
    } else {
        ffi::current_url(js)
    };
    let t = ffi::navigation_timing(js).unwrap_or_default();
    let complete = t.load_event_end_ms;
    set_str(scope, &o, "name", &url);
    set_str(scope, &o, "entryType", b"navigation");
    set(scope, &o, "startTime", Value::number(0.0));
    set(scope, &o, "duration", Value::number(complete));
    set_str(scope, &o, "type", b"navigate");
    set_str(scope, &o, "initiatorType", b"navigation");
    set_str(scope, &o, "nextHopProtocol", b"h2");
    set(scope, &o, "redirectCount", Value::int(0));
    set(scope, &o, "workerStart", Value::number(0.0));
    let fields = [
        ("unloadEventStart", 0.0),
        ("unloadEventEnd", 0.0),
        ("redirectStart", 0.0),
        ("redirectEnd", 0.0),
        ("fetchStart", 0.0),
        ("domainLookupStart", t.domain_lookup_start_ms),
        ("domainLookupEnd", t.domain_lookup_end_ms),
        ("connectStart", t.connect_start_ms),
        ("connectEnd", t.connect_end_ms),
        ("secureConnectionStart", t.secure_connection_start_ms),
        ("requestStart", t.request_start_ms),
        ("responseStart", t.response_start_ms),
        ("responseEnd", t.response_end_ms),
        ("domLoading", t.dom_loading_ms),
        ("domInteractive", t.dom_interactive_ms),
        (
            "domContentLoadedEventStart",
            t.dom_content_loaded_event_start_ms,
        ),
        (
            "domContentLoadedEventEnd",
            t.dom_content_loaded_event_end_ms,
        ),
        ("domComplete", t.dom_complete_ms),
        ("loadEventStart", t.load_event_start_ms),
        ("loadEventEnd", complete),
    ];
    for (key, value) in fields {
        set(scope, &o, key, Value::number(value));
    }
    set(scope, &o, "transferSize", Value::int64(0));
    set(scope, &o, "encodedBodySize", Value::int64(0));
    set(scope, &o, "decodedBodySize", Value::int64(0));
    let server_timing = scope.new_array();
    set(scope, &o, "serverTiming", server_timing);
    set(scope, &o, "responseStatus", Value::int(200));
    bind_to_json(scope, &o);
    o
}

fn paint_entry(scope: &mut Scope<'_>, name: &[u8], start: f64) -> Value {
    let o = scope.new_object();
    set_str(scope, &o, "name", name);
    set_str(scope, &o, "entryType", b"paint");
    set(scope, &o, "startTime", Value::number(start));
    set(scope, &o, "duration", Value::number(0.0));
    bind_to_json(scope, &o);
    o
}

fn push_paint_entries(scope: &mut Scope<'_>, array: &Value, out: &mut u32) {
    for (name, start) in [
        (&b"first-paint"[..], FIRST_PAINT_MS),
        (&b"first-contentful-paint"[..], FIRST_CONTENTFUL_PAINT_MS),
    ] {
        let entry = paint_entry(scope, name, start);
        let _ = scope.set_index(array, *out, entry);
        *out += 1;
    }
}

fn push_entry(scope: &mut Scope<'_>, array: &Value, out: &mut u32, entry: &Entry) {
    let value = entry_to_js(scope, entry);
    let _ = scope.set_index(array, *out, value);
    *out += 1;
}

fn records_to_array(scope: &mut Scope<'_>, records: &[Entry]) -> Value {
    let array = scope.new_array();
    let mut out = 0;
    for record in records {
        push_entry(scope, &array, &mut out, record);
    }
    array
}

fn push_to_timeline(js: Js, entry: Entry) {
    let Some(timeline) = timeline::timeline(js) else {
        return;
    };
    let queued = entry.clone();
    if timeline.push(entry) {
        timeline::queue(js, &queued);
    }
}

pub(crate) fn add_resource_timed(
    js: Js,
    info: &ResourceInfo<'_>,
    fetch: &Fetch<'_>,
    resp: Option<&Response<'_>>,
) {
    let Some(timeline) = timeline::timeline(js) else {
        return;
    };
    if !timeline.has_entries() || entry::untimed(fetch.url) {
        return;
    }
    let realm = realm_key(js, info.timeline);
    let origin = timeline::time_origin_us(js, realm);
    push_to_timeline(js, entry::resource_entry(realm, origin, info, fetch, resp));
}

pub(crate) fn has_resource(js: Js, timeline: usize, url: &[u8], initiator: Option<&[u8]>) -> bool {
    let key = realm_key(js, timeline);
    timeline::timeline(js).is_some_and(|t| {
        t.any(|e| {
            e.realm == key
                && e.is(b"resource")
                && e.name == url
                && e.initiator_type.as_deref() == initiator
        })
    })
}

pub(crate) fn move_timeline(js: Js, from: usize, to: usize) {
    if from == 0 {
        return;
    }
    let key = realm_key(js, to);
    if let Some(timeline) = timeline::timeline(js) {
        timeline.move_realm(from, key);
    }
}

pub(crate) fn now_ms(js: Js) -> f64 {
    let origin = if js.is_null() {
        0
    } else {
        ffi::page_time_origin_us(js)
    };
    relative_ms(ffi::monotonic_us(), origin)
}

pub(crate) fn realm_now_ms(js: Js, realm: usize) -> f64 {
    relative_ms(ffi::monotonic_us(), timeline::time_origin_us(js, realm))
}

pub(crate) fn new_performance_object(scope: &mut Scope<'_>) -> Value {
    let js = ffi::js_of(scope);
    let data = PerformanceData {
        realm: realm_key(js, ffi::realm_of(scope)),
        objects: RefCell::new([Value::undefined(), Value::undefined(), Value::undefined()]),
    };
    scope.new_traced_host_object(None, PerformanceHandle(Rc::new(data)))
}

pub(crate) fn set_performance_objects(scope: &mut Scope<'_>, perf: &Value, objects: [Value; 3]) {
    if let Some(data) = performance_of(scope, perf) {
        let previous = data.objects.replace(objects);
        drop(previous);
    }
}

pub(crate) fn time_origin_get(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let Some(data) = performance_of(scope, this) else {
        return Err(illegal_invocation(scope));
    };
    let js = ffi::js_of(scope);
    Ok(Value::number(timeline::time_origin_real_ms(js, data.realm)))
}

pub(crate) fn object_get(
    scope: &mut Scope<'_>,
    this: &Value,
    which: usize,
) -> Result<Value, Value> {
    let Some(data) = performance_of(scope, this) else {
        return Err(illegal_invocation(scope));
    };
    let objects = data.objects.borrow();
    Ok(objects[which.min(2)].clone())
}

pub(crate) fn performance_now(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let Some(data) = performance_of(scope, this) else {
        return Err(illegal_invocation(scope));
    };
    let js = ffi::js_of(scope);
    Ok(Value::number(realm_now_ms(js, data.realm)))
}

fn mark_like_result(
    scope: &mut Scope<'_>,
    kind: &[u8],
    name: &[u8],
    start: f64,
    duration: f64,
) -> Value {
    let r = scope.new_object();
    set_str(scope, &r, "name", name);
    set_str(scope, &r, "entryType", kind);
    set(scope, &r, "startTime", Value::number(start));
    set(scope, &r, "duration", Value::number(duration));
    r
}

pub(crate) fn mark(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let name = match args.first() {
        Some(name) => text_of(scope, name),
        None => None,
    };
    let name = name.unwrap_or_default();
    let realm = this_realm(scope, js, this);
    let t = realm_now_ms(js, realm);
    push_to_timeline(js, Entry::simple(realm, b"mark", &name, t, 0.0));
    Ok(mark_like_result(scope, b"mark", &name, t, 0.0))
}

fn lookup_mark(js: Js, realm: usize, name: &[u8]) -> Option<f64> {
    timeline::timeline(js)?
        .latest(|e| e.realm == realm && e.is(b"mark") && e.name == name)
        .map(|e| e.start_time)
}

fn resolve_time(
    scope: &mut Scope<'_>,
    value: &Value,
    js: Js,
    realm: usize,
    fallback: f64,
) -> Option<f64> {
    if value.is_undefined() || value.is_null() {
        return Some(fallback);
    }
    if value.is_number() {
        return scope.to_number(value).ok();
    }
    if value.is_string() {
        let name = text_of(scope, value)?;
        return Some(lookup_mark(js, realm, &name).unwrap_or(fallback));
    }
    Some(fallback)
}

pub(crate) fn measure(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let realm = this_realm(scope, js, this);
    let name = match args.first() {
        Some(name) => text_of(scope, name),
        None => None,
    };
    let name = name.unwrap_or_default();
    let end_time = realm_now_ms(js, realm);
    let undefined = Value::undefined();
    let start_value = args.get(1).unwrap_or(&undefined);
    let end_value = args.get(2).unwrap_or(&undefined);
    let start_time = resolve_time(scope, start_value, js, realm, 0.0).unwrap_or(0.0);
    let resolved_end = resolve_time(scope, end_value, js, realm, end_time).unwrap_or(end_time);
    let duration = resolved_end - start_time;
    let duration = if duration < 0.0 { 0.0 } else { duration };
    push_to_timeline(
        js,
        Entry::simple(realm, b"measure", &name, start_time, duration),
    );
    Ok(mark_like_result(
        scope, b"measure", &name, start_time, duration,
    ))
}

fn clear(js: Js, realm: usize, kind: &[u8], name: Option<&[u8]>) {
    if let Some(timeline) = timeline::timeline(js) {
        timeline
            .retain(|e| !(e.realm == realm && e.kind == kind && name.is_none_or(|n| e.name == n)));
    }
}

fn string_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Option<Vec<u8>> {
    let value = args.get(index).filter(|value| value.is_string())?;
    text_of(scope, value)
}

pub(crate) fn clear_marks(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let name = string_arg(scope, args, 0);
    let realm = this_realm(scope, js, this);
    clear(js, realm, b"mark", name.as_deref());
    Ok(Value::undefined())
}

pub(crate) fn clear_measures(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let name = string_arg(scope, args, 0);
    let realm = this_realm(scope, js, this);
    clear(js, realm, b"measure", name.as_deref());
    Ok(Value::undefined())
}

pub(crate) fn clear_resource_timings(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let realm = this_realm(scope, js, this);
    clear(js, realm, b"resource", None);
    Ok(Value::undefined())
}

fn timeline_entries(js: Js, realm: usize) -> Vec<Entry> {
    timeline::timeline(js).map_or_else(Vec::new, |t| t.entries_in(realm))
}

pub(crate) fn get_entries(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let array = scope.new_array();
    if js.is_null() {
        return Ok(array);
    }
    let realm = this_realm(scope, js, this);
    let mut out = 0;
    let navigation = navigation_entry(scope, js);
    let _ = scope.set_index(&array, out, navigation);
    out += 1;
    push_paint_entries(scope, &array, &mut out);
    for entry in timeline_entries(js, realm) {
        push_entry(scope, &array, &mut out, &entry);
    }
    Ok(array)
}

pub(crate) fn get_entries_by_name(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let array = scope.new_array();
    let has_timeline = timeline::timeline(js).is_some_and(|t| t.has_entries());
    let Some(first) = args.first().filter(|_| has_timeline) else {
        return Ok(array);
    };
    let realm = this_realm(scope, js, this);
    let name = text_of(scope, first);
    let kind = string_arg(scope, args, 1);
    let mut out = 0;
    for entry in timeline_entries(js, realm) {
        if name.as_ref().is_some_and(|name| entry.name != *name)
            || kind.as_ref().is_some_and(|kind| entry.kind != *kind)
        {
            continue;
        }
        push_entry(scope, &array, &mut out, &entry);
    }
    Ok(array)
}

pub(crate) fn get_entries_by_type(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let array = scope.new_array();
    let Some(first) = args.first().filter(|_| !js.is_null()) else {
        return Ok(array);
    };
    let realm = this_realm(scope, js, this);
    let kind = text_of(scope, first);
    let mut out = 0;
    match kind.as_deref() {
        Some(b"navigation") => {
            let navigation = navigation_entry(scope, js);
            let _ = scope.set_index(&array, out, navigation);
            out += 1;
        }
        Some(b"paint") => push_paint_entries(scope, &array, &mut out),
        _ => {}
    }
    for entry in timeline_entries(js, realm) {
        if kind.as_ref().is_some_and(|kind| entry.kind != *kind) {
            continue;
        }
        push_entry(scope, &array, &mut out, &entry);
    }
    Ok(array)
}

pub(crate) fn memory_get(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let usage = quickjs::memory_usage(scope);
    let used = usage.memory_used_size.max(0);
    let total = if usage.malloc_size > 0 {
        usage.malloc_size
    } else {
        used
    }
    .max(used);
    let limit = if usage.malloc_limit > 0 {
        usage.malloc_limit
    } else {
        DEFAULT_HEAP_LIMIT
    };
    let memory = scope.new_object();
    set(
        scope,
        &memory,
        "jsHeapSizeLimit",
        Value::number(limit as f64),
    );
    set(
        scope,
        &memory,
        "totalJSHeapSize",
        Value::number(total as f64),
    );
    set(scope, &memory, "usedJSHeapSize", Value::number(used as f64));
    Ok(memory)
}

pub(crate) fn supported_entry_types(scope: &mut Scope<'_>) -> Value {
    let array = scope.new_array();
    for (index, kind) in SUPPORTED_ENTRY_TYPES.iter().enumerate() {
        let value = scope.string(kind);
        let _ = scope.set_index(&array, index as u32, value);
    }
    array
}

fn list_entries(scope: &mut Scope<'_>, this: &Value) -> Option<Value> {
    scope.get(this, "_entries").ok().filter(Value::is_object)
}

fn entry_matches(
    scope: &mut Scope<'_>,
    entry: &Value,
    name: Option<&[u8]>,
    kind: Option<&[u8]>,
) -> bool {
    for (key, wanted) in [("name", name), ("entryType", kind)] {
        let Some(wanted) = wanted else {
            continue;
        };
        let actual = scope
            .get(entry, key)
            .ok()
            .and_then(|value| text_of(scope, &value));
        if actual.as_deref() != Some(wanted) {
            return false;
        }
    }
    true
}

fn filter_list(
    scope: &mut Scope<'_>,
    this: &Value,
    name: Option<&[u8]>,
    kind: Option<&[u8]>,
) -> Value {
    let out = scope.new_array();
    let Some(entries) = list_entries(scope, this) else {
        return out;
    };
    let length = array_length(scope, &entries);
    let mut index = 0;
    for i in 0..length {
        let Ok(entry) = scope.get_index(&entries, i) else {
            continue;
        };
        if entry_matches(scope, &entry, name, kind) {
            let _ = scope.set_index(&out, index, entry);
            index += 1;
        }
    }
    out
}

fn list_get_entries(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let out = scope.new_array();
    let Some(entries) = list_entries(scope, this) else {
        return Ok(out);
    };
    let length = array_length(scope, &entries);
    for i in 0..length {
        if let Ok(entry) = scope.get_index(&entries, i) {
            let _ = scope.set_index(&out, i, entry);
        }
    }
    Ok(out)
}

fn list_get_entries_by_name(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let name = match args.first() {
        Some(name) => text_of(scope, name),
        None => None,
    };
    let kind = string_arg(scope, args, 1);
    let Some(name) = name else {
        let out = scope.new_array();
        let _ = list_entries(scope, this);
        return Ok(out);
    };
    Ok(filter_list(scope, this, Some(&name), kind.as_deref()))
}

fn list_get_entries_by_type(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let kind = match args.first() {
        Some(kind) => text_of(scope, kind),
        None => None,
    };
    let Some(kind) = kind else {
        let out = scope.new_array();
        let _ = list_entries(scope, this);
        return Ok(out);
    };
    Ok(filter_list(scope, this, None, Some(&kind)))
}

const LIST_METHODS: [(&str, u32, NativeFn); 3] = [
    ("getEntries", 0, list_get_entries),
    ("getEntriesByName", 2, list_get_entries_by_name),
    ("getEntriesByType", 1, list_get_entries_by_type),
];

fn entry_list_from_array(scope: &mut Scope<'_>, entries: &Value) -> Value {
    let list = scope.new_object();
    adopt_global_prototype(scope, &list, "PerformanceObserverEntryList");
    set(scope, &list, "_entries", entries.clone());
    for (name, arity, f) in LIST_METHODS {
        bind(scope, &list, name, arity, f);
    }
    list
}

pub(crate) fn install_entry_list(scope: &mut Scope<'_>, global: &Value) {
    let ctor = scope
        .get(global, "PerformanceObserverEntryList")
        .unwrap_or_else(|_| Value::undefined());
    let proto = if ctor.is_object() {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    if proto.is_object() {
        for (name, arity, f) in LIST_METHODS {
            bind_if_not_callable(scope, &proto, name, arity, f);
        }
    }
}

const OBSERVER_METHODS: [(&str, u32, NativeFn); 3] = [
    ("observe", 1, observe),
    ("disconnect", 0, disconnect),
    ("takeRecords", 0, take_records),
];

pub(crate) fn observer_ctor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let proto = scope
        .get(new_target, "prototype")
        .ok()
        .filter(Value::is_object);
    let callback = args.first().filter(|cb| scope.is_function(cb)).cloned();
    let callback_realm = match &callback {
        Some(cb) => quickjs::function_realm(scope, cb).map_or(0, |realm| realm as usize),
        None => 0,
    };
    let observer = Rc::new(Observer {
        js,
        realm: realm_key(js, callback_realm),
        callback,
        wrapper: Cell::new(None),
        state: RefCell::new(ObserverState::default()),
    });
    let object = scope.new_host_object(proto.as_ref(), ObserverHandle(observer.clone()));
    match &proto {
        Some(proto) => {
            if !matches!(scope.has_property(proto, "observe"), Ok(true)) {
                for (name, arity, f) in OBSERVER_METHODS {
                    bind(scope, proto, name, arity, f);
                }
            }
        }
        None => {
            for (name, arity, f) in OBSERVER_METHODS {
                bind(scope, &object, name, arity, f);
            }
        }
    }
    observer.wrapper.set(Some(ffi::raw_object(&object)));
    for (name, arity, f) in OBSERVER_METHODS {
        bind_if_not_callable(scope, &object, name, arity, f);
    }
    if !js.is_null() {
        timeline::timeline_or_new(js).add_observer(&observer);
    }
    Ok(object)
}

fn add_entry_type(types: &mut Vec<Vec<u8>>, kind: Vec<u8>) {
    if !kind.is_empty() && !types.contains(&kind) {
        types.push(kind);
    }
}

fn collect_types(scope: &mut Scope<'_>, options: &Value) -> Vec<Vec<u8>> {
    let mut types = Vec::new();
    let kind = scope
        .get(options, "type")
        .ok()
        .filter(Value::is_string)
        .and_then(|kind| text_of(scope, &kind));
    if let Some(kind) = kind {
        add_entry_type(&mut types, kind);
    }
    let entry_types = scope
        .get(options, "entryTypes")
        .ok()
        .filter(Value::is_object);
    if let Some(entry_types) = entry_types {
        let length = array_length(scope, &entry_types);
        for i in 0..length {
            let kind = scope
                .get_index(&entry_types, i)
                .ok()
                .and_then(|value| text_of(scope, &value));
            if let Some(kind) = kind {
                add_entry_type(&mut types, kind);
            }
        }
    }
    types
}

pub(crate) fn observe(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let observer = observer_of(scope, this);
    let js = ffi::js_of(scope);
    let (Some(observer), Some(options)) = (observer, args.first().filter(|o| o.is_object())) else {
        return Err(scope.type_error("PerformanceObserver.observe: options required"));
    };
    observer.state.borrow_mut().entry_types.clear();
    let types = collect_types(scope, options);
    if types.is_empty() {
        return Err(scope.type_error("PerformanceObserver.observe: type or entryTypes required"));
    }
    {
        let mut state = observer.state.borrow_mut();
        state.entry_types = types;
        state.disconnected = false;
    }
    observer.pin(|| ffi::wrapper_value(scope, &observer));
    let Some(timeline) = timeline::timeline(js).filter(|t| t.has_entries()) else {
        return Ok(Value::undefined());
    };
    if bool_prop(scope, options, "buffered") {
        for entry in timeline.entries() {
            if entry.realm == observer.realm && observer.wants(&entry.kind) {
                observer.record(entry);
            }
        }
        if !observer.state.borrow().records.is_empty() {
            timeline::schedule_drain(js, &timeline);
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn disconnect(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    if let Some(observer) = observer_of(scope, this) {
        observer.disconnect();
    }
    Ok(Value::undefined())
}

pub(crate) fn take_records(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let Some(observer) = observer_of(scope, this) else {
        return Ok(scope.new_array());
    };
    let records = observer.take_records();
    Ok(records_to_array(scope, &records))
}

pub(crate) fn drain(scope: &mut Scope<'_>) {
    let js = ffi::js_of(scope);
    let Some(timeline) = timeline::timeline(js) else {
        return;
    };
    timeline.take_drain();
    let mut index = 0;
    while let Some(observer) = timeline.observer_at(index) {
        index += 1;
        let Some(observer) = observer else {
            continue;
        };
        let Some(callback) = observer.callback.clone() else {
            continue;
        };
        {
            let state = observer.state.borrow();
            if state.disconnected || state.records.is_empty() {
                continue;
            }
        }
        let records = observer.take_records();
        let wrapper = ffi::wrapper_value(scope, &observer).unwrap_or_else(Value::undefined);
        let array = records_to_array(scope, &records);
        let list = entry_list_from_array(scope, &array);
        let Err(exception) = scope.call(&callback, &wrapper, &[list, wrapper.clone()]) else {
            continue;
        };
        if let Some(message) = text_of(scope, &exception) {
            let mut line = b"JS error in PerformanceObserver: ".to_vec();
            line.extend_from_slice(&message);
            ffi::log_line(js, &line);
        }
    }
}
