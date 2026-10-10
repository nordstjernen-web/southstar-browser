//! Southstar — installs the event interfaces on a window or worker global: constructors, prototype methods and their chains.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ctors::{self, *};
use crate::event::{self, bind};
use crate::{WRITABLE_CONFIGURABLE, attrs, ffi, get, set};

const DRAG_EVENT_SOURCE: &str = include_str!("drag_event.js");

const PROTOTYPE_ONLY_WRITABLE: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

pub(crate) fn make_ctor(scope: &mut Scope<'_>, name: &str, arity: u32, f: NativeFn) -> Value {
    let function = scope.constructor_or_function(name, arity, f);
    let proto = scope.new_object();
    let _ = scope.define(
        &proto,
        "constructor",
        function.clone(),
        WRITABLE_CONFIGURABLE,
    );
    let _ = scope.define_to_string_tag(&proto, name);
    let _ = scope.define(&function, "prototype", proto, PROTOTYPE_ONLY_WRITABLE);
    function
}

fn bind_ctor(scope: &mut Scope<'_>, global: &Value, name: &str, arity: u32, f: NativeFn) {
    let ctor = make_ctor(scope, name, arity, f);
    set(scope, global, name, ctor);
}

fn bind_proto_fn(
    scope: &mut Scope<'_>,
    global: &Value,
    ctor_name: &str,
    name: &str,
    arity: u32,
    f: NativeFn,
) {
    let proto = crate::interface_prototype(scope, global, ctor_name);
    if proto.is_object() && !matches!(scope.has_property(&proto, name), Ok(true)) {
        bind(scope, &proto, name, arity, f);
    }
}

fn link_proto(scope: &mut Scope<'_>, global: &Value, child: &str, parent: &str) {
    let c = get(scope, global, child);
    let p = get(scope, global, parent);
    if c.is_object() && p.is_object() {
        let cp = get(scope, &c, "prototype");
        let pp = get(scope, &p, "prototype");
        if cp.is_object() && pp.is_object() {
            let _ = scope.set_prototype(&cp, &pp);
        }
    }
}

fn bind_event_proto_methods(scope: &mut Scope<'_>, proto: &Value, init_arity: u32) {
    bind(scope, proto, "initEvent", init_arity, init_event);
    bind(scope, proto, "preventDefault", 0, event::prevent_default);
    bind(scope, proto, "stopPropagation", 0, event::stop_propagation);
    bind(
        scope,
        proto,
        "stopImmediatePropagation",
        0,
        event::stop_immediate,
    );
    bind(scope, proto, "composedPath", 0, event::composed_path);
    event::define_legacy_accessors(scope, proto);
}

pub(crate) fn install_window_base(scope: &mut Scope<'_>, global: &Value) {
    bind_ctor(scope, global, "Event", 2, event_ctor);
    let proto = crate::interface_prototype(scope, global, "Event");
    if proto.is_object() {
        bind_event_proto_methods(scope, &proto, 3);
    }
    bind_ctor(scope, global, "CustomEvent", 2, custom_event_ctor);
    bind_ctor(scope, global, "MouseEvent", 2, mouse_event_ctor);
    bind_ctor(scope, global, "PointerEvent", 2, pointer_event_ctor);
    bind_ctor(scope, global, "WheelEvent", 2, wheel_event_ctor);
    bind_ctor(scope, global, "TouchEvent", 2, touch_event_ctor);
    bind_ctor(scope, global, "UIEvent", 2, ui_event_ctor);
}

