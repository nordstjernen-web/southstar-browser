//! Southstar — EventTarget objects that are not nodes (their listener arrays, on* handlers and dispatch at the target), dispatchEvent() for every kind of target, and the page's EventTarget bootstrap.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::Cell;

use southstar_js_engine::quickjs::{self, JSContext};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::handlers;
use crate::listeners::{self, parse_options, signal_is_aborted};
use crate::{JsResult, get, is_real_document, same_object, set, truthy, with_key};

const EVENT_TARGET_SOURCE: &str = include_str!("event_target.js");

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

thread_local! {
    static ENGINE_EVENT: Cell<usize> = const { Cell::new(0) };
}

fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let value = get(scope, array, "length");
    scope.to_number(&value).map_or(0, |n| n as u32)
}

fn entry_kind_is(scope: &mut Scope<'_>, entry: &Value, kind: &str) -> bool {
    let value = get(scope, entry, "type");
    scope.to_string(&value).is_ok_and(|text| text == kind)
}

fn entry_signal_aborted(scope: &mut Scope<'_>, entry: &Value) -> bool {
    if !entry.is_object() {
        return false;
    }
    let signal = get(scope, entry, "signal");
    signal.is_object() && truthy(scope, &signal, "aborted")
}

pub(crate) fn make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    let event = ffi::new_event(scope);
    let kind = scope.string(kind);
    set(scope, &event, "type", kind);
    set(scope, &event, "target", target.clone());
    set(scope, &event, "currentTarget", target.clone());
    set(scope, &event, "bubbles", Value::boolean(false));
    set(scope, &event, "cancelable", Value::boolean(false));
    set(scope, &event, "defaultPrevented", Value::boolean(false));
    ffi::bind_event_methods(scope, &event);
    set(scope, &event, "_is_trusted", Value::boolean(true));
    ffi::bind_stop_immediate(scope, &event);
    event
}

pub(crate) fn add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let Some(node) = ffi::unwrap(this) {
        return if is_real_document(node) {
            listeners::document_add(scope, this, args)
        } else {
            listeners::element_add(scope, this, args)
        };
    }
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let kind = scope.to_string(&args[0])?;
    let options = if args.len() >= 3 {
        parse_options(scope, &args[2], true)?
    } else {
        listeners::Options::default()
    };
    let callback = &args[1];
    if !callback.is_object() || signal_is_aborted(scope, options.signal.as_ref()) {
        return Ok(Value::undefined());
    }
    let mut list = get(scope, this, "_listeners");
    if !list.is_array() {
        list = scope.new_array();
        set(scope, this, "_listeners", list.clone());
    }
    let count = length(scope, &list);
    for i in 0..count {
        let entry = scope
            .get_index(&list, i)
            .unwrap_or_else(|_| Value::undefined());
        if !entry.is_object() {
            continue;
        }
        let function = get(scope, &entry, "fn");
        if entry_kind_is(scope, &entry, &kind)
            && same_object(&function, callback)
            && !entry_signal_aborted(scope, &entry)
        {
            return Ok(Value::undefined());
        }
    }
    let entry = scope.new_object();
    let kind = scope.string(&kind);
    set(scope, &entry, "type", kind);
    set(scope, &entry, "fn", callback.clone());
    if options.once {
        set(scope, &entry, "once", Value::boolean(true));
    }
    if options.passive {
        set(scope, &entry, "passive", Value::boolean(true));
    }
    if let Some(signal) = options.signal {
        set(scope, &entry, "signal", signal);
    }
    let _ = scope.set_index(&list, count, entry);
    Ok(Value::undefined())
}

pub(crate) fn remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let Some(node) = ffi::unwrap(this) {
        return if is_real_document(node) {
            listeners::document_remove(scope, this, args)
        } else {
            listeners::element_remove(scope, this, args)
        };
    }
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let kind = scope.to_string(&args[0])?;
    let list = get(scope, this, "_listeners");
    if list.is_array() {
        let count = length(scope, &list);
        let kept = scope.new_array();
        let mut k = 0;
        for i in 0..count {
            let entry = scope
                .get_index(&list, i)
                .unwrap_or_else(|_| Value::undefined());
            let matched = entry.is_object() && {
                let function = get(scope, &entry, "fn");
                entry_kind_is(scope, &entry, &kind) && same_object(&function, &args[1])
            };
            if matched {
                set(scope, &entry, "_dead", Value::boolean(true));
            } else {
                let _ = scope.set_index(&kept, k, entry);
                k += 1;
            }
        }
        set(scope, this, "_listeners", kept);
    }
    Ok(Value::undefined())
}

