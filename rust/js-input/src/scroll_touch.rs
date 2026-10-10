//! Southstar — wheel, legacy mousewheel and touch events, cancelable only when a non-passive listener is on their path.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, set};

fn positioned_event(
    js: Js,
    scope: &mut Scope<'_>,
    kind: &str,
    target: Element,
    (x, y): (f64, f64),
) -> Value {
    let event = ffi::make_event(scope, kind, target);
    let cancelable = js.path_has_active_listener(target, kind);
    set(scope, &event, "bubbles", Value::boolean(true));
    set(scope, &event, "cancelable", Value::boolean(cancelable));
    set(scope, &event, "composed", Value::boolean(true));
    set(scope, &event, "clientX", Value::number(x));
    set(scope, &event, "clientY", Value::number(y));
    event
}

fn wheel_type(js: Js, target: Element, kind: &str, at: (f64, f64), (dx, dy): (f64, f64)) {
    let Some(event) = js.scope(|scope| {
        let event = positioned_event(js, scope, kind, target, at);
        set(scope, &event, "screenX", Value::number(at.0));
        set(scope, &event, "screenY", Value::number(at.1));
        if kind == "mousewheel" {
            set(scope, &event, "wheelDelta", Value::number(-dy));
            set(scope, &event, "wheelDeltaX", Value::number(-dx));
            set(scope, &event, "wheelDeltaY", Value::number(-dy));
        } else {
            set(scope, &event, "deltaX", Value::number(dx));
            set(scope, &event, "deltaY", Value::number(dy));
            set(scope, &event, "deltaZ", Value::number(0.0));
            set(scope, &event, "deltaMode", Value::int(0));
        }
        event
    }) else {
        return;
    };
    js.dispatch_built(target, kind, event);
}

pub(crate) fn wheel_event(js: Js, target: Element, at: (f64, f64), delta: (f64, f64)) {
    if js.blocked() {
        return;
    }
    wheel_type(js, target, "wheel", at, delta);
    if !js.blocked() {
        wheel_type(js, target, "mousewheel", at, delta);
    }
}

pub(crate) fn touch_event(js: Js, target: Element, kind: &str, at: (f64, f64)) {
    if kind == "touchstart" || kind == "touchend" {
        js.note_user_activation();
    }
    let Some(event) = js.scope(|scope| {
        let event = positioned_event(js, scope, kind, target, at);
        for list in ["touches", "targetTouches", "changedTouches"] {
            let array = scope.new_array();
            set(scope, &event, list, array);
        }
        event
    }) else {
        return;
    };
    js.dispatch_built(target, kind, event);
}