const EVENT_SUBCLASSES: &[&str] = &[
    "ProgressEvent",
    "ErrorEvent",
    "HashChangeEvent",
    "PopStateEvent",
    "PageTransitionEvent",
    "BeforeUnloadEvent",
    "InputEvent",
    "DragEvent",
    "FocusEvent",
    "AnimationEvent",
    "TransitionEvent",
    "ClipboardEvent",
    "CompositionEvent",
    "CloseEvent",
    "MediaQueryListEvent",
    "BlobEvent",
    "FontFaceSetLoadEvent",
    "GamepadEvent",
    "DeviceMotionEvent",
    "DeviceOrientationEvent",
    "PromiseRejectionEvent",
    "SecurityPolicyViolationEvent",
    "TextEvent",
    "ToggleEvent",
    "FormDataEvent",
    "TrackEvent",
    "MediaStreamTrackEvent",
    "CookieChangeEvent",
    "AnimationPlaybackEvent",
    "OfflineAudioCompletionEvent",
];

const PROTO_LINKS: &[(&str, &str)] = &[
    ("CustomEvent", "Event"),
    ("MessageEvent", "Event"),
    ("StorageEvent", "Event"),
    ("SubmitEvent", "Event"),
    ("CommandEvent", "Event"),
    ("UIEvent", "Event"),
    ("MouseEvent", "UIEvent"),
    ("PointerEvent", "MouseEvent"),
    ("WheelEvent", "MouseEvent"),
    ("DragEvent", "MouseEvent"),
    ("KeyboardEvent", "UIEvent"),
    ("FocusEvent", "UIEvent"),
    ("CompositionEvent", "UIEvent"),
    ("TextEvent", "UIEvent"),
    ("InputEvent", "UIEvent"),
    ("TouchEvent", "UIEvent"),
];

const PROTO_FNS: &[(&str, &str, u32, NativeFn)] = &[
    ("UIEvent", "initUIEvent", 5, init_ui_event),
    ("MouseEvent", "initMouseEvent", 15, init_mouse_event),
    (
        "KeyboardEvent",
        "initKeyboardEvent",
        10,
        init_keyboard_event,
    ),
    ("CustomEvent", "initCustomEvent", 4, init_event),
    ("MouseEvent", "getModifierState", 1, get_modifier_state),
    ("KeyboardEvent", "getModifierState", 1, get_modifier_state),
    ("PointerEvent", "getCoalescedEvents", 0, event::empty_array),
    ("PointerEvent", "getPredictedEvents", 0, event::empty_array),
    ("InputEvent", "getTargetRanges", 0, event::empty_array),
    ("StorageEvent", "initStorageEvent", 1, storage_event_init),
    (
        "CompositionEvent",
        "initCompositionEvent",
        1,
        init_ui_event_data,
    ),
    ("TextEvent", "initTextEvent", 1, init_ui_event_data),
];

const CONSTANTS: &[(&str, &str, i32)] = &[
    ("KeyboardEvent", "DOM_KEY_LOCATION_STANDARD", 0),
    ("KeyboardEvent", "DOM_KEY_LOCATION_LEFT", 1),
    ("KeyboardEvent", "DOM_KEY_LOCATION_RIGHT", 2),
    ("KeyboardEvent", "DOM_KEY_LOCATION_NUMPAD", 3),
    ("WheelEvent", "DOM_DELTA_PIXEL", 0),
    ("WheelEvent", "DOM_DELTA_LINE", 1),
    ("WheelEvent", "DOM_DELTA_PAGE", 2),
];

fn install_drag_event_support(scope: &mut Scope<'_>) {
    let _ = scope.eval_native_script(DRAG_EVENT_SOURCE, "<drag-event>");
}