pub(crate) fn compact_dead(scope: &mut Scope<'_>, owner: &Value) {
    let list = get(scope, owner, "_listeners");
    if !list.is_array() {
        return;
    }
    let count = length(scope, &list);
    let kept = scope.new_array();
    let mut k = 0;
    for i in 0..count {
        let entry = scope
            .get_index(&list, i)
            .unwrap_or_else(|_| Value::undefined());
        if !truthy(scope, &entry, "_dead") {
            let _ = scope.set_index(&kept, k, entry);
            k += 1;
        }
    }
    set(scope, owner, "_listeners", kept);
}

pub(crate) fn handler_realm(
    scope: &mut Scope<'_>,
    obj: &Value,
    kind: &str,
    listener_key: &str,
) -> *mut JSContext {
    let mut function = with_key(&["on", kind], |name| get(scope, obj, name));
    if !scope.is_function(&function) {
        function = Value::undefined();
        let list = get(scope, obj, "_listeners");
        let count = if list.is_array() {
            length(scope, &list)
        } else {
            0
        };
        let mut i = 0;
        while i < count && !scope.is_function(&function) {
            let entry = scope
                .get_index(&list, i)
                .unwrap_or_else(|_| Value::undefined());
            if entry_kind_is(scope, &entry, kind) {
                let callback = get(scope, &entry, listener_key);
                function = if scope.is_function(&callback) {
                    callback
                } else {
                    Value::undefined()
                };
            }
            i += 1;
        }
    }
    ffi::function_realm(scope, &function)
}

pub(crate) fn call_listener(
    scope: &mut Scope<'_>,
    callback: &Value,
    this: &Value,
    event: &Value,
) -> JsResult {
    if !callback.is_object() {
        return Ok(Value::undefined());
    }
    let js = ffi::js_of(scope);
    let is_function = scope.is_function(callback);
    let guard = if js.is_null() {
        None
    } else if is_function {
        handlers::push_current_event(scope, callback, event, false)
    } else {
        let global = scope.global();
        if handlers::global_is_window(scope, &global) {
            Some(handlers::set_current_event(scope, global, event))
        } else {
            None
        }
    };
    let result = if is_function {
        scope.call(callback, this, core::slice::from_ref(event))
    } else {
        match scope.get(callback, "handleEvent") {
            Ok(handle) => scope.call(&handle, callback, core::slice::from_ref(event)),
            Err(exception) => Err(exception),
        }
    };
    handlers::pop_current_event(scope, guard);
    result
}

fn call_listener_native(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    call_listener(scope, &args[0], &args[1], &args[2])
}

fn report_listener_native(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if !js.is_null()
        && let Some(exception) = args.first()
    {
        let url = js.current_url();
        crate::errors::report_exception_at(js, exception, url.as_deref(), 0, 0);
    }
    Ok(Value::undefined())
}

pub(crate) fn dispatch_with_event(scope: &mut Scope<'_>, obj: &Value, kind: &str, event: &Value) {
    let js = ffi::js_of(scope);
    let realm = handler_realm(scope, obj, kind, "fn");
    js.in_realm_scope(realm, || {
        ffi::at_target(scope, event, obj, |scope| {
            run_at_target(js, scope, obj, kind, event)
        });
    });
}

fn report(js: Js, scope: &mut Scope<'_>, kind: &str, exception: &Value) {
    crate::errors::report_handler_exception(js, scope, Some(kind), exception);
}

