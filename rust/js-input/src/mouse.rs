//! Southstar — mouse and pointer events from the shell, with coordinates mapped into the target's frame and the user-activation and light-dismiss steps they trigger.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::Value;

use crate::ffi::{self, Js};
use crate::{Element, Pointer, put};

const ACTIVATING: [&str; 6] = [
    "mousedown",
    "pointerdown",
    "mouseup",
    "pointerup",
    "click",
    "dblclick",
];

const BUTTON_CHANGE: [&str; 8] = [
    "pointerdown",
    "pointerup",
    "mousedown",
    "mouseup",
    "click",
    "dblclick",
    "auxclick",
    "contextmenu",
];

fn owner_iframe(target: Element) -> Option<Element> {
    let mut cur = target.parent();
    while let Some(n) = cur {
        if n.element_name() == Some(b"iframe") {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

struct Coordinates {
    client: (f64, f64),
    page: (f64, f64),
    screen: (f64, f64),
    offset: (f64, f64),
}

fn coordinates(js: Js, target: Element, input: &Pointer) -> Coordinates {
    let root = js.layout_root();
    let owner_box = root.and_then(|root| owner_iframe(target).and_then(|f| ffi::find_box(root, f)));
    let frame = owner_box.map_or((0.0, 0.0), |b| {
        let (x, y) = ffi::visual_border_box(b);
        let (border, padding) = (b.border(), b.padding());
        (x + border.left + padding.left, y + border.top + padding.top)
    });
    let screen = input.client;
    let (client, page) = if owner_box.is_some() {
        let client = (input.page.0 - frame.0, input.page.1 - frame.1);
        (client, client)
    } else {
        (input.client, input.page)
    };
    let offset = root
        .and_then(|root| ffi::find_box(root, target))
        .map_or(client, |b| {
            let (bx, by) = ffi::visual_border_box(b);
            let border = b.border();
            (
                page.0 + frame.0 - bx - border.left,
                page.1 + frame.1 - by - border.top,
            )
        });
    Coordinates {
        client,
        page,
        screen,
        offset,
    }
}

pub(crate) fn mouse_event(
    js: Js,
    target: Element,
    kind: &str,
    input: &Pointer,
    related: Option<Element>,
) -> (bool, bool) {
    if js.blocked() {
        return (false, false);
    }
    if ACTIVATING.contains(&kind) {
        js.note_user_activation();
    }
    let at = coordinates(js, target, input);
    let is_pointer = kind.starts_with("pointer");
    let button_change = BUTTON_CHANGE.contains(&kind);
    let Some(event) = js.realm_scope(target, |scope| {
        let event = ffi::make_event(scope, kind, target);
        if kind.ends_with("enter") || kind.ends_with("leave") {
            put(scope, &event, "bubbles", Value::boolean(false));
            put(scope, &event, "cancelable", Value::boolean(false));
        }
        put(scope, &event, "clientX", Value::number(at.client.0));
        put(scope, &event, "clientY", Value::number(at.client.1));
        put(scope, &event, "x", Value::number(at.client.0));
        put(scope, &event, "y", Value::number(at.client.1));
        put(scope, &event, "pageX", Value::number(at.page.0));
        put(scope, &event, "pageY", Value::number(at.page.1));
        put(scope, &event, "screenX", Value::number(at.screen.0));
        put(scope, &event, "screenY", Value::number(at.screen.1));
        put(scope, &event, "offsetX", Value::number(at.offset.0));
        put(scope, &event, "offsetY", Value::number(at.offset.1));
        put(scope, &event, "layerX", Value::number(at.offset.0));
        put(scope, &event, "layerY", Value::number(at.offset.1));
        let now = ffi::realm_now_ms(scope);
        put(scope, &event, "timeStamp", Value::number(now));
        let (move_x, move_y) = match kind {
            "mousemove" => crate::movement(js, 0, at.client),
            "pointermove" => crate::movement(js, 1, at.client),
            _ => (0, 0),
        };
        put(scope, &event, "movementX", Value::int(move_x));
        put(scope, &event, "movementY", Value::int(move_y));
        let button = if is_pointer && !button_change {
            -1
        } else {
            input.button
        };
        put(scope, &event, "button", Value::int(button));
        put(scope, &event, "buttons", Value::int(input.buttons));
        put(
            scope,
            &event,
            "which",
            Value::int(input.button.wrapping_add(1)),
        );
        let detail = if kind == "dblclick" {
            2
        } else if !is_pointer && button_change {
            1
        } else {
            0
        };
        put(scope, &event, "detail", Value::int(detail));
        let m = input.modifiers;
        put(scope, &event, "shiftKey", Value::boolean(m.shift));
        put(scope, &event, "ctrlKey", Value::boolean(m.ctrl));
        put(scope, &event, "altKey", Value::boolean(m.alt));
        put(scope, &event, "metaKey", Value::boolean(m.meta));
        let related = match related {
            Some(related) => ffi::wrap(scope, related),
            None => Value::null(),
        };
        put(scope, &event, "relatedTarget", related);
        let view = scope.global();
        put(scope, &event, "view", view);
        let click_like = matches!(kind, "click" | "auxclick" | "contextmenu");
        if is_pointer || click_like {
            put(scope, &event, "pointerId", Value::int(1));
            let pointer_type = scope.string("mouse");
            put(scope, &event, "pointerType", pointer_type);
            put(scope, &event, "isPrimary", Value::boolean(is_pointer));
            let pressure = if is_pointer && input.buttons != 0 {
                0.5
            } else {
                0.0
            };
            put(scope, &event, "pressure", Value::number(pressure));
            put(scope, &event, "width", Value::int(1));
            put(scope, &event, "height", Value::int(1));
            put(scope, &event, "tangentialPressure", Value::int(0));
            put(scope, &event, "tiltX", Value::int(0));
            put(scope, &event, "tiltY", Value::int(0));
            put(scope, &event, "twist", Value::int(0));
            put(
                scope,
                &event,
                "altitudeAngle",
                Value::number(core::f64::consts::FRAC_PI_2),
            );
            put(scope, &event, "azimuthAngle", Value::int(0));
        }
        event
    }) else {
        return (false, false);
    };
    if kind == "pointerdown" || kind == "pointerup" {
        js.light_dismiss(target, kind == "pointerup");
    }
    js.dispatch_built(target, kind, event)
}