pub(crate) fn install_window(scope: &mut Scope<'_>, global: &Value) {
    bind_ctor(scope, global, "KeyboardEvent", 2, keyboard_event_ctor);
    bind_ctor(scope, global, "SubmitEvent", 2, ffi::submit_event_ctor);
    for name in EVENT_SUBCLASSES {
        bind_ctor(scope, global, name, 2, event_ctor);
    }
    bind_ctor(
        scope,
        global,
        "CookieChangeEvent",
        2,
        cookie_change_event_ctor,
    );
    bind_ctor(scope, global, "FocusEvent", 2, focus_event_ctor);
    bind_ctor(scope, global, "AnimationEvent", 2, animation_event_ctor);
    bind_ctor(scope, global, "TransitionEvent", 2, transition_event_ctor);
    bind_ctor(scope, global, "CompositionEvent", 2, composition_event_ctor);
    bind_ctor(scope, global, "TextEvent", 2, ui_event_ctor);
    bind_ctor(scope, global, "InputEvent", 2, input_event_ctor);
    bind_ctor(scope, global, "MessageEvent", 2, message_event_ctor);
    bind_ctor(
        scope,
        global,
        "ExtendableMessageEvent",
        2,
        message_event_ctor,
    );
    bind_proto_fn(
        scope,
        global,
        "MessageEvent",
        "initMessageEvent",
        1,
        message_event_init,
    );
    bind_proto_fn(
        scope,
        global,
        "ExtendableMessageEvent",
        "initMessageEvent",
        1,
        message_event_init,
    );
    bind_ctor(scope, global, "StorageEvent", 1, storage_event_ctor);
    bind_ctor(scope, global, "ToggleEvent", 2, toggle_event_ctor);
    bind_ctor(scope, global, "CommandEvent", 2, command_event_ctor);
    install_drag_event_support(scope);
    for name in EVENT_SUBCLASSES {
        link_proto(scope, global, name, "Event");
    }
    let cookie_proto = crate::interface_prototype(scope, global, "CookieChangeEvent");
    if cookie_proto.is_object() {
        let getters: [(&str, NativeFn); 2] = [
            ("changed", ctors::cookie_change_get_changed),
            ("deleted", ctors::cookie_change_get_deleted),
        ];
        for (name, f) in getters {
            let getter = scope.function(&format!("get {name}"), 0, f);
            let _ = scope.define_accessor(
                &cookie_proto,
                name,
                Some(&getter),
                None,
                Attributes::CONFIGURABLE,
            );
        }
    }
    for (child, parent) in PROTO_LINKS {
        link_proto(scope, global, child, parent);
    }
    for (ctor, name, arity, f) in PROTO_FNS {
        bind_proto_fn(scope, global, ctor, name, *arity, *f);
    }
    for (iface, name, value) in CONSTANTS {
        let ctor = get(scope, global, iface);
        let proto = if ctor.is_object() {
            get(scope, &ctor, "prototype")
        } else {
            Value::undefined()
        };
        if ctor.is_object() {
            let _ = scope.define(&ctor, name, Value::int(*value), Attributes::ENUMERABLE);
        }
        if proto.is_object() {
            let _ = scope.define(&proto, name, Value::int(*value), Attributes::ENUMERABLE);
        }
    }
}

pub(crate) fn install_worker(scope: &mut Scope<'_>, global: &Value) {
    let ctor = get(scope, global, "Event");
    let proto = if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    if proto.is_object() {
        bind_event_proto_methods(scope, &proto, 1);
        for (name, value) in [
            ("NONE", 0),
            ("CAPTURING_PHASE", 1),
            ("AT_TARGET", 2),
            ("BUBBLING_PHASE", 3),
        ] {
            let _ = scope.define(&ctor, name, Value::int(value), Attributes::ENUMERABLE);
            let _ = scope.define(&proto, name, Value::int(value), Attributes::ENUMERABLE);
        }
    }
    bind_proto_fn(
        scope,
        global,
        "MessageEvent",
        "initMessageEvent",
        1,
        message_event_init,
    );
    link_proto(scope, global, "MessageEvent", "Event");
    link_proto(scope, global, "ErrorEvent", "Event");
    link_proto(scope, global, "ExtendableEvent", "Event");
    link_proto(scope, global, "FetchEvent", "ExtendableEvent");
    link_proto(scope, global, "ExtendableMessageEvent", "ExtendableEvent");
}

pub(crate) fn install_attribute_getters(scope: &mut Scope<'_>, global: &Value) {
    attrs::install(scope, global);
}
