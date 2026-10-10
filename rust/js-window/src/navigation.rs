//! Southstar — the Navigation API: navigation entries, navigate with intercepted same-document commits, traversal and currententrychange.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::history::{ensure_stack, new_key, traverse};
use crate::{Entry, array_length, bind, bind_getter, get, is_nullish, page, set, text_of, truthy};

fn entry_get_state(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let state = get(scope, this, "_state");
    if is_nullish(&state) {
        return Ok(state);
    }
    Ok(ffi::structured_clone(scope, &state).unwrap_or_else(|_| Value::null()))
}

fn make_entry(scope: &mut Scope<'_>, js: Js, index: usize) -> Value {
    let needs_key = page(js)
        .session
        .borrow()
        .entries
        .as_ref()
        .and_then(|entries| entries.get(index))
        .map(|entry| (entry.key.is_none(), entry.id.is_none()));
    let Some((needs_key, needs_id)) = needs_key else {
        return Value::null();
    };
    let key = needs_key.then(|| new_key(js));
    let id = needs_id.then(|| new_key(js));
    let (url, key, id, state) = {
        let page = page(js);
        let mut session = page.session.borrow_mut();
        let Some(entry) = session
            .entries
            .as_mut()
            .and_then(|entries| entries.get_mut(index))
        else {
            return Value::null();
        };
        if key.is_some() {
            entry.key = key;
        }
        if id.is_some() {
            entry.id = id;
        }
        (
            entry.url.clone(),
            entry.key.clone().unwrap_or_default(),
            entry.id.clone().unwrap_or_default(),
            entry.state.clone(),
        )
    };
    let object = scope.new_object();
    let url = scope.string_from_bytes(&url);
    set(scope, &object, "url", url);
    let key = scope.string(&key);
    set(scope, &object, "key", key);
    let id = scope.string(&id);
    set(scope, &object, "id", id);
    set(scope, &object, "index", Value::int(index as i32));
    set(scope, &object, "sameDocument", Value::boolean(true));
    let state = if state.is_undefined() {
        Value::null()
    } else {
        state
    };
    set(scope, &object, "_state", state);
    bind(scope, &object, "getState", 0, entry_get_state);
    let listeners = scope.new_array();
    set(scope, &object, "_listeners", listeners);
    ffi::bind_event_target_listeners(scope, &object);
    object
}

fn current_entry_value(scope: &mut Scope<'_>, js: Js) -> Value {
    if js.is_null() {
        return Value::null();
    }
    ensure_stack(js);
    let pos = page(js).session.borrow().pos;
    make_entry(scope, js, pos)
}

pub(crate) fn fire_current_entry_change(js: Js, navigation_type: Option<&str>) {
    let navigation = page(js).session.borrow().navigation.clone();
    if !navigation.is_object() {
        return;
    }
    ffi::with_main_context(js, |scope| {
        let event = ffi::target_make_event(scope, &navigation, "currententrychange");
        let kind = navigation_type.map_or_else(Value::null, |kind| scope.string(kind));
        set(scope, &event, "navigationType", kind);
        ffi::target_dispatch(scope, &navigation, "currententrychange", &event);
    });
}

fn settled_promise(scope: &mut Scope<'_>, value: Value, reject: bool) -> Value {
    let Ok((promise, resolve, rejecter)) = scope.new_promise() else {
        return Value::undefined();
    };
    let settle = if reject { rejecter } else { resolve };
    let _ = scope.call(&settle, &Value::undefined(), &[value]);
    promise
}

fn dom_error(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    let error = scope.new_error();
    let name = scope.string(name);
    set(scope, &error, "name", name);
    let message = scope.string(message);
    set(scope, &error, "message", message);
    error
}

fn result(scope: &mut Scope<'_>, committed: Value, finished: Value) -> Value {
    let object = scope.new_object();
    set(scope, &object, "committed", committed);
    set(scope, &object, "finished", finished);
    object
}

fn failed(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    let committed_error = dom_error(scope, name, message);
    let committed = settled_promise(scope, committed_error, true);
    let finished_error = dom_error(scope, name, message);
    let finished = settled_promise(scope, finished_error, true);
    result(scope, committed, finished)
}

