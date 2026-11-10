//! Southstar — the window a document of another origin is handed: a proxy whose script never holds the real window, only native functions bound to it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::bootstrap;
use crate::ffi::ctx::{self, Ctx, PropertyKey};
use crate::ffi::{self, Js};
use crate::links::{nsr_window_current_document_for, nsr_window_post_message_from};
use crate::page::{self, Frame};
use crate::{owner, realm};

type JsResult = Result<Value, Value>;

const POST_MESSAGE_ARGS: usize = 3;

pub(crate) struct CrossWindow {
    pub accessor: Value,
    pub target: Value,
    pub proxy: Value,
}

fn cached(js: Js, accessor: &Value, target: &Value) -> Option<Value> {
    page::peek(js, |page| {
        page.cross_windows
            .iter()
            .find(|held| {
                held.accessor.same_object(accessor)
                    && (held.target.same_object(target) || held.proxy.same_object(target))
            })
            .map(|held| held.proxy.clone())
    })
    .flatten()
}

fn frame_windows(js: Js) -> Vec<(Frame, Value)> {
    let mut windows: Vec<(Frame, Value)> = page::realms(js)
        .into_iter()
        .map(|(frame, realm)| (frame, realm.global()))
        .collect();
    let stored = page::peek(js, |page| {
        page.frames
            .iter()
            .filter_map(|(frame, entry)| Some((*frame, entry.window.clone()?)))
            .collect::<Vec<_>>()
    });
    windows.extend(stored.unwrap_or_default());
    windows
}

fn live_windows(js: Js) -> Vec<Value> {
    let mut windows = vec![ffi::main_realm(js).global()];
    windows.extend(frame_windows(js).into_iter().map(|(_, window)| window));
    windows
}

fn remember(js: Js, entry: CrossWindow) {
    let live = live_windows(js);
    let is_live = |window: &Value| live.iter().any(|held| held.same_object(window));
    let stale = page::with(js, |page| {
        let (kept, stale): (Vec<_>, Vec<_>) = core::mem::take(&mut page.cross_windows)
            .into_iter()
            .partition(|held| is_live(&held.accessor) && is_live(&held.target));
        page.cross_windows = kept;
        page.cross_windows.push(entry);
        stale
    });
    drop(stale);
}

fn send(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let [target, source] = data else {
        return Ok(Value::undefined());
    };
    let Some((count, rest)) = args.split_first() else {
        return Ok(Value::undefined());
    };
    let count = scope.to_int32(count).unwrap_or(0).max(0) as usize;
    let given = &rest[..count.min(POST_MESSAGE_ARGS).min(rest.len())];
    nsr_window_post_message_from(scope, target, source, given)
}

fn navigate(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let ([target], [url, replace, ..]) = (data, args) else {
        return Ok(Value::undefined());
    };
    if !url.is_string() {
        return Ok(Value::undefined());
    }
    let Ok(location) = scope.get(target, "location") else {
        return Ok(Value::undefined());
    };
    if !location.is_object() {
        return Ok(Value::undefined());
    }
    if scope.to_bool(replace) {
        if let Ok(method) = scope.get(&location, "replace")
            && scope.is_function(&method)
        {
            drop(scope.call(&method, &location, core::slice::from_ref(url)));
        }
    } else {
        drop(scope.set(&location, "href", url.clone()));
    }
    Ok(Value::undefined())
}

fn closed(scope: &mut Scope<'_>, _this: &Value, _args: &[Value], data: &[Value]) -> JsResult {
    let state = data
        .first()
        .and_then(|target| scope.get(target, "closed").ok());
    Ok(Value::boolean(
        state.is_some_and(|state| scope.to_bool(&state)),
    ))
}

fn length(scope: &mut Scope<'_>, _this: &Value, _args: &[Value], data: &[Value]) -> JsResult {
    let count = data
        .first()
        .and_then(|target| scope.get(target, "length").ok())
        .filter(Value::is_number)
        .and_then(|count| scope.to_number(&count).ok())
        .filter(|count| count.is_finite() && *count >= 0.0)
        .map_or(0, |count| count.min(f64::from(u32::MAX)) as u32);
    Ok(Value::number(f64::from(count)))
}

