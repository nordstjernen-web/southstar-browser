//! Southstar — the events the browser builds and dispatches itself for key presses, text input, toggles and resource loads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Value};

use crate::ffi::{self, Js};
use crate::{Element, set};

const READONLY: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

pub(crate) struct KeyPress<'a> {
    pub key: Option<&'a str>,
    pub code: Option<&'a str>,
    pub key_code: i32,
    pub char_code: i32,
    pub modifiers: Modifiers,
}

fn blocked(js: Js) -> bool {
    js.halted() || js.in_pump()
}

pub(crate) fn key_event(js: Js, target: Element, kind: &str, press: &KeyPress<'_>) -> (bool, bool) {
    if blocked(js) {
        return (false, false);
    }
    if kind == "keydown" && press.key != Some("Escape") {
        js.note_user_activation();
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, Some(target));
        let key = scope.string(press.key.unwrap_or(""));
        set(scope, &event, "key", key);
        let code = scope.string(press.code.unwrap_or(""));
        set(scope, &event, "code", code);
        set(scope, &event, "keyCode", Value::int(press.key_code));
        let which = if press.char_code != 0 {
            press.char_code
        } else {
            press.key_code
        };
        set(scope, &event, "which", Value::int(which));
        set(scope, &event, "charCode", Value::int(press.char_code));
        set(
            scope,
            &event,
            "shiftKey",
            Value::boolean(press.modifiers.shift),
        );
        set(
            scope,
            &event,
            "ctrlKey",
            Value::boolean(press.modifiers.ctrl),
        );
        set(scope, &event, "altKey", Value::boolean(press.modifiers.alt));
        set(
            scope,
            &event,
            "metaKey",
            Value::boolean(press.modifiers.meta),
        );
        set(scope, &event, "repeat", Value::boolean(false));
        event
    }) else {
        return (false, false);
    };
    crate::dispatch::dispatch_built(js, target, kind, event)
}

pub(crate) fn input_event(
    js: Js,
    target: Element,
    kind: &str,
    input_type: Option<&str>,
    data: Option<&str>,
) -> (bool, bool) {
    if blocked(js) {
        return (false, false);
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, Some(target));
        ffi::adopt_interface(scope, &event, c"InputEvent");
        set(scope, &event, "bubbles", Value::boolean(true));
        set(
            scope,
            &event,
            "cancelable",
            Value::boolean(kind == "beforeinput"),
        );
        let input_type = scope.string(input_type.unwrap_or(""));
        set(scope, &event, "inputType", input_type);
        let data = match data {
            Some(data) => scope.string(data),
            None => Value::null(),
        };
        set(scope, &event, "data", data);
        set(scope, &event, "isComposing", Value::boolean(false));
        set(scope, &event, "dataTransfer", Value::null());
        event
    }) else {
        return (false, false);
    };
    crate::dispatch::dispatch_built(js, target, kind, event)
}

pub(crate) struct Toggle<'a> {
    pub old_state: Option<&'a str>,
    pub new_state: Option<&'a str>,
    pub cancelable: bool,
    pub source: Option<Element>,
}

pub(crate) fn toggle_event(
    js: Js,
    target: Element,
    kind: &str,
    toggle: &Toggle<'_>,
) -> (bool, bool) {
    if blocked(js) {
        return (false, false);
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, Some(target));
        set(scope, &event, "bubbles", Value::boolean(false));
        set(
            scope,
            &event,
            "cancelable",
            Value::boolean(toggle.cancelable),
        );
        let old_state = scope.string(toggle.old_state.unwrap_or(""));
        let _ = scope.define(&event, "oldState", old_state, READONLY);
        let new_state = scope.string(toggle.new_state.unwrap_or(""));
        let _ = scope.define(&event, "newState", new_state, READONLY);
        let source = match toggle.source {
            Some(source) => ffi::wrap(scope, source),
            None => Value::null(),
        };
        ffi::define_source(scope, &event, source);
        ffi::adopt_interface(scope, &event, c"ToggleEvent");
        event
    }) else {
        return (false, false);
    };
    crate::dispatch::dispatch_built(js, target, kind, event)
}

pub(crate) fn resource_event(js: Js, target: Element, kind: &str) -> bool {
    if blocked(js) {
        return false;
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, Some(target));
        set(scope, &event, "bubbles", Value::boolean(false));
        set(scope, &event, "cancelable", Value::boolean(false));
        event
    }) else {
        return false;
    };
    crate::dispatch::dispatch_built(js, target, kind, event).0
}