fn settled_at_current(scope: &mut Scope<'_>, js: Js) -> Value {
    let committed_entry = current_entry_value(scope, js);
    let finished_entry = current_entry_value(scope, js);
    let committed = settled_promise(scope, committed_entry, false);
    let finished = settled_promise(scope, finished_entry, false);
    result(scope, committed, finished)
}

fn get_current(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    Ok(current_entry_value(scope, js))
}

fn entries(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let array = scope.new_array();
    if js.is_null() {
        return Ok(array);
    }
    ensure_stack(js);
    let count = page(js)
        .session
        .borrow()
        .entries
        .as_ref()
        .map_or(0, Vec::len);
    for index in 0..count {
        let entry = make_entry(scope, js, index);
        let _ = scope.set_index(&array, index as u32, entry);
    }
    Ok(array)
}

fn can_go_back(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::boolean(false));
    }
    ensure_stack(js);
    Ok(Value::boolean(page(js).session.borrow().pos > 0))
}

fn can_go_forward(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::boolean(false));
    }
    ensure_stack(js);
    let page = page(js);
    let session = page.session.borrow();
    let count = session.entries.as_ref().map_or(0, Vec::len);
    Ok(Value::boolean(session.pos + 1 < count))
}

fn event_intercept(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    if !truthy(scope, this, "canIntercept") {
        return Err(scope.dom_exception(
            "SecurityError",
            "intercept: this navigation cannot be intercepted",
        ));
    }
    set(scope, this, "_intercepted", Value::boolean(true));
    let mut handlers = get(scope, this, "_handlers");
    if !handlers.is_array() {
        handlers = scope.new_array();
        set(scope, this, "_handlers", handlers.clone());
    }
    if let Some(options) = args.first().filter(|options| options.is_object()) {
        let handler = get(scope, options, "handler");
        if scope.is_function(&handler) {
            let count = array_length(scope, &handlers);
            let _ = scope.set_index(&handlers, count, handler);
        }
    }
    Ok(Value::undefined())
}

fn event_noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn fire_at_navigation(scope: &mut Scope<'_>, navigation: &Value, kind: &str) {
    if ffi::js_of(scope).is_null() || !navigation.is_object() {
        return;
    }
    let event = ffi::target_make_event(scope, navigation, kind);
    ffi::target_dispatch(scope, navigation, kind, &event);
}

fn finish_success(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    fire_at_navigation(scope, &data[0], "navigatesuccess");
    let _ = scope.call(&data[1], &Value::undefined(), &data[3..4]);
    Ok(Value::undefined())
}

fn finish_error(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    fire_at_navigation(scope, &data[0], "navigateerror");
    let reason = args.first().cloned().unwrap_or_else(Value::undefined);
    let _ = scope.call(&data[2], &Value::undefined(), &[reason]);
    Ok(Value::undefined())
}

fn run_intercept(
    scope: &mut Scope<'_>,
    js: Js,
    event: &Value,
    commit: &Value,
    finish: &Value,
    fail: &Value,
) {
    let handlers = get(scope, event, "_handlers");
    let results = scope.new_array();
    if handlers.is_array() {
        let count = array_length(scope, &handlers);
        for index in 0..count {
            let handler = scope
                .get_index(&handlers, index)
                .unwrap_or_else(|_| Value::undefined());
            let outcome = scope
                .call(&handler, &Value::undefined(), &[])
                .unwrap_or_else(|error| error);
            let _ = scope.set_index(&results, index, outcome);
        }
    }
    let current = current_entry_value(scope, js);
    let _ = scope.call(commit, &Value::undefined(), core::slice::from_ref(&current));
    let global = scope.global();
    let promise = get(scope, &global, "Promise");
    let all = get(scope, &promise, "all");
    let all = scope
        .call(&all, &promise, &[results])
        .unwrap_or_else(|_| Value::undefined());
    let navigation = page(js).session.borrow().navigation.clone();
    let data = [navigation, finish.clone(), fail.clone(), current];
    let on_success = scope.bound_function("", 0, finish_success, &data);
    let on_error = scope.bound_function("", 0, finish_error, &data);
    let then = get(scope, &all, "then");
    let _ = scope.call(&then, &all, &[on_success, on_error]);
}

