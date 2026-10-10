//! Southstar — the History object: pushState, replaceState, traversal and the popstate event.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Entry, bind, bind_getter, is_nullish, navigation, page, set, text_of};

pub(crate) fn ensure_stack(js: Js) {
    let page = page(js);
    let mut session = page.session.borrow_mut();
    if session.entries.is_some() {
        return;
    }
    session.entries = Some(vec![Entry {
        url: ffi::current_url(js),
        state: Value::null(),
        key: None,
        id: None,
    }]);
    session.pos = 0;
}

pub(crate) fn new_key(js: Js) -> String {
    let page = page(js);
    let mut session = page.session.borrow_mut();
    session.key_seq += 1;
    format!("{:016}x", session.key_seq)
}

fn get_state(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::null());
    }
    Ok(page(js).session.borrow().state.clone())
}

fn get_length(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::int(1));
    }
    let page = page(js);
    let session = page.session.borrow();
    let length = match &session.entries {
        Some(entries) if !entries.is_empty() => entries.len() as i32,
        _ => session.length,
    };
    Ok(Value::int(length))
}

fn same_origin(a: &[u8], b: &[u8]) -> bool {
    match (ffi::url_origin(a), ffi::url_origin(b)) {
        (Some(a), Some(b)) => !a.is_empty() && a == b,
        _ => false,
    }
}

fn set_state(scope: &mut Scope<'_>, args: &[Value], replace: bool) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    ensure_stack(js);
    let state = match args.first() {
        Some(state) if !is_nullish(state) => ffi::structured_clone(scope, state)?,
        _ => Value::null(),
    };
    let current = ffi::current_url(js);
    let mut new_url = None;
    if let Some(url) = args.get(2).filter(|url| !is_nullish(url))
        && let Some(raw) = text_of(scope, url)
        && !raw.is_empty()
    {
        let resolved = if current.is_empty() {
            Some(raw)
        } else {
            ffi::url_resolve(Some(&current), &raw)
        };
        let Some(resolved) = resolved else {
            return Err(
                scope.dom_exception("SecurityError", "history: the URL could not be parsed")
            );
        };
        new_url = Some(resolved);
    }
    if let Some(url) = &new_url
        && !same_origin(url, &current)
    {
        return Err(scope.dom_exception(
            "SecurityError",
            "history: URL must be same-origin as the document",
        ));
    }
    if let Some(url) = &new_url {
        ffi::set_current_url(js, url);
    }
    let url = ffi::current_url(js);
    {
        let page = page(js);
        let mut session = page.session.borrow_mut();
        session.state = state.clone();
        if !replace {
            session.length += 1;
        }
        let pos = session.pos;
        if let Some(entries) = session.entries.as_mut() {
            if replace {
                entries[pos].url = url.clone();
                entries[pos].state = state;
            } else {
                entries.truncate(pos + 1);
                entries.push(Entry {
                    url: url.clone(),
                    state,
                    key: None,
                    id: None,
                });
                session.pos = entries.len() - 1;
            }
        }
    }
    ffi::soft_navigate(js, &url, replace);
    navigation::fire_current_entry_change(js, Some(if replace { "replace" } else { "push" }));
    Ok(Value::undefined())
}

fn push_state(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_state(scope, args, false)
}

fn replace_state(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_state(scope, args, true)
}

fn popstate(scope: &mut Scope<'_>) {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return;
    }
    let state = page(js).session.borrow_mut().popstates.pop_front();
    if ffi::window_events_blocked(js) {
        return;
    }
    let state = state.unwrap_or_else(Value::null);
    let event = ffi::make_window_event(scope, "popstate");
    set(scope, &event, "state", state);
    ffi::dispatch_document_window_event(js, "popstate", event);
}

pub(crate) fn traverse(scope: &mut Scope<'_>, delta: i64) {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return;
    }
    ensure_stack(js);
    let page = page(js);
    let url = {
        let mut session = page.session.borrow_mut();
        let count = session.entries.as_ref().map_or(0, Vec::len) as i64;
        let target = session.pos as i64 + delta;
        if target < 0 || target > count - 1 || target == session.pos as i64 {
            return;
        }
        session.pos = target as usize;
        let Some(entry) = session
            .entries
            .as_ref()
            .map(|entries| &entries[target as usize])
        else {
            return;
        };
        let (url, state) = (entry.url.clone(), entry.state.clone());
        session.state = state.clone();
        session.popstates.push_back(state);
        url
    };
    ffi::set_current_url(js, &url);
    ffi::soft_navigate(js, &url, true);
    if scope.enqueue_job(popstate).is_err() {
        page.session.borrow_mut().popstates.pop_back();
    }
    navigation::fire_current_entry_change(js, Some("traverse"));
}

fn back(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    traverse(scope, -1);
    Ok(Value::undefined())
}

fn forward(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    traverse(scope, 1);
    Ok(Value::undefined())
}

fn go(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let delta = match args.first() {
        Some(delta) => scope.to_int32(delta).unwrap_or(0),
        None => 0,
    };
    traverse(scope, delta as i64);
    Ok(Value::undefined())
}

pub(crate) fn make_history(scope: &mut Scope<'_>) -> Value {
    let history = scope.new_object();
    let auto = scope.string("auto");
    set(scope, &history, "scrollRestoration", auto);
    bind(scope, &history, "pushState", 3, push_state);
    bind(scope, &history, "replaceState", 3, replace_state);
    bind(scope, &history, "back", 0, back);
    bind(scope, &history, "forward", 0, forward);
    bind(scope, &history, "go", 1, go);
    bind_getter(scope, &history, "state", "get state", get_state);
    bind_getter(scope, &history, "length", "get length", get_length);
    let _ = scope.define_to_string_tag(&history, "History");
    history
}
