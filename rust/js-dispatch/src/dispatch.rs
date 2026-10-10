//! Southstar — the dispatch algorithm for node and window targets: the event path, the capture, target and bubble phases, the window's listeners and handlers, and the bubbles and cancelable flags of the browser's own events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_dom::{NsNode, ancestors_and_self};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::handlers;
use crate::listeners::{Listener, retire};
use crate::{
    DispatchPath, Element, FLAG_HAS_LISTENERS, get, is_document, is_named, is_real_document,
    message_of, nearest_document, set, truthy,
};

pub(crate) fn init_flags(kind: &str, at_document: bool) -> (bool, bool) {
    match kind {
        "scroll" | "scrollend" => (at_document, false),
        "invalid" | "cancel" => (false, true),
        "abort" | "error" | "load" | "unload" | "readystatechange" | "resize" | "focus"
        | "blur" | "toggle" | "close" | "loadstart" | "progress" | "suspend" | "emptied"
        | "stalled" | "loadedmetadata" | "loadeddata" | "canplay" | "canplaythrough"
        | "playing" | "waiting" | "seeking" | "seeked" | "ended" | "durationchange"
        | "timeupdate" | "play" | "pause" | "ratechange" | "volumechange" => (false, false),
        "DOMContentLoaded"
        | "input"
        | "change"
        | "focusin"
        | "focusout"
        | "select"
        | "fullscreenchange"
        | "fullscreenerror"
        | "webkitfullscreenchange"
        | "webkitfullscreenerror"
        | "mozfullscreenchange"
        | "MSFullscreenChange" => (true, false),
        _ => (true, true),
    }
}

pub(crate) fn dispatch_event(js: Js, target: Element, kind: &str) -> (bool, bool) {
    if js.halted() || js.in_pump() {
        return (false, false);
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, Some(target));
        let (bubbles, cancelable) = init_flags(kind, is_document(target));
        set(scope, &event, "bubbles", Value::boolean(bubbles));
        set(scope, &event, "cancelable", Value::boolean(cancelable));
        event
    }) else {
        return (false, false);
    };
    dispatch_built(js, target, kind, event)
}

fn propagation_stopped(scope: &mut Scope<'_>, event: &Value) -> bool {
    truthy(scope, event, "_propagation_stopped")
}

fn event_stopped(js: Js, event: &Value) -> bool {
    js.scope(|scope| propagation_stopped(scope, event))
        .unwrap_or(false)
}

fn finish_event(scope: &mut Scope<'_>, event: &Value) -> bool {
    set(scope, event, "currentTarget", Value::null());
    set(scope, event, "eventPhase", Value::int(0));
    set(scope, event, "_propagation_stopped", Value::boolean(false));
    set(scope, event, "_immediate_stopped", Value::boolean(false));
    set(scope, event, "_dispatching", Value::boolean(false));
    truthy(scope, event, "defaultPrevented")
}

pub(crate) fn dispatch_built(js: Js, target: Element, kind: &str, event: Value) -> (bool, bool) {
    if js.is_null() {
        return (false, false);
    }
    let saved_ctx = js.ctx();
    let target_ctx = js.realm_for_node(target);
    let mut root = target;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let detached = !is_real_document(root);
    let main_realm = js.main_realm();
    let ctx = if !target_ctx.is_null() {
        target_ctx
    } else if detached || main_realm.is_null() {
        saved_ctx
    } else {
        main_realm
    };
    let target_doc = nearest_document(Some(target));
    let frame = target_doc.and_then(|doc| doc.parent().map(|frame| (doc, frame)));
    js.in_dispatch_scope(ctx, frame, || {
        let budget = js.budget_enter();
        let result = run_path(js, target, kind, &event);
        drop(event);
        js.finish_dispatch();
        js.budget_leave(budget);
        result
    })
}

fn take_path(js: Js) -> (Vec<*const NsNode>, Vec<bool>) {
    crate::with(js, |page| {
        (
            page.spare_paths.pop().unwrap_or_default(),
            page.spare_flags.pop().unwrap_or_default(),
        )
    })
}

fn return_path(js: Js, mut nodes: Vec<*const NsNode>, mut flags: Vec<bool>) {
    nodes.clear();
    flags.clear();
    crate::with(js, |page| {
        page.spare_paths.push(nodes);
        page.spare_flags.push(flags);
    });
}

