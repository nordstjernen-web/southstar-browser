//! Southstar — the constructors of Event and its subclasses, and their legacy init*Event() methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::event::{self, bind};
use crate::{
    ENUMERABLE_CONFIGURABLE, HIDDEN, JsResult, arg, bool_prop, ffi, get, int_prop, number_prop,
    set, truthy,
};

fn init_of(args: &[Value]) -> Option<&Value> {
    args.get(1).filter(|init| init.is_object())
}

fn member(scope: &mut Scope<'_>, args: &[Value], key: &str) -> Value {
    match init_of(args) {
        Some(init) => get(scope, init, key),
        None => Value::undefined(),
    }
}

fn or_null(value: Value) -> Value {
    if value.is_undefined() {
        Value::null()
    } else {
        value
    }
}

fn string_or_empty(scope: &mut Scope<'_>, value: &Value) -> JsResult {
    if value.is_undefined() || value.is_null() {
        Ok(scope.string(""))
    } else {
        let text = scope.to_string(value)?;
        Ok(scope.string(&text))
    }
}

fn default_if_absent(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: Value) {
    if get(scope, object, key).is_undefined() {
        set(scope, object, key, fallback);
    }
}

fn define_error_members(scope: &mut Scope<'_>, ev: &Value, args: &[Value]) {
    let _ = scope.define(ev, "__ndErrorEvent", Value::boolean(true), HIDDEN);
    for (key, empty_string) in [
        ("message", true),
        ("filename", true),
        ("lineno", false),
        ("colno", false),
    ] {
        let value = member(scope, args, key);
        let value = match (value.is_undefined(), empty_string) {
            (true, true) => scope.string(""),
            (true, false) => Value::int(0),
            _ => value,
        };
        set(scope, ev, key, value);
    }
    let error = member(scope, args, "error");
    set(scope, ev, "error", error);
}

pub(crate) fn event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if this.is_undefined() {
        return Err(scope.type_error("Event constructor requires 'new'"));
    }
    let Some(kind) = args.first() else {
        return Err(scope.type_error("Event constructor requires a type"));
    };
    let kind = scope.to_string(kind)?;
    let (mut bubbles, mut cancelable, mut composed) = (false, false, false);
    if let Some(init) = init_of(args) {
        bubbles = bool_prop(scope, init, "bubbles").0;
        cancelable = bool_prop(scope, init, "cancelable").0;
        composed = bool_prop(scope, init, "composed").0;
    }
    let proto = if this.is_object() {
        get(scope, this, "prototype")
    } else {
        Value::undefined()
    };
    let ev = ffi::new_event_with_proto(scope, &proto)?;
    let kind = scope.string(&kind);
    set(scope, &ev, "type", kind);
    let now = Value::number(ffi::realm_now_ms(scope));
    set(scope, &ev, "timeStamp", now);
    set(scope, &ev, "target", Value::null());
    set(scope, &ev, "currentTarget", Value::null());
    set(scope, &ev, "defaultPrevented", Value::boolean(false));
    set(scope, &ev, "eventPhase", Value::int(0));
    set(scope, &ev, "bubbles", Value::boolean(bubbles));
    set(scope, &ev, "cancelable", Value::boolean(cancelable));
    set(scope, &ev, "composed", Value::boolean(composed));
    event::bind_methods(scope, &ev);
    bind(
        scope,
        &ev,
        "stopImmediatePropagation",
        0,
        event::stop_immediate,
    );
    bind(scope, &ev, "composedPath", 0, event::composed_path);
    let name = if this.is_object() {
        get(scope, this, "name")
    } else {
        Value::undefined()
    };
    if name.is_string() && scope.to_string(&name).is_ok_and(|n| n == "ErrorEvent") {
        define_error_members(scope, &ev, args);
    }
    Ok(ev)
}

pub(crate) fn ui_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    let mut view = Value::null();
    let mut detail = 0;
    if let Some(init) = init_of(args) {
        view = or_null(get(scope, init, "view"));
        if !view.is_null() && !view.is_object() {
            return Err(scope.type_error("UIEvent: view must be a Window or null"));
        }
        detail = int_prop(scope, init, "detail", 0);
    }
    set(scope, &ev, "view", view);
    set(scope, &ev, "detail", Value::int(detail));
    Ok(ev)
}

pub(crate) fn custom_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    let detail = match init_of(args) {
        Some(init) => or_null(get(scope, init, "detail")),
        None => Value::null(),
    };
    set(scope, &ev, "detail", detail);
    Ok(ev)
}