fn origin_parts(url: &[u8]) -> Option<ffi::UrlOrigin> {
    ffi::url_parts(url)
}

fn same_origin(url: &[u8], current: &[u8]) -> bool {
    match (origin_parts(url), origin_parts(current)) {
        (Some(a), Some(b)) => {
            a.protocol == b.protocol
                && a.hostname.eq_ignore_ascii_case(&b.hostname)
                && a.port == b.port
        }
        _ => false,
    }
}

fn commit_intercepted(js: Js, url: &[u8], state: Value, replace: bool) {
    let page = page(js);
    let mut session = page.session.borrow_mut();
    session.state = state.clone();
    let pos = session.pos;
    if replace {
        session.key_seq += 1;
        let id = format!("{:016}x", session.key_seq);
        if let Some(entry) = session
            .entries
            .as_mut()
            .and_then(|entries| entries.get_mut(pos))
        {
            entry.url = url.to_vec();
            entry.state = state;
            entry.id = Some(id);
        }
        return;
    }
    session.key_seq += 2;
    let key = format!("{:016}x", session.key_seq - 1);
    let id = format!("{:016}x", session.key_seq);
    session.length += 1;
    if let Some(entries) = session.entries.as_mut() {
        entries.truncate(pos + 1);
        entries.push(Entry {
            url: url.to_vec(),
            state,
            key: Some(key),
            id: Some(id),
        });
        session.pos = entries.len() - 1;
    }
}

fn navigate(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(target) = args.first().filter(|_| !js.is_null()) else {
        return Ok(failed(scope, "SyntaxError", "navigate: a URL is required"));
    };
    ensure_stack(js);
    let current = ffi::current_url(js);
    let url = match text_of(scope, target) {
        Some(raw) if !raw.is_empty() => {
            ffi::url_resolve((!current.is_empty()).then_some(&current[..]), &raw)
        }
        _ => None,
    };
    let Some(url) = url else {
        return Ok(failed(
            scope,
            "SyntaxError",
            "navigate: the URL could not be parsed",
        ));
    };

    let mut state = Value::undefined();
    let mut info = Value::undefined();
    let mut replace = false;
    if let Some(options) = args.get(1).filter(|options| options.is_object()) {
        state = get(scope, options, "state");
        info = get(scope, options, "info");
        let history = get(scope, options, "history");
        replace = text_of(scope, &history).is_some_and(|mode| mode == b"replace");
    }
    let same_origin = same_origin(&url, &current);

    let event = ffi::target_make_event(scope, this, "navigate");
    set(scope, &event, "cancelable", Value::boolean(true));
    let kind = scope.string(if replace { "replace" } else { "push" });
    set(scope, &event, "navigationType", kind);
    set(scope, &event, "canIntercept", Value::boolean(same_origin));
    set(scope, &event, "userInitiated", Value::boolean(false));
    set(scope, &event, "hashChange", Value::boolean(false));
    set(scope, &event, "info", info);
    let destination = scope.new_object();
    let destination_url = scope.string_from_bytes(&url);
    set(scope, &destination, "url", destination_url);
    let empty = scope.string("");
    set(scope, &destination, "key", empty.clone());
    set(scope, &destination, "id", empty);
    set(scope, &destination, "index", Value::int(-1));
    set(
        scope,
        &destination,
        "sameDocument",
        Value::boolean(same_origin),
    );
    set(scope, &destination, "_state", state.clone());
    bind(scope, &destination, "getState", 0, entry_get_state);
    set(scope, &event, "destination", destination);
    bind(scope, &event, "intercept", 1, event_intercept);
    bind(scope, &event, "scroll", 0, event_noop);
    bind(scope, &event, "commit", 0, event_noop);

    ffi::target_dispatch(scope, this, "navigate", &event);

    let prevented = truthy(scope, &event, "defaultPrevented");
    let intercepted = truthy(scope, &event, "_intercepted");
    if prevented {
        return Ok(failed(
            scope,
            "AbortError",
            "navigate: canceled by a navigate handler",
        ));
    }
    if !(intercepted && same_origin) {
        ffi::navigate(js, &url, false);
        return Ok(settled_at_current(scope, js));
    }
    let (committed, commit, _) = scope.new_promise()?;
    let (finished, finish, fail) = scope.new_promise()?;
    let cloned = if is_nullish(&state) {
        Value::null()
    } else {
        ffi::structured_clone(scope, &state).unwrap_or_else(|_| Value::null())
    };
    ffi::set_current_url(js, &url);
    commit_intercepted(js, &url, cloned, replace);
    ffi::soft_navigate(js, &url, replace);
    fire_current_entry_change(js, Some(if replace { "replace" } else { "push" }));
    run_intercept(scope, js, &event, &commit, &finish, &fail);
    Ok(result(scope, committed, finished))
}