fn run_at_target(js: Js, scope: &mut Scope<'_>, obj: &Value, kind: &str, event: &Value) {
    let list = get(scope, obj, "_listeners");
    let count = if list.is_array() {
        length(scope, &list)
    } else {
        0
    };
    let handler = with_key(&["on", kind], |name| get(scope, obj, name));
    if scope.is_function(&handler) {
        let result = if js.is_null() {
            scope.call(&handler, obj, core::slice::from_ref(event))
        } else {
            let global = scope.global();
            let is_window = same_object(&global, obj);
            js.scope(|outer| {
                handlers::call_on_handler(outer, &handler, obj, kind, event, is_window).0
            })
            .unwrap_or_else(|| Ok(Value::undefined()))
        };
        if let Err(exception) = result {
            report(js, scope, kind, &exception);
        }
        js.microtask_checkpoint();
    }
    if !list.is_array() {
        return;
    }
    let mut any_dead = false;
    for i in 0..count {
        let entry = scope
            .get_index(&list, i)
            .unwrap_or_else(|_| Value::undefined());
        if entry.is_object()
            && entry_kind_is(scope, &entry, kind)
            && !truthy(scope, &entry, "_dead")
        {
            let skip = entry_signal_aborted(scope, &entry);
            let once = truthy(scope, &entry, "once");
            if once || skip {
                set(scope, &entry, "_dead", Value::boolean(true));
                any_dead = true;
            }
            if !skip {
                let function = get(scope, &entry, "fn");
                if function.is_object() {
                    let passive = truthy(scope, &entry, "passive");
                    if passive {
                        set(scope, event, "_passive_active", Value::boolean(true));
                    }
                    let result = call_listener(scope, &function, obj, event);
                    if passive {
                        set(scope, event, "_passive_active", Value::boolean(false));
                    }
                    if let Err(exception) = result {
                        report(js, scope, kind, &exception);
                    }
                    js.microtask_checkpoint();
                }
            }
        }
        if truthy(scope, event, "_immediate_stopped") {
            break;
        }
    }
    if any_dead {
        compact_dead(scope, obj);
    }
}

fn dispatch_guard(scope: &mut Scope<'_>, event: &Value) -> JsResult<()> {
    let dispatching = truthy(scope, event, "_dispatching");
    let initialized = get(scope, event, "_initialized");
    let uninitialized = initialized.is_bool() && !scope.to_bool(&initialized);
    if dispatching || uninitialized {
        return Err(ffi::invalid_state(
            scope,
            c"Event is already being dispatched or was not initialized",
        ));
    }
    Ok(())
}

fn event_argument(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    match args.first() {
        Some(event) if event.is_object() => Ok(event.clone()),
        _ => Err(scope.type_error("dispatchEvent: argument is not an Event")),
    }
}

fn event_kind(scope: &mut Scope<'_>, event: &Value) -> JsResult<String> {
    let kind = get(scope, event, "type");
    scope.to_string(&kind)
}

fn default_false(scope: &mut Scope<'_>, event: &Value, key: &str) {
    if get(scope, event, key).is_undefined() {
        set(scope, event, key, Value::boolean(false));
    }
}

pub(crate) fn dispatch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let event = event_argument(scope, args)?;
    if let Some(node) = ffi::unwrap(this) {
        return if is_real_document(node) {
            document_dispatch(scope, this, args)
        } else {
            element_dispatch(scope, this, args)
        };
    }
    dispatch_guard(scope, &event)?;
    let kind = event_kind(scope, &event)?;
    if quickjs::identity(&event) != ENGINE_EVENT.with(Cell::get) && !ffi::is_host_caller(scope) {
        set(scope, &event, "_is_trusted", Value::boolean(false));
    }
    set(scope, &event, "target", this.clone());
    set(scope, &event, "currentTarget", this.clone());
    default_false(scope, &event, "defaultPrevented");
    dispatch_with_event(scope, this, &kind, &event);
    set(scope, &event, "currentTarget", Value::null());
    let prevented = truthy(scope, &event, "defaultPrevented");
    Ok(Value::boolean(!prevented))
}