fn cookie_list(scope: &mut Scope<'_>, this: &Value, key: &str) -> JsResult {
    let value = get(scope, this, key);
    Ok(if value.is_undefined() {
        scope.new_array()
    } else {
        value
    })
}

pub(crate) fn cookie_change_get_changed(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> JsResult {
    cookie_list(scope, this, "__nd_changed")
}

pub(crate) fn cookie_change_get_deleted(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> JsResult {
    cookie_list(scope, this, "__nd_deleted")
}

pub(crate) fn cookie_change_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    for name in ["changed", "deleted"] {
        let mut value = member(scope, args, name);
        if !value.is_object() {
            value = scope.new_array();
        }
        let _ = scope.define(
            &ev,
            &format!("__nd_{name}"),
            value,
            Attributes::CONFIGURABLE,
        );
    }
    Ok(ev)
}

fn timed_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    name_key: &str,
) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    let name = member(scope, args, name_key);
    let elapsed = member(scope, args, "elapsedTime");
    let pseudo = member(scope, args, "pseudoElement");
    let name = if name.is_undefined() {
        scope.string("")
    } else {
        scope.to_string_value(&name)?
    };
    let mut t = 0.0;
    if !elapsed.is_undefined() {
        t = scope.to_number(&elapsed)?;
        if !t.is_finite() {
            return Err(scope.type_error("elapsedTime is not a finite number"));
        }
    }
    let pseudo = if pseudo.is_undefined() {
        scope.string("")
    } else {
        scope.to_string_value(&pseudo)?
    };
    let enumerable = Attributes::ENUMERABLE;
    let _ = scope.define(&ev, name_key, name, enumerable);
    let _ = scope.define(&ev, "elapsedTime", Value::number(t), enumerable);
    let _ = scope.define(&ev, "pseudoElement", pseudo, enumerable);
    if name_key == "animationName" {
        let animation = match init_of(args) {
            Some(init) => or_null(get(scope, init, "animation")),
            None => Value::null(),
        };
        let _ = scope.define(&ev, "animation", animation, enumerable);
    }
    Ok(ev)
}

pub(crate) fn animation_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    timed_event_ctor(scope, this, args, "animationName")
}

pub(crate) fn transition_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    timed_event_ctor(scope, this, args, "propertyName")
}

fn init_string(scope: &mut Scope<'_>, ev: &Value, args: &[Value], key: &str) -> JsResult<()> {
    let value = match init_of(args) {
        Some(init) => scope.get(init, key)?,
        None => Value::undefined(),
    };
    let value = if value.is_undefined() {
        scope.string("")
    } else {
        scope.to_string_value(&value)?
    };
    scope.define(ev, key, value, ENUMERABLE_CONFIGURABLE)
}

fn init_source(scope: &mut Scope<'_>, ev: &Value, args: &[Value], key: &str) -> JsResult<()> {
    let value = match init_of(args) {
        Some(init) => scope.get(init, key)?,
        None => Value::undefined(),
    };
    let value = if value.is_undefined() || value.is_null() {
        Value::null()
    } else {
        if !ffi::unwrap_node(&value).is_some_and(|n| n.kind() == Kind::Element) {
            return Err(scope.type_error(&format!(
                "Failed to read the '{key}' property: value is not of type 'Element'"
            )));
        }
        value
    };
    event::define_source(scope, ev, value);
    Ok(())
}

pub(crate) fn toggle_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    init_string(scope, &ev, args, "newState")?;
    init_string(scope, &ev, args, "oldState")?;
    init_source(scope, &ev, args, "source")?;
    Ok(ev)
}

pub(crate) fn command_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    init_string(scope, &ev, args, "command")?;
    init_source(scope, &ev, args, "source")?;
    Ok(ev)
}