fn in_shadow_tree(node: Element) -> bool {
    for p in ancestors_and_self(node) {
        if ffi::is_shadow_root(p) {
            return true;
        }
        if is_document(p) {
            return false;
        }
    }
    false
}

fn run_path(js: Js, target: Element, kind: &str, event: &Value) -> (bool, bool) {
    let mut fired = false;
    let (mut nodes, mut flags) = take_path(js);
    let mut cur = Some(target);
    while let Some(n) = cur {
        nodes.push(n.as_ptr());
        if is_document(n) {
            break;
        }
        cur = n.parent();
    }
    flags.extend(
        nodes
            .iter()
            .map(|&n| ffi::node(n).is_some_and(in_shadow_tree)),
    );
    let path_len = nodes.len();
    let mut window_in_path = nodes
        .last()
        .and_then(|&tail| ffi::node(tail))
        .is_some_and(is_real_document);
    crate::with(js, |page| {
        page.paths.push(DispatchPath {
            nodes: nodes.as_ptr(),
            len: path_len,
            window: window_in_path,
        })
    });
    let node_at = |i: usize| ffi::node(nodes[i]);
    let Some(bubbles) = js.scope(|scope| {
        set(scope, event, "_dispatching", Value::boolean(true));
        truthy(scope, event, "bubbles")
    }) else {
        crate::with(js, |page| page.paths.pop());
        return_path(js, nodes, flags);
        return (false, false);
    };
    let current_doc = js.current_document();
    let at_document = current_doc == Some(target);
    if !bubbles && !at_document && matches!(kind, "load" | "error") {
        window_in_path = false;
    }
    let window_is_target =
        !bubbles && window_in_path && at_document && matches!(kind, "load" | "unload");
    let mut stopped = event_stopped(js, event);
    if window_is_target {
        if !stopped {
            stopped = window_listeners(js, Some(target), kind, event, true, true, &mut fired);
        }
        if !stopped {
            window_listeners(js, Some(target), kind, event, false, true, &mut fired);
        }
        stopped = true;
        window_in_path = false;
    }
    if window_in_path && !stopped {
        stopped = window_listeners(js, Some(target), kind, event, true, false, &mut fired);
    }
    for i in (0..path_len).rev() {
        if stopped {
            break;
        }
        if let Some(cur) = node_at(i) {
            stopped = invoke_at(
                js,
                cur,
                target,
                kind,
                event,
                Phase::capture(i == 0, flags[i]),
                &mut fired,
            );
        }
    }
    let mut bubble_len = path_len;
    if matches!(kind, "submit" | "reset")
        && let Some(form) = (1..path_len).find(|&i| is_named(node_at(i), b"form"))
    {
        bubble_len = form;
    }
    for (i, &in_shadow) in flags.iter().enumerate().take(bubble_len) {
        if stopped || (i > 0 && !bubbles) {
            break;
        }
        if let Some(cur) = node_at(i) {
            stopped = invoke_at(
                js,
                cur,
                target,
                kind,
                event,
                Phase::bubble(i == 0, in_shadow),
                &mut fired,
            );
        }
    }
    if window_in_path && !stopped && (bubbles || window_is_target) && bubble_len == path_len {
        window_listeners(js, Some(target), kind, event, false, false, &mut fired);
    }
    crate::with(js, |page| page.paths.pop());
    return_path(js, nodes, flags);
    let prevented = js
        .scope(|scope| finish_event(scope, event))
        .unwrap_or(false);
    (fired, prevented)
}

#[derive(Clone, Copy)]
struct Phase {
    capture: bool,
    at_target: bool,
    in_shadow: bool,
}

impl Phase {
    fn capture(at_target: bool, in_shadow: bool) -> Phase {
        Phase {
            capture: true,
            at_target,
            in_shadow,
        }
    }

    fn bubble(at_target: bool, in_shadow: bool) -> Phase {
        Phase {
            capture: false,
            at_target,
            in_shadow,
        }
    }
}

fn alias_shadowed(js: Js, cur: Element, event: &Value) -> bool {
    js.scope(|scope| {
        let base = get(scope, event, "__ns_alias_base");
        if !base.is_string() {
            return false;
        }
        match scope.to_string(&base) {
            Ok(base) => handlers::has_own_unprefixed(js, scope, cur, &base),
            Err(_) => false,
        }
    })
    .unwrap_or(false)
}

