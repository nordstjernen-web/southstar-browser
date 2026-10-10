//! Southstar — the __nsWpt* hooks the WPT test driver uses to send wheel, touch, pointer and key input and user activation, live only while a test-driver script is injected.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, Modifiers, Pointer, arg, int_arg, number_arg};

type JsResult = Result<Value, Value>;

const HOOKS: [(&str, u32, NativeFn); 5] = [
    ("__nsWptWheel", 5, wheel),
    ("__nsWptTouch", 4, touch),
    ("__nsWptPointer", 6, pointer),
    ("__nsWptKey", 4, key),
    ("__nsWptActivate", 0, activate),
];

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    for (name, arity, f) in HOOKS {
        let function = scope.function(name, arity, f);
        let _ = scope.define(global, name, function, Attributes::METHOD);
    }
}

fn driver(scope: &Scope<'_>) -> Option<Js> {
    let js = ffi::js_of(scope);
    js.wpt_hooks_enabled().then_some(js)
}

fn target_arg(js: Js, args: &[Value]) -> Option<Element> {
    ffi::unwrap(&arg(args, 0)).or_else(|| js.current_document())
}

fn string_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Option<String> {
    scope.to_string(&arg(args, index)).ok()
}

fn wheel(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(js) = driver(scope).filter(|_| args.len() >= 5) else {
        return Ok(Value::undefined());
    };
    let Some(target) = target_arg(js, args) else {
        return Ok(Value::undefined());
    };
    let at = (number_arg(scope, args, 1), number_arg(scope, args, 2));
    let delta = (number_arg(scope, args, 3), number_arg(scope, args, 4));
    crate::scroll_touch::wheel_event(js, target, at, delta);
    Ok(Value::undefined())
}

fn touch(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(js) = driver(scope).filter(|_| args.len() >= 2) else {
        return Ok(Value::undefined());
    };
    let Some(target) = target_arg(js, args) else {
        return Ok(Value::undefined());
    };
    let Some(kind) = string_arg(scope, args, 1) else {
        return Ok(Value::undefined());
    };
    let x = if args.len() > 2 {
        number_arg(scope, args, 2)
    } else {
        0.0
    };
    let y = if args.len() > 3 {
        number_arg(scope, args, 3)
    } else {
        0.0
    };
    if !js.blocked() {
        crate::scroll_touch::touch_event(js, target, &kind, (x, y));
    }
    Ok(Value::undefined())
}

fn pointer(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(js) = driver(scope).filter(|js| args.len() >= 4 && !js.blocked()) else {
        return Ok(Value::undefined());
    };
    let target = target_arg(js, args);
    let kind = string_arg(scope, args, 1);
    let (Some(target), Some(kind)) = (target, kind) else {
        return Ok(Value::undefined());
    };
    let at = (number_arg(scope, args, 2), number_arg(scope, args, 3));
    let button = if args.len() > 4 {
        int_arg(scope, args, 4)
    } else {
        0
    };
    let buttons = if args.len() > 5 {
        int_arg(scope, args, 5)
    } else {
        0
    };
    js.note_pointer_input(true);
    let input = Pointer {
        client: at,
        page: at,
        button,
        buttons,
        modifiers: Modifiers::default(),
    };
    let (_, prevented) = crate::mouse::mouse_event(js, target, &kind, &input, None);
    if !prevented && kind == "mousedown" {
        let mut focus = None;
        let mut cur = Some(target);
        while let Some(n) = cur {
            if ffi::is_focusable(n) {
                focus = Some(n);
                break;
            }
            cur = n.parent();
        }
        js.set_focus(focus);
    }
    Ok(Value::boolean(prevented))
}

const NAMED_KEYS: [(&str, i32); 17] = [
    ("Backspace", 8),
    ("Tab", 9),
    ("Enter", 13),
    ("Shift", 16),
    ("Control", 17),
    ("Alt", 18),
    ("Escape", 27),
    (" ", 32),
    ("PageUp", 33),
    ("PageDown", 34),
    ("End", 35),
    ("Home", 36),
    ("ArrowLeft", 37),
    ("ArrowUp", 38),
    ("ArrowRight", 39),
    ("ArrowDown", 40),
    ("Delete", 46),
];

fn key_code(key: &str) -> i32 {
    if let Some((_, code)) = NAMED_KEYS.iter().find(|(name, _)| *name == key) {
        return *code;
    }
    match key.as_bytes() {
        [only] => i32::from(only.to_ascii_uppercase()),
        _ => 0,
    }
}

fn key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(js) = driver(scope).filter(|js| args.len() >= 3 && !js.blocked()) else {
        return Ok(Value::undefined());
    };
    let target = target_arg(js, args);
    let kind = string_arg(scope, args, 1);
    let key = string_arg(scope, args, 2);
    let shift = args.len() > 3 && scope.to_bool(&arg(args, 3));
    let (Some(target), Some(kind), Some(key)) = (target, kind, key) else {
        return Ok(Value::undefined());
    };
    js.note_pointer_input(false);
    let prevented = js.dispatch_key(target, &kind, &key, key_code(&key), shift);
    let focused = js.focused_node();
    if !prevented && kind == "keydown" {
        if key == "Escape" {
            js.process_close_request();
        } else if key == "Tab" {
            if let Some(next) = js.sequential_focus_target(shift) {
                js.set_focus(Some(next));
            }
        } else if let Some(focused) = focused {
            js.keyboard_activate(focused, &key, false);
        }
    } else if !prevented
        && kind == "keyup"
        && let Some(focused) = focused
    {
        js.keyboard_activate(focused, &key, true);
    }
    Ok(Value::boolean(prevented))
}

fn activate(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    if let Some(js) = driver(scope) {
        js.note_user_activation();
    }
    Ok(Value::undefined())
}