fn message_ports(scope: &mut Scope<'_>, input: &Value) -> JsResult {
    if input.is_null() {
        return Err(scope.type_error("MessageEvent ports must be iterable"));
    }
    let global = scope.global();
    let ports = if input.is_undefined() {
        scope.new_array()
    } else {
        let symbol = get(scope, &global, "Symbol");
        let iterator_key = get(scope, &symbol, "iterator");
        let iterator = if iterator_key.is_symbol() {
            scope.get_key(input, &iterator_key)?
        } else {
            Value::undefined()
        };
        if !scope.is_function(&iterator) {
            return Err(scope.type_error("MessageEvent ports must be iterable"));
        }
        let array = get(scope, &global, "Array");
        let from = get(scope, &array, "from");
        scope.call(&from, &array, core::slice::from_ref(input))?
    };
    let len_value = get(scope, &ports, "length");
    let len = scope.to_number(&len_value).unwrap_or(0.0) as u32;
    for i in 0..len {
        let item = scope
            .get_index(&ports, i)
            .unwrap_or_else(|_| Value::undefined());
        if !(item.is_object() && truthy(scope, &item, "_is_port")) {
            return Err(scope.type_error("MessageEvent ports must contain MessagePort objects"));
        }
    }
    let _ = scope.freeze(&ports);
    Ok(ports)
}

pub(crate) fn message_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    if let Some(init) = init_of(args) {
        for key in ["data", "source"] {
            let value = get(scope, init, key);
            if !value.is_undefined() {
                set(scope, &ev, key, value);
            }
        }
        for key in ["origin", "lastEventId"] {
            let value = get(scope, init, key);
            if !value.is_undefined() {
                let text = scope.to_string_value(&value)?;
                set(scope, &ev, key, text);
            }
        }
        let input = scope.get(init, "ports")?;
        let ports = message_ports(scope, &input)?;
        set(scope, &ev, "ports", ports);
    }
    default_if_absent(scope, &ev, "data", Value::null());
    let empty = scope.string("");
    default_if_absent(scope, &ev, "origin", empty.clone());
    default_if_absent(scope, &ev, "lastEventId", empty);
    default_if_absent(scope, &ev, "source", Value::null());
    let ports = message_ports(scope, &Value::undefined())?;
    default_if_absent(scope, &ev, "ports", ports);
    Ok(ev)
}

fn in_dispatch(scope: &mut Scope<'_>, this: &Value) -> bool {
    truthy(scope, this, "_dispatching")
}

pub(crate) fn message_event_init(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("initMessageEvent requires at least 1 argument"));
    }
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    let origin = match args.get(4) {
        Some(v) => scope.to_string_value(v)?,
        None => scope.string(""),
    };
    let last_event_id = match args.get(5) {
        Some(v) => scope.to_string_value(v)?,
        None => scope.string(""),
    };
    let ports = message_ports(scope, &arg(args, 7))?;
    let base = [
        arg(args, 0),
        args.get(1).cloned().unwrap_or(Value::boolean(false)),
        args.get(2).cloned().unwrap_or(Value::boolean(false)),
    ];
    init_event(scope, this, &base)?;
    set(
        scope,
        this,
        "data",
        args.get(3).cloned().unwrap_or_else(Value::null),
    );
    set(scope, this, "origin", origin);
    set(scope, this, "lastEventId", last_event_id);
    set(
        scope,
        this,
        "source",
        args.get(6).cloned().unwrap_or_else(Value::null),
    );
    set(scope, this, "ports", ports);
    Ok(Value::undefined())
}

fn nullable_string(scope: &mut Scope<'_>, value: &Value) -> Value {
    if value.is_undefined() || value.is_null() {
        return Value::null();
    }
    scope
        .to_string_value(value)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn storage_event_init(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(kind) = args.first() else {
        return Err(scope.type_error("initStorageEvent requires at least 1 argument"));
    };
    let kind = scope
        .to_string_value(kind)
        .unwrap_or_else(|_| Value::undefined());
    set(scope, this, "type", kind);
    let bubbles = args.get(1).is_some_and(|v| scope.to_bool(v));
    set(scope, this, "bubbles", Value::boolean(bubbles));
    let cancelable = args.get(2).is_some_and(|v| scope.to_bool(v));
    set(scope, this, "cancelable", Value::boolean(cancelable));
    for (index, key) in [(3, "key"), (4, "oldValue"), (5, "newValue")] {
        let value = match args.get(index) {
            Some(v) => nullable_string(scope, v),
            None => Value::null(),
        };
        set(scope, this, key, value);
    }
    let url = match args.get(6).filter(|v| !v.is_undefined()) {
        Some(v) => scope
            .to_string_value(v)
            .unwrap_or_else(|_| Value::undefined()),
        None => scope.string(""),
    };
    set(scope, this, "url", url);
    let area = args
        .get(7)
        .filter(|v| !v.is_undefined())
        .cloned()
        .unwrap_or_else(Value::null);
    set(scope, this, "storageArea", area);
    Ok(Value::undefined())
}

pub(crate) fn storage_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("StorageEvent constructor requires at least 1 argument"));
    }
    let ev = event_ctor(scope, this, args)?;
    if let Some(init) = init_of(args) {
        for key in ["key", "oldValue", "newValue"] {
            let value = get(scope, init, key);
            if !value.is_undefined() {
                let value = nullable_string(scope, &value);
                set(scope, &ev, key, value);
            }
        }
        let url = get(scope, init, "url");
        if !url.is_undefined() {
            let url = scope
                .to_string_value(&url)
                .unwrap_or_else(|_| Value::undefined());
            set(scope, &ev, "url", url);
        }
        let area = get(scope, init, "storageArea");
        if !area.is_undefined() {
            set(scope, &ev, "storageArea", area);
        }
    }
    default_if_absent(scope, &ev, "key", Value::null());
    default_if_absent(scope, &ev, "oldValue", Value::null());
    default_if_absent(scope, &ev, "newValue", Value::null());
    let empty = scope.string("");
    default_if_absent(scope, &ev, "url", empty);
    default_if_absent(scope, &ev, "storageArea", Value::null());
    Ok(ev)
}