pub(crate) fn element_dispatch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let event = event_argument(scope, args)?;
    dispatch_guard(scope, &event)?;
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::boolean(false));
    };
    if js.is_null() {
        return Ok(Value::boolean(false));
    }
    let kind = event_kind(scope, &event)?;
    set(scope, &event, "_is_trusted", Value::boolean(false));
    let wrapped = ffi::wrap(scope, el);
    set(scope, &event, "target", wrapped);
    if !truthy(scope, &event, "defaultPrevented") {
        set(scope, &event, "defaultPrevented", Value::boolean(false));
    }
    default_false_true(scope, &event, "bubbles");
    let is_mouse = truthy(scope, &event, "__ndMouseEvent");
    let activates = is_mouse && kind == "click" && !southstar_dom::tree::effectively_inert(el);
    let act = if !activates {
        None
    } else if ffi::has_activation_behavior(el) {
        Some(el)
    } else if truthy(scope, &event, "bubbles") {
        ffi::click_activation_target(el)
    } else {
        None
    };
    let checkable = act.map_or(0, ffi::checkable_input_kind);
    let state = act
        .filter(|_| checkable != 0)
        .map(|act| js.checkable_pre_click(act, checkable));
    let (_, prevented) = crate::dispatch::dispatch_built(js, el, &kind, event);
    match (act, state) {
        (Some(act), Some(state)) => js.checkable_post_click(act, checkable, &state, prevented),
        (Some(act), None) if !prevented => js.synthetic_activation(act, el),
        _ => {}
    }
    Ok(Value::boolean(!prevented))
}

fn default_false_true(scope: &mut Scope<'_>, event: &Value, key: &str) {
    if get(scope, event, key).is_undefined() {
        set(scope, event, key, Value::boolean(true));
    }
}

pub(crate) fn document_dispatch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let event = event_argument(scope, args)?;
    dispatch_guard(scope, &event)?;
    let js = ffi::js_of(scope);
    let target = ffi::document_root_for(scope, this);
    let Some(target) = target.filter(|_| !js.is_null()) else {
        return Ok(Value::boolean(false));
    };
    let kind = event_kind(scope, &event)?;
    set(scope, &event, "_is_trusted", Value::boolean(false));
    let wrapped = ffi::wrap(scope, target);
    set(scope, &event, "target", wrapped);
    default_false(scope, &event, "_propagation_stopped");
    let (_, prevented) = crate::dispatch::dispatch_built(js, target, &kind, event);
    Ok(Value::boolean(!prevented))
}

pub(crate) fn window_dispatch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let event = event_argument(scope, args)?;
    dispatch_guard(scope, &event)?;
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::boolean(false));
    }
    let kind = event_kind(scope, &event)?;
    if !ffi::is_host_caller(scope) {
        set(scope, &event, "_is_trusted", Value::boolean(false));
    }
    let window = if this.is_object() {
        this.clone()
    } else {
        scope.global()
    };
    set(scope, &event, "target", window);
    default_false(scope, &event, "defaultPrevented");
    default_false(scope, &event, "_propagation_stopped");
    let doc = ffi::window_document_for(scope, this);
    let prevented = crate::dispatch::dispatch_window_only(js, doc, &kind, event);
    Ok(Value::boolean(!prevented))
}

pub(crate) fn fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    loaded: f64,
    total: f64,
    length_computable: bool,
) {
    let event = make_event(scope, target, kind);
    crate::errors::adopt_global_proto(scope, &event, "ProgressEvent");
    set(
        scope,
        &event,
        "lengthComputable",
        Value::boolean(length_computable),
    );
    set(scope, &event, "loaded", Value::number(loaded));
    set(scope, &event, "total", Value::number(total));
    dispatch_with_event(scope, target, kind, &event);
}

pub(crate) fn dispatch_engine_event(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    let dispatch = get(scope, target, "dispatchEvent");
    if !scope.is_function(&dispatch) {
        return;
    }
    let outer = ENGINE_EVENT.with(|engine| engine.replace(quickjs::identity(event)));
    let result = scope.call(&dispatch, target, core::slice::from_ref(event));
    ENGINE_EVENT.with(|engine| engine.set(outer));
    if let Err(exception) = result {
        let js = ffi::js_of(scope);
        crate::errors::report_exception_at(js, &exception, Some("worker"), 0, 0);
    }
}

pub(crate) fn install_event_target(scope: &mut Scope<'_>, global: &Value) {
    let call = scope.function("__ns_event_call", 3, call_listener_native);
    let _ = scope.define(global, "__ns_event_call", call, HIDDEN);
    let report = scope.function("__ns_event_report", 1, report_listener_native);
    let _ = scope.define(global, "__ns_event_report", report, HIDDEN);
    if let Err(error) = scope.eval_native_script(EVENT_TARGET_SOURCE, "<event-target>") {
        let message = scope
            .to_string(&error)
            .unwrap_or_else(|_| "(unknown)".to_owned());
        ffi::debug_error(c"js", &format!("event-target setup failed: {message}"));
    }
}