fn act(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let ([target], [name, ..]) = (data, args) else {
        return Ok(Value::undefined());
    };
    let name = match scope.to_bytes(name).as_deref() {
        Ok(b"close") if name.is_string() => "close",
        Ok(b"focus") if name.is_string() => "focus",
        Ok(b"blur") if name.is_string() => "blur",
        _ => return Ok(Value::undefined()),
    };
    if let Ok(method) = scope.get(target, name)
        && scope.is_function(&method)
    {
        drop(scope.call(&method, target, &[]));
    }
    Ok(Value::undefined())
}

fn child(scope: &mut Scope<'_>, _this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let ([target], [key, up, ..]) = (data, args) else {
        return Ok(Value::undefined());
    };
    if !key.is_string() {
        return Ok(Value::undefined());
    }
    let doc = nsr_window_current_document_for(scope, target);
    let Ok(key) = ctx::property_key(scope, key) else {
        return Ok(Value::undefined());
    };
    let found = match key {
        PropertyKey::Index(index) => owner::child_frame_window(scope, doc, index, None, true),
        PropertyKey::Name(name) => {
            owner::child_frame_window(scope, doc, 0, Some(name.as_c_str()), true)
        }
    };
    match found {
        Ok(window) if window.is_object() => {
            let ctx = ffi::scope_ctx(scope);
            Ok(make(ctx, window, Some(up.clone())))
        }
        _ => Ok(Value::undefined()),
    }
}

fn own_window(ctx: Ctx, global: &Value, name: &str) -> Value {
    let held = ctx.get(global, name);
    if held.is_object() {
        held
    } else {
        global.clone()
    }
}

fn frame_of_accessor(js: Js, accessor: &Value, target: &Value) -> bool {
    frame_windows(js)
        .into_iter()
        .find(|(_, window)| window.same_object(target))
        .and_then(|(frame, _)| ffi::frame_node(frame))
        .is_some_and(|frame| {
            realm::parent_realm(js, Some(frame))
                .global()
                .same_object(accessor)
        })
}

fn make(ctx: Ctx, target: Value, up: Option<Value>) -> Value {
    let js = Js::of_ctx(ctx);
    let global = ctx.global();
    if !target.is_object() || js.is_null() {
        return Value::null();
    }
    if let Some(proxy) = cached(js, &global, &target) {
        return proxy;
    }
    let Some(maker) = ctx.eval_hidden(bootstrap::CROSS_WINDOW, c"<cross-window>") else {
        return Value::null();
    };
    if !ctx.is_function(&maker) {
        return Value::null();
    }
    let (up, top) = match up {
        Some(up) => (up, Value::undefined()),
        None if frame_of_accessor(js, &global, &target) => (
            own_window(ctx, &global, "window"),
            own_window(ctx, &global, "top"),
        ),
        None => (Value::undefined(), Value::undefined()),
    };
    let lists_frames = page::frame_of_realm(js, ctx).is_some();
    let natives = ctx.enter(|scope| {
        let one = core::slice::from_ref(&target);
        let pair = [target.clone(), global.clone()];
        [
            scope.bound_function("postMessage", 4, send, &pair),
            scope.bound_function("navigate", 2, navigate, one),
            scope.bound_function("closed", 0, closed, one),
            scope.bound_function("length", 0, length, one),
            scope.bound_function("act", 1, act, one),
            if lists_frames {
                scope.bound_function("child", 2, child, one)
            } else {
                Value::undefined()
            },
        ]
    });
    let [send, navigate, closed, length, act, child] = natives;
    let proxy = ctx
        .call(
            &maker,
            &Value::undefined(),
            &[send, navigate, closed, length, act, child, up, top],
        )
        .unwrap_or_else(Value::null);
    if !proxy.is_object() {
        return Value::null();
    }
    remember(
        js,
        CrossWindow {
            accessor: global,
            target,
            proxy: proxy.clone(),
        },
    );
    proxy
}

pub(crate) fn cross_origin_window(ctx: Ctx, target: Value) -> Value {
    make(ctx, target, None)
}

fn wrap(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(target) = args.first().filter(|target| target.is_object()) else {
        return Ok(Value::undefined());
    };
    Ok(make(ffi::scope_ctx(scope), target.clone(), None))
}

pub(crate) fn wrap_function(ctx: Ctx) -> Value {
    ctx.enter(|scope| scope.function("crossWindow", 1, wrap))
}