pub(crate) fn get_modifier_state(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(key) = args.first() else {
        return Ok(Value::boolean(false));
    };
    let Ok(key) = scope.to_string(key) else {
        return Ok(Value::boolean(false));
    };
    let prop = match key.as_str() {
        "Control" => "ctrlKey",
        "Shift" => "shiftKey",
        "Alt" => "altKey",
        "Meta" | "OS" => "metaKey",
        _ => return Ok(Value::boolean(false)),
    };
    Ok(Value::boolean(truthy(scope, this, prop)))
}

fn apply_modifier_init(scope: &mut Scope<'_>, ev: &Value, init: Option<&Value>) {
    for key in ["shiftKey", "ctrlKey", "altKey", "metaKey"] {
        let on = init.is_some_and(|init| bool_prop(scope, init, key).0);
        set(scope, ev, key, Value::boolean(on));
    }
}

pub(crate) fn mouse_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = ui_event_ctor(scope, this, args)?;
    let (mut cx, mut cy, mut button, mut buttons, mut detail) = (0, 0, 0, 0, 0);
    let mut related = Value::null();
    let init = init_of(args);
    if let Some(init) = init {
        cx = int_prop(scope, init, "clientX", 0);
        cy = int_prop(scope, init, "clientY", 0);
        button = int_prop(scope, init, "button", 0);
        buttons = int_prop(scope, init, "buttons", 0);
        detail = int_prop(scope, init, "detail", 0);
        related = or_null(get(scope, init, "relatedTarget"));
    }
    for (key, value) in [
        ("clientX", cx),
        ("clientY", cy),
        ("pageX", cx),
        ("pageY", cy),
        ("screenX", cx),
        ("screenY", cy),
        ("offsetX", cx),
        ("offsetY", cy),
        ("movementX", 0),
        ("movementY", 0),
        ("button", button),
        ("buttons", buttons),
        ("which", button.wrapping_add(1)),
        ("detail", detail),
    ] {
        set(scope, &ev, key, Value::int(value));
    }
    set(scope, &ev, "relatedTarget", related);
    let _ = scope.define(&ev, "__ndMouseEvent", Value::boolean(true), HIDDEN);
    apply_modifier_init(scope, &ev, init);
    Ok(ev)
}

pub(crate) fn wheel_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = mouse_event_ctor(scope, this, args)?;
    let (mut dx, mut dy, mut dz, mut mode) = (0.0, 0.0, 0.0, 0);
    if let Some(init) = init_of(args) {
        dx = number_prop(scope, init, "deltaX", 0.0);
        dy = number_prop(scope, init, "deltaY", 0.0);
        dz = number_prop(scope, init, "deltaZ", 0.0);
        mode = int_prop(scope, init, "deltaMode", 0);
    }
    set(scope, &ev, "deltaX", Value::number(dx));
    set(scope, &ev, "deltaY", Value::number(dy));
    set(scope, &ev, "deltaZ", Value::number(dz));
    set(scope, &ev, "deltaMode", Value::int(mode));
    Ok(ev)
}

