//! Southstar — the Fullscreen API with its change and error events, and Pointer Lock.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::index::next_in_subtree;
use southstar_dom::tree::root;
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult};

const SLOT: &str = "_nd_fullscreen_element";
const CHANGE_EVENTS: [&CStr; 4] = [
    c"fullscreenchange",
    c"webkitfullscreenchange",
    c"mozfullscreenchange",
    c"MSFullscreenChange",
];
const ERROR_EVENTS: [&CStr; 2] = [c"fullscreenerror", c"webkitfullscreenerror"];

fn is_set(value: &Value) -> bool {
    !value.is_undefined() && !value.is_null()
}

pub(crate) fn get_fullscreen_element(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let value = scope.get(this, SLOT)?;
    Ok(if value.is_undefined() {
        Value::null()
    } else {
        value
    })
}

pub(crate) fn get_fullscreen_enabled(
    _scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> JsResult {
    Ok(Value::boolean(true))
}

pub(crate) fn get_is_fullscreen(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let value = scope.get(this, SLOT)?;
    Ok(Value::boolean(is_set(&value)))
}

fn dispatch_all(js: Js, target: Option<Element>, events: &[&CStr]) {
    let Some(doc) = js.current_document() else {
        return;
    };
    let target = target.unwrap_or(doc);
    for kind in events {
        js.dispatch(target, kind);
    }
}

fn change_job(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if !js.is_null() {
        dispatch_all(js, args.first().and_then(ffi::unwrap), &CHANGE_EVENTS);
    }
    Ok(Value::undefined())
}

fn error_job(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if !js.is_null() {
        dispatch_all(js, args.first().and_then(ffi::unwrap), &ERROR_EVENTS);
    }
    Ok(Value::undefined())
}

fn enqueue(scope: &mut Scope<'_>, job: southstar_js_engine::NativeFn, arg: Value) {
    let function = scope.function("", 1, job);
    let _ = scope.enqueue_call(&function, &[arg]);
}

fn set_fullscreen_element(scope: &mut Scope<'_>, element: Value, action: &CStr) -> JsResult<()> {
    let js = ffi::js_of(scope);
    let node = if element.is_null() {
        None
    } else {
        ffi::unwrap(&element)
    };
    ffi::set_fullscreen_node(node);
    let global = scope.global();
    let document = scope.get(&global, "document")?;
    let previous = scope.get(&document, SLOT)?;
    let event_value = if element.is_null() {
        previous
    } else {
        element.clone()
    };
    let event_target = ffi::unwrap(&event_value);
    scope.set(&document, SLOT, element)?;
    if js.is_null() {
        return Ok(());
    }
    if js.has_window_action() {
        let target = event_target.or_else(|| js.current_document());
        crate::with(js, |page| page.fullscreen_target = target);
        js.window_action(action);
    }
    js.mark_mutated();
    if !js.has_window_action() {
        enqueue(scope, change_job, event_value);
    }
    Ok(())
}

fn transition_promise(scope: &mut Scope<'_>) -> JsResult {
    let js = ffi::js_of(scope);
    let (promise, resolve, _reject) = scope.new_promise()?;
    if !js.is_null() && js.has_window_action() {
        let old = crate::with(js, |page| page.fullscreen_resolve.replace(resolve));
        drop(old);
    } else {
        scope.call(&resolve, &Value::undefined(), &[])?;
    }
    Ok(promise)
}

fn rejected_without_activation(scope: &mut Scope<'_>, element: &Value) -> JsResult {
    let js = ffi::js_of(scope);
    if !js.is_null() {
        js.log_line(c"Blocked requestFullscreen(): the page has no recent user interaction");
    }
    enqueue(scope, error_job, element.clone());
    let (promise, _resolve, reject) = scope.new_promise()?;
    let error = scope.new_error();
    let name = scope.string("TypeError");
    scope.set(&error, "name", name)?;
    let message = scope.string("Fullscreen request denied: no transient user activation");
    scope.set(&error, "message", message)?;
    scope.call(&reject, &Value::undefined(), &[error])?;
    Ok(promise)
}

pub(crate) fn request_fullscreen(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() || !js.has_transient_activation() {
        return rejected_without_activation(scope, this);
    }
    set_fullscreen_element(scope, this.clone(), c"fullscreen-enter")?;
    transition_promise(scope)
}

pub(crate) fn exit_fullscreen(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let current = scope.get(this, SLOT)?;
    if is_set(&current) {
        set_fullscreen_element(scope, Value::null(), c"fullscreen-exit")?;
        return transition_promise(scope);
    }
    let (promise, resolve, _reject) = scope.new_promise()?;
    scope.call(&resolve, &Value::undefined(), &[Value::undefined()])?;
    Ok(promise)
}

pub(crate) fn window_action_applied(js: Js) {
    let resolve = crate::with(js, |page| page.fullscreen_resolve.take());
    if let Some(resolve) = resolve {
        js.scope(|scope| {
            let _ = scope.call(&resolve, &Value::undefined(), &[]);
        });
        drop(resolve);
    }
    let Some(target) = crate::with(js, |page| page.fullscreen_target.take()) else {
        return;
    };
    dispatch_all(js, Some(target), &CHANGE_EVENTS);
}

fn pointer_lock_changed(js: Js) {
    if let Some(doc) = js.current_document() {
        js.dispatch(doc, c"pointerlockchange");
    }
}

pub(crate) fn request_pointer_lock(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    let key = el.as_ptr() as usize;
    if !js.is_null() && crate::peek(js, |page| page.pointer_lock) != key {
        crate::with(js, |page| page.pointer_lock = key);
        pointer_lock_changed(js);
    }
    Ok(Value::undefined())
}

pub(crate) fn exit_pointer_lock(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if !js.is_null() && crate::peek(js, |page| page.pointer_lock) != 0 {
        crate::with(js, |page| page.pointer_lock = 0);
        pointer_lock_changed(js);
    }
    Ok(Value::undefined())
}

pub(crate) fn get_pointer_lock_element(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::null());
    }
    let key = crate::peek(js, |page| page.pointer_lock);
    let Some(doc) = js.current_document().filter(|_| key != 0) else {
        return Ok(Value::null());
    };
    let top = root(doc);
    let mut cur = Some(top);
    while let Some(n) = cur {
        if n.as_ptr() as usize == key {
            return Ok(ffi::wrap(scope, n));
        }
        cur = next_in_subtree(n, Some(top), true);
    }
    crate::with(js, |page| page.pointer_lock = 0);
    Ok(Value::null())
}