fn snapshot_own(
    js: Js,
    cur: Element,
    kind: &str,
    capture: bool,
    window_level: bool,
) -> Vec<Rc<Listener>> {
    let mut snapshot = crate::take_snapshot(js);
    let key = cur.as_ptr() as usize;
    crate::with(js, |page| {
        if let Some(own) = page.listeners.get(&key) {
            snapshot.extend(
                own.iter()
                    .filter(|l| {
                        !l.is_dead()
                            && l.window_level == window_level
                            && l.target == key
                            && *l.kind == *kind
                            && l.capture == capture
                    })
                    .cloned(),
            );
        }
    });
    snapshot
}

fn invoke_at(
    js: Js,
    cur: Element,
    target: Element,
    kind: &str,
    event: &Value,
    phase: Phase,
    fired: &mut bool,
) -> bool {
    if kind.starts_with("webkit") && alias_shadowed(js, cur, event) {
        return false;
    }
    let has_listeners = is_document(cur) || cur.flags() & FLAG_HAS_LISTENERS != 0;
    let snapshot = if has_listeners {
        snapshot_own(js, cur, kind, phase.capture, false)
    } else {
        crate::take_snapshot(js)
    };
    if !phase.capture {
        if handlers::fire_property(js, cur, kind, event)
            || handlers::fire_inline(js, cur, kind, event)
        {
            *fired = true;
        }
        if handlers::fires_at_document(js, cur, kind, event, phase.at_target) {
            *fired = true;
        }
    }
    js.dispatch_depth_add(1);
    let mut stopped = false;
    if !snapshot.is_empty() {
        let current = js.scope(|scope| {
            let current = ffi::wrap(scope, cur);
            set(scope, event, "currentTarget", current.clone());
            let number = if cur == target {
                2
            } else if phase.capture {
                1
            } else {
                3
            };
            set(scope, event, "eventPhase", Value::int(number));
            current
        });
        if let Some(current) = current {
            stopped = run_listeners(js, &snapshot, &current, kind, event, phase.in_shadow, fired);
        }
    }
    crate::return_snapshot(js, snapshot);
    if !stopped {
        stopped = event_stopped(js, event);
    }
    js.dispatch_depth_add(-1);
    stopped
}

fn log_listener_exception(js: Js, scope: &mut Scope<'_>, kind: &str, exception: &Value) {
    let Some(message) = message_of(scope, exception) else {
        return;
    };
    if !js.log_enabled() {
        return;
    }
    let stack = get(scope, exception, "stack");
    let stack = if stack.is_undefined() || stack.is_null() {
        None
    } else {
        scope.to_string(&stack).ok()
    };
    let line = match stack {
        Some(stack) => format!("JS error in {kind} handler: {message}\n{stack}"),
        None => format!("JS error in {kind} handler: {message}"),
    };
    js.log(&line);
}

pub(crate) fn run_listeners(
    js: Js,
    to_call: &[Rc<Listener>],
    current_target: &Value,
    kind: &str,
    event: &Value,
    in_shadow: bool,
    fired: &mut bool,
) -> bool {
    for listener in to_call {
        if listener.is_dead() {
            continue;
        }
        let Some(aborted) = js.scope(|scope| listener.signal_aborted(scope)) else {
            return false;
        };
        if aborted {
            retire(js, listener);
            continue;
        }
        js.scope(|scope| {
            let (function, this) = if scope.is_function(&listener.callback) {
                (Ok(listener.callback.clone()), current_target.clone())
            } else if listener.callback.is_object() {
                (
                    scope.get(&listener.callback, "handleEvent"),
                    listener.callback.clone(),
                )
            } else {
                (Ok(Value::undefined()), current_target.clone())
            };
            if listener.once {
                retire(js, listener);
            }
            if listener.passive {
                set(scope, event, "_passive_active", Value::boolean(true));
            }
            let guard = match &function {
                Ok(function) => handlers::push_current_event(scope, function, event, in_shadow),
                Err(_) => None,
            };
            let result = match function {
                Ok(function) => scope.call(&function, &this, core::slice::from_ref(event)),
                Err(exception) => Err(exception),
            };
            handlers::pop_current_event(scope, guard);
            if listener.passive {
                set(scope, event, "_passive_active", Value::boolean(false));
            }
            if let Err(exception) = result {
                log_listener_exception(js, scope, kind, &exception);
                let url = js.current_url();
                crate::errors::report_exception_at(js, &exception, url.as_deref(), 0, 0);
            }
        });
        *fired = true;
        js.microtask_checkpoint();
        let immediate = js
            .scope(|scope| truthy(scope, event, "_immediate_stopped"))
            .unwrap_or(false);
        if immediate {
            return true;
        }
    }
    false
}