fn reload(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if !js.is_null() {
        let current = ffi::current_url(js);
        if !current.is_empty() {
            ffi::navigate(js, &current, true);
        }
    }
    Ok(settled_at_current(scope, js))
}

fn traverse_by(scope: &mut Scope<'_>, js: Js, delta: i64) -> Value {
    traverse(scope, delta);
    settled_at_current(scope, js)
}

fn back(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    Ok(traverse_by(scope, js, -1))
}

fn forward(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    Ok(traverse_by(scope, js, 1))
}

fn traverse_to(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(key) = args.first().filter(|_| !js.is_null()) else {
        return Ok(failed(
            scope,
            "InvalidStateError",
            "traverseTo: a key is required",
        ));
    };
    ensure_stack(js);
    let key = text_of(scope, key);
    let found = key.and_then(|key| {
        let page = page(js);
        let session = page.session.borrow();
        let index = session.entries.as_ref()?.iter().position(|entry| {
            entry
                .key
                .as_ref()
                .is_some_and(|entry_key| entry_key.as_bytes() == key)
        })?;
        Some(index as i64 - session.pos as i64)
    });
    let Some(delta) = found else {
        return Ok(failed(
            scope,
            "InvalidStateError",
            "traverseTo: no entry has that key",
        ));
    };
    Ok(traverse_by(scope, js, delta))
}

fn update_current(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    ensure_stack(js);
    let Some(options) = args.first().filter(|options| options.is_object()) else {
        return Err(scope.type_error("updateCurrentEntry: an options object is required"));
    };
    let state = get(scope, options, "state");
    let cloned = if is_nullish(&state) {
        Value::null()
    } else {
        ffi::structured_clone(scope, &state)?
    };
    {
        let page = page(js);
        let mut session = page.session.borrow_mut();
        session.state = cloned.clone();
        let pos = session.pos;
        if let Some(entry) = session
            .entries
            .as_mut()
            .and_then(|entries| entries.get_mut(pos))
        {
            entry.state = cloned;
        }
    }
    fire_current_entry_change(js, None);
    Ok(Value::undefined())
}

pub(crate) fn make_navigation(scope: &mut Scope<'_>) -> Value {
    let navigation = scope.new_object();
    let listeners = scope.new_array();
    set(scope, &navigation, "_listeners", listeners);
    ffi::bind_event_target_listeners(scope, &navigation);
    bind(scope, &navigation, "entries", 0, entries);
    bind(scope, &navigation, "navigate", 2, navigate);
    bind(scope, &navigation, "reload", 1, reload);
    bind(scope, &navigation, "back", 0, back);
    bind(scope, &navigation, "forward", 0, forward);
    bind(scope, &navigation, "traverseTo", 1, traverse_to);
    bind(scope, &navigation, "updateCurrentEntry", 1, update_current);
    bind_getter(
        scope,
        &navigation,
        "currentEntry",
        "currentEntry",
        get_current,
    );
    bind_getter(scope, &navigation, "canGoBack", "canGoBack", can_go_back);
    bind_getter(
        scope,
        &navigation,
        "canGoForward",
        "canGoForward",
        can_go_forward,
    );
    for handler in [
        "onnavigate",
        "onnavigatesuccess",
        "onnavigateerror",
        "oncurrententrychange",
    ] {
        set(scope, &navigation, handler, Value::null());
    }
    let js = ffi::js_of(scope);
    if !js.is_null() {
        page(js).session.borrow_mut().navigation = navigation.clone();
    }
    navigation
}
