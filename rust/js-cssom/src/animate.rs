//! Southstar — Element.animate(): the already-finished Animation it returns and the finish event queued for it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi;

const ANIMATION_METHODS: [(&str, u32); 8] = [
    ("play", 0),
    ("pause", 0),
    ("cancel", 0),
    ("finish", 0),
    ("reverse", 0),
    ("commitStyles", 0),
    ("persist", 0),
    ("updatePlaybackRate", 1),
];

fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn get_or_undefined(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

fn call_listener(scope: &mut Scope<'_>, callback: &Value, animation: &Value, event: &Value) {
    if scope.is_function(callback) {
        let _ = scope.call(callback, animation, core::slice::from_ref(event));
    }
}

fn finish_job(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(animation) = args.first() else {
        return Ok(Value::undefined());
    };
    let event = ffi::new_event(scope);
    let kind = scope.string("finish");
    let _ = scope.set(&event, "type", kind);
    let _ = scope.set(&event, "target", animation.clone());
    let _ = scope.set(&event, "currentTarget", animation.clone());
    let on_finish = get_or_undefined(scope, animation, "onfinish");
    call_listener(scope, &on_finish, animation, &event);
    let listeners = get_or_undefined(scope, animation, "_listeners");
    if listeners.is_array() {
        let length = get_or_undefined(scope, &listeners, "length");
        let length = scope.to_number(&length).unwrap_or(0.0) as u32;
        for index in 0..length {
            let entry = scope
                .get_index(&listeners, index)
                .unwrap_or_else(|_| Value::undefined());
            let kind = get_or_undefined(scope, &entry, "type");
            if scope.to_string(&kind).is_ok_and(|k| k == "finish") {
                let callback = get_or_undefined(scope, &entry, "cb");
                call_listener(scope, &callback, animation, &event);
            }
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn element_animate(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let animation = scope.new_object();
    for (name, arity) in ANIMATION_METHODS {
        let method = scope.function(name, arity, noop);
        let _ = scope.set(&animation, name, method);
    }
    let add = ffi::port_add_event_listener(scope);
    let _ = scope.set(&animation, "addEventListener", add);
    let remove = ffi::port_remove_event_listener(scope);
    let _ = scope.set(&animation, "removeEventListener", remove);
    let listeners = scope.new_array();
    let play_state = scope.string("finished");
    let id = scope.string("");
    let fields = [
        ("_listeners", listeners),
        ("onfinish", Value::null()),
        ("oncancel", Value::null()),
        ("playState", play_state),
        ("playbackRate", Value::int(1)),
        ("currentTime", Value::number(0.0)),
        ("startTime", Value::null()),
        ("pending", Value::boolean(false)),
        ("id", id),
    ];
    for (key, value) in fields {
        let _ = scope.set(&animation, key, value);
    }
    let (finished, resolve, _reject) = scope.new_promise()?;
    let _ = scope.call(
        &resolve,
        &Value::undefined(),
        core::slice::from_ref(&animation),
    );
    let _ = scope.set(&animation, "finished", finished.clone());
    let _ = scope.set(&animation, "ready", finished);
    let job = scope.function("", 1, finish_job);
    let _ = scope.enqueue_call(&job, core::slice::from_ref(&animation));
    Ok(animation)
}