fn event_document_for(js: Js, target: Option<Element>) -> Option<Element> {
    nearest_document(target).or_else(|| js.current_document())
}

pub(crate) fn window_for_document(js: Js, scope: &mut Scope<'_>, doc: Option<Element>) -> Value {
    if let Some(doc) = doc
        && Some(doc) != js.current_document()
        && let Some(frame) = doc.parent()
        && (is_named(Some(frame), b"iframe")
            || is_named(Some(frame), b"frame")
            || is_named(Some(frame), b"object"))
    {
        let realm = js.frame_realm_window(scope, frame);
        if realm.is_object() {
            return realm;
        }
    }
    scope.global()
}

pub(crate) fn window_listeners(
    js: Js,
    target: Option<Element>,
    kind: &str,
    event: &Value,
    capture: bool,
    at_target: bool,
    fired: &mut bool,
) -> bool {
    let event_doc = event_document_for(js, target);
    let Some(global) = js.scope(|scope| {
        let global = window_for_document(js, scope, event_doc);
        set(scope, event, "currentTarget", global.clone());
        let number = if at_target {
            2
        } else if capture {
            1
        } else {
            3
        };
        set(scope, event, "eventPhase", Value::int(number));
        global
    }) else {
        return false;
    };
    let snapshot = match event_doc {
        Some(doc) => snapshot_own(js, doc, kind, capture, true),
        None => crate::take_snapshot(js),
    };
    if !capture && handlers::fire_window_property(js, target, kind, event) {
        *fired = true;
    }
    js.dispatch_depth_add(1);
    let mut stopped = run_listeners(js, &snapshot, &global, kind, event, false, fired);
    crate::return_snapshot(js, snapshot);
    if !stopped {
        stopped = event_stopped(js, event);
    }
    js.dispatch_depth_add(-1);
    stopped
}

pub(crate) fn dispatch_window_only(
    js: Js,
    target_doc: Option<Element>,
    kind: &str,
    event: Value,
) -> bool {
    if js.is_null() {
        return false;
    }
    let budget = js.budget_enter();
    let mut fired = false;
    let mut stopped = js
        .scope(|scope| {
            set(scope, &event, "_dispatching", Value::boolean(true));
            propagation_stopped(scope, &event)
        })
        .unwrap_or(true);
    if !stopped {
        stopped = window_listeners(js, target_doc, kind, &event, true, true, &mut fired);
    }
    if !stopped {
        window_listeners(js, target_doc, kind, &event, false, true, &mut fired);
    }
    let prevented = js
        .scope(|scope| finish_event(scope, &event))
        .unwrap_or(false);
    drop(event);
    js.finish_dispatch();
    js.budget_leave(budget);
    prevented
}

pub(crate) fn path_has_active_listener(js: Js, target: Element, kind: &str) -> bool {
    let mut with_signal: Vec<Rc<Listener>> = Vec::new();
    let found = crate::with(js, |page| {
        for own in page.listeners.values() {
            for l in own {
                if l.is_dead() || l.passive || *l.kind != *kind {
                    continue;
                }
                let hit = l.window_level
                    || ancestors_and_self(target).any(|n| n.as_ptr() as usize == l.target);
                if !hit {
                    continue;
                }
                if l.signal.is_none() {
                    return true;
                }
                with_signal.push(l.clone());
            }
        }
        false
    });
    if found {
        return true;
    }
    if with_signal.is_empty() {
        return false;
    }
    let found = js
        .scope(|scope| with_signal.iter().any(|l| !l.signal_aborted(scope)))
        .unwrap_or(false);
    drop(with_signal);
    found
}