pub(crate) fn touch_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = event_ctor(scope, this, args)?;
    for key in ["touches", "targetTouches", "changedTouches"] {
        let mut list = member(scope, args, key);
        if !list.is_object() {
            list = scope.new_array();
        }
        set(scope, &ev, key, list);
    }
    let init = args.get(1).filter(|init| init.is_object());
    apply_modifier_init(scope, &ev, init);
    Ok(ev)
}

pub(crate) fn keyboard_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = ui_event_ctor(scope, this, args)?;
    let mut key = scope.string("");
    let mut code = scope.string("");
    let (mut key_code, mut which, mut location, mut char_code) = (0, 0, 0, 0);
    let (mut repeat, mut composing) = (false, false);
    let init = init_of(args);
    if let Some(init) = init {
        let key_value = get(scope, init, "key");
        key = string_or_empty(scope, &key_value)?;
        let code_value = get(scope, init, "code");
        code = string_or_empty(scope, &code_value)?;
        key_code = int_prop(scope, init, "keyCode", 0);
        which = int_prop(scope, init, "which", key_code);
        location = int_prop(scope, init, "location", 0);
        char_code = int_prop(scope, init, "charCode", 0);
        repeat = bool_prop(scope, init, "repeat").0;
        composing = bool_prop(scope, init, "isComposing").0;
    }
    set(scope, &ev, "key", key);
    set(scope, &ev, "code", code);
    set(scope, &ev, "keyCode", Value::int(key_code));
    set(scope, &ev, "which", Value::int(which));
    set(scope, &ev, "charCode", Value::int(char_code));
    set(scope, &ev, "location", Value::int(location));
    set(scope, &ev, "repeat", Value::boolean(repeat));
    set(scope, &ev, "isComposing", Value::boolean(composing));
    apply_modifier_init(scope, &ev, init);
    Ok(ev)
}

pub(crate) fn focus_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = ui_event_ctor(scope, this, args)?;
    let related = match init_of(args) {
        Some(init) => or_null(get(scope, init, "relatedTarget")),
        None => Value::null(),
    };
    set(scope, &ev, "relatedTarget", related);
    Ok(ev)
}

pub(crate) fn composition_event_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let ev = ui_event_ctor(scope, this, args)?;
    let data = member(scope, args, "data");
    let data = string_or_empty(scope, &data)?;
    set(scope, &ev, "data", data);
    Ok(ev)
}

pub(crate) fn input_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = ui_event_ctor(scope, this, args)?;
    let mut data = Value::null();
    let mut input_type = scope.string("");
    let mut composing = false;
    let mut transfer = Value::null();
    if let Some(init) = init_of(args) {
        data = or_null(get(scope, init, "data"));
        if !data.is_null() {
            data = scope.to_string_value(&data)?;
        }
        let kind = get(scope, init, "inputType");
        input_type = string_or_empty(scope, &kind)?;
        composing = bool_prop(scope, init, "isComposing").0;
        transfer = or_null(get(scope, init, "dataTransfer"));
    }
    set(scope, &ev, "data", data);
    set(scope, &ev, "inputType", input_type);
    set(scope, &ev, "isComposing", Value::boolean(composing));
    set(scope, &ev, "dataTransfer", transfer);
    Ok(ev)
}

pub(crate) fn pointer_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let ev = mouse_event_ctor(scope, this, args)?;
    let mut pointer_id = 0;
    let (mut width, mut height, mut pressure, mut tangential) = (1.0, 1.0, 0.0, 0.0);
    let (mut tilt_x, mut tilt_y, mut twist) = (0, 0, 0);
    let mut pointer_type = scope.string("");
    let mut primary = false;
    if let Some(init) = init_of(args) {
        pointer_id = int_prop(scope, init, "pointerId", 0);
        width = number_prop(scope, init, "width", 1.0);
        height = number_prop(scope, init, "height", 1.0);
        pressure = number_prop(scope, init, "pressure", 0.0);
        tangential = number_prop(scope, init, "tangentialPressure", 0.0);
        tilt_x = int_prop(scope, init, "tiltX", 0);
        tilt_y = int_prop(scope, init, "tiltY", 0);
        twist = int_prop(scope, init, "twist", 0);
        primary = bool_prop(scope, init, "isPrimary").0;
        let kind = get(scope, init, "pointerType");
        pointer_type = string_or_empty(scope, &kind)?;
    }
    set(scope, &ev, "pointerId", Value::int(pointer_id));
    set(scope, &ev, "width", Value::number(width));
    set(scope, &ev, "height", Value::number(height));
    set(scope, &ev, "pressure", Value::number(pressure));
    set(scope, &ev, "tangentialPressure", Value::number(tangential));
    set(scope, &ev, "tiltX", Value::int(tilt_x));
    set(scope, &ev, "tiltY", Value::int(tilt_y));
    set(scope, &ev, "twist", Value::int(twist));
    set(scope, &ev, "pointerType", pointer_type);
    set(scope, &ev, "isPrimary", Value::boolean(primary));
    Ok(ev)
}

pub(crate) fn init_event(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(kind) = args.first() else {
        return Err(scope.type_error("initEvent requires at least 1 argument"));
    };
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    if let Ok(kind) = scope.to_string(kind) {
        let kind = scope.string(&kind);
        set(scope, this, "type", kind);
    }
    let bubbles = args.get(1).is_some_and(|v| scope.to_bool(v));
    set(scope, this, "bubbles", Value::boolean(bubbles));
    let cancelable = args.get(2).is_some_and(|v| scope.to_bool(v));
    set(scope, this, "cancelable", Value::boolean(cancelable));
    if let Some(detail) = args.get(3) {
        set(scope, this, "detail", detail.clone());
    }
    set(scope, this, "_propagation_stopped", Value::boolean(false));
    set(scope, this, "_immediate_stopped", Value::boolean(false));
    set(scope, this, "defaultPrevented", Value::boolean(false));
    set(scope, this, "_initialized", Value::boolean(true));
    Ok(Value::undefined())
}

pub(crate) fn init_ui_event(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("initUIEvent requires at least 1 argument"));
    }
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    let base = [
        arg(args, 0),
        args.get(1).cloned().unwrap_or(Value::boolean(false)),
        args.get(2).cloned().unwrap_or(Value::boolean(false)),
    ];
    let _ = init_event(scope, this, &base);
    set(
        scope,
        this,
        "view",
        args.get(3).cloned().unwrap_or_else(Value::null),
    );
    set(
        scope,
        this,
        "detail",
        args.get(4).cloned().unwrap_or(Value::int(0)),
    );
    Ok(Value::undefined())
}

pub(crate) fn init_ui_event_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("1 argument required, but only 0 present."));
    }
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    let _ = init_ui_event(scope, this, &args[..args.len().min(4)]);
    let data = match args.get(4).filter(|v| !v.is_undefined()) {
        Some(v) => scope.to_string_value(v)?,
        None => scope.string(""),
    };
    set(scope, this, "data", data);
    Ok(Value::undefined())
}

pub(crate) fn init_mouse_event(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("initMouseEvent requires at least 1 argument"));
    }
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    let _ = init_ui_event(scope, this, &args[..args.len().min(5)]);
    for (i, key) in ["screenX", "screenY", "clientX", "clientY"]
        .into_iter()
        .enumerate()
    {
        let v = match args.get(5 + i) {
            Some(v) => scope.to_int32(v).unwrap_or(0),
            None => 0,
        };
        set(scope, this, key, Value::int(v));
    }
    for (i, key) in ["ctrlKey", "altKey", "shiftKey", "metaKey"]
        .into_iter()
        .enumerate()
    {
        let on = args.get(9 + i).is_some_and(|v| scope.to_bool(v));
        set(scope, this, key, Value::boolean(on));
    }
    let button = match args.get(13) {
        Some(v) => scope.to_int32(v).unwrap_or(0),
        None => 0,
    };
    set(scope, this, "button", Value::int(button));
    set(
        scope,
        this,
        "relatedTarget",
        args.get(14).cloned().unwrap_or_else(Value::null),
    );
    Ok(Value::undefined())
}

pub(crate) fn init_keyboard_event(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("initKeyboardEvent requires at least 1 argument"));
    }
    if in_dispatch(scope, this) {
        return Ok(Value::undefined());
    }
    let _ = init_ui_event(scope, this, &args[..args.len().min(4)]);
    if let Some(key) = args.get(4) {
        set(scope, this, "key", key.clone());
    }
    if let Some(location) = args.get(5) {
        let location = scope.to_int32(location).unwrap_or(0);
        set(scope, this, "location", Value::int(location));
    }
    for (i, key) in ["ctrlKey", "altKey", "shiftKey", "metaKey"]
        .into_iter()
        .enumerate()
    {
        let on = args.get(6 + i).is_some_and(|v| scope.to_bool(v));
        set(scope, this, key, Value::boolean(on));
    }
    Ok(Value::undefined())
}
