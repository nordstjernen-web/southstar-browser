//! Southstar — event objects: the engine-built events, their methods and legacy accessors, retargeted sources and composedPath().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{ALL, ENUMERABLE_CONFIGURABLE, Element, HIDDEN, JsResult, ffi, get, set, truthy};

const SHADOW_ATTR: &core::ffi::CStr = c"data-nd-shadow-root";

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn prevent_default(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !truthy(scope, this, "_passive_active") && truthy(scope, this, "cancelable") {
        set(scope, this, "defaultPrevented", Value::boolean(true));
    }
    Ok(Value::undefined())
}

pub(crate) fn mark_default_prevented(scope: &mut Scope<'_>, event: &Value) {
    if truthy(scope, event, "cancelable") {
        set(scope, event, "defaultPrevented", Value::boolean(true));
    }
}

pub(crate) fn stop_propagation(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    set(scope, this, "_propagation_stopped", Value::boolean(true));
    Ok(Value::undefined())
}

pub(crate) fn stop_immediate(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    set(scope, this, "_propagation_stopped", Value::boolean(true));
    set(scope, this, "_immediate_stopped", Value::boolean(true));
    Ok(Value::undefined())
}

fn get_cancel_bubble(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(truthy(scope, this, "_propagation_stopped")))
}

fn set_cancel_bubble(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.first().is_some_and(|v| scope.to_bool(v)) {
        set(scope, this, "_propagation_stopped", Value::boolean(true));
    }
    Ok(Value::undefined())
}

fn get_return_value(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(!truthy(scope, this, "defaultPrevented")))
}

fn set_return_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.first().is_some_and(|v| !scope.to_bool(v)) && !truthy(scope, this, "_passive_active") {
        mark_default_prevented(scope, this);
    }
    Ok(Value::undefined())
}

fn get_is_trusted(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(truthy(scope, this, "_is_trusted")))
}

fn get_src_element(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let target = get(scope, this, "target");
    Ok(if target.is_undefined() {
        Value::null()
    } else {
        target
    })
}

fn shared_accessor(scope: &mut Scope<'_>, key: &str, f: NativeFn, name: &str, arity: u32) -> Value {
    let global = scope.global();
    let cached = get(scope, &global, key);
    if scope.is_function(&cached) {
        return cached;
    }
    let function = scope.function(name, arity, f);
    let _ = scope.define(&global, key, function.clone(), HIDDEN);
    function
}

fn define_accessor(
    scope: &mut Scope<'_>,
    event: &Value,
    prop: &str,
    getter: NativeFn,
    setter: Option<NativeFn>,
) {
    let get_fn = shared_accessor(
        scope,
        &format!("__ns_event_get_{prop}"),
        getter,
        &format!("get {prop}"),
        0,
    );
    let set_fn = setter.map(|f| {
        shared_accessor(
            scope,
            &format!("__ns_event_set_{prop}"),
            f,
            &format!("set {prop}"),
            1,
        )
    });
    let _ = scope.define_accessor(
        event,
        prop,
        Some(&get_fn),
        set_fn.as_ref(),
        ENUMERABLE_CONFIGURABLE,
    );
}

fn is_shadow_root(node: Element) -> bool {
    node.kind() == Kind::Element && node.attr(SHADOW_ATTR).is_some()
}

fn ancestor_or_self(node: Element, ancestor: Element) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n == ancestor {
            return true;
        }
        cur = n.parent();
    }
    false
}

pub(crate) fn retarget_node(target: Element, current: Option<Element>) -> Element {
    let mut visible = Some(target);
    let mut depth = 0;
    while let Some(v) = visible {
        if depth >= southstar_dom::MAX_DEPTH {
            break;
        }
        depth += 1;
        let mut root = Some(v);
        while let Some(r) = root {
            if is_shadow_root(r) {
                break;
            }
            root = r.parent();
        }
        let Some(root) = root else {
            return v;
        };
        if current.is_some_and(|c| ancestor_or_self(c, root)) {
            return v;
        }
        visible = root.parent();
    }
    target
}

fn get_source(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let source = get(scope, this, "__nd_source");
    let Some(node) = ffi::unwrap_node(&source) else {
        return Ok(Value::null());
    };
    let current = get(scope, this, "currentTarget");
    let visible = retarget_node(node, ffi::unwrap_node(&current));
    if visible == node {
        Ok(source)
    } else {
        Ok(ffi::wrap_node(scope, visible))
    }
}

pub(crate) fn define_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    let _ = scope.define(
        event,
        "__nd_source",
        source,
        southstar_js_engine::Attributes::CONFIGURABLE,
    );
    define_accessor(scope, event, "source", get_source, None);
}

const INTERFACES: &[(&str, &str)] = &[
    ("message", "MessageEvent"),
    ("messageerror", "MessageEvent"),
    ("error", "ErrorEvent"),
    ("hashchange", "HashChangeEvent"),
    ("popstate", "PopStateEvent"),
    ("storage", "StorageEvent"),
    ("pageshow", "PageTransitionEvent"),
    ("pagehide", "PageTransitionEvent"),
    ("unhandledrejection", "PromiseRejectionEvent"),
    ("rejectionhandled", "PromiseRejectionEvent"),
    ("click", "PointerEvent"),
    ("auxclick", "PointerEvent"),
    ("contextmenu", "PointerEvent"),
    ("pointerdown", "PointerEvent"),
    ("pointerup", "PointerEvent"),
    ("pointermove", "PointerEvent"),
    ("pointerover", "PointerEvent"),
    ("pointerout", "PointerEvent"),
    ("pointerenter", "PointerEvent"),
    ("pointerleave", "PointerEvent"),
    ("pointercancel", "PointerEvent"),
    ("mousedown", "MouseEvent"),
    ("mouseup", "MouseEvent"),
    ("mousemove", "MouseEvent"),
    ("mouseover", "MouseEvent"),
    ("mouseout", "MouseEvent"),
    ("mouseenter", "MouseEvent"),
    ("mouseleave", "MouseEvent"),
    ("dblclick", "MouseEvent"),
    ("focus", "FocusEvent"),
    ("blur", "FocusEvent"),
    ("focusin", "FocusEvent"),
    ("focusout", "FocusEvent"),
    ("submit", "SubmitEvent"),
    ("wheel", "WheelEvent"),
    ("mousewheel", "WheelEvent"),
    ("keydown", "KeyboardEvent"),
    ("keyup", "KeyboardEvent"),
    ("keypress", "KeyboardEvent"),
    ("copy", "ClipboardEvent"),
    ("cut", "ClipboardEvent"),
    ("paste", "ClipboardEvent"),
    ("touchstart", "TouchEvent"),
    ("touchmove", "TouchEvent"),
    ("touchend", "TouchEvent"),
    ("touchcancel", "TouchEvent"),
    ("drag", "DragEvent"),
    ("dragstart", "DragEvent"),
    ("dragend", "DragEvent"),
    ("dragenter", "DragEvent"),
    ("dragover", "DragEvent"),
    ("dragleave", "DragEvent"),
    ("drop", "DragEvent"),
    ("animationstart", "AnimationEvent"),
    ("animationend", "AnimationEvent"),
    ("animationiteration", "AnimationEvent"),
    ("animationcancel", "AnimationEvent"),
    ("webkitAnimationStart", "AnimationEvent"),
    ("webkitAnimationEnd", "AnimationEvent"),
    ("webkitAnimationIteration", "AnimationEvent"),
    ("transitionrun", "TransitionEvent"),
    ("transitionstart", "TransitionEvent"),
    ("transitionend", "TransitionEvent"),
    ("transitioncancel", "TransitionEvent"),
    ("webkitTransitionEnd", "TransitionEvent"),
];

fn interface_for(scope: &mut Scope<'_>, event: &Value) -> &'static str {
    let kind = get(scope, event, "type");
    if !kind.is_string() {
        return "Event";
    }
    let Ok(kind) = scope.to_string(&kind) else {
        return "Event";
    };
    INTERFACES
        .iter()
        .find(|(t, _)| *t == kind)
        .map_or("Event", |(_, iface)| iface)
}

pub(crate) fn define_legacy_accessors(scope: &mut Scope<'_>, object: &Value) {
    define_accessor(
        scope,
        object,
        "cancelBubble",
        get_cancel_bubble,
        Some(set_cancel_bubble),
    );
    define_accessor(
        scope,
        object,
        "returnValue",
        get_return_value,
        Some(set_return_value),
    );
    define_accessor(scope, object, "srcElement", get_src_element, None);
}

pub(crate) fn define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    let global = scope.global();
    let current = scope
        .get_prototype(event)
        .unwrap_or_else(|_| Value::undefined());
    let object_proto = crate::interface_prototype(scope, &global, "Object");
    if current.same_object(&object_proto) {
        let iface = interface_for(scope, event);
        let mut ctor = get(scope, &global, iface);
        if !ctor.is_object() {
            ctor = get(scope, &global, "Event");
        }
        if ctor.is_object() {
            let proto = get(scope, &ctor, "prototype");
            if proto.is_object() && !proto.same_object(event) {
                let _ = scope.set_prototype(event, &proto);
            }
        }
    }
    define_accessor(scope, event, "isTrusted", get_is_trusted, None);
    define_legacy_accessors(scope, event);
    let key = scope.string("timeStamp");
    if let Ok(None) = scope.own_property(event, &key) {
        let now = Value::number(ffi::realm_now_ms(scope));
        let _ = scope.define(event, "timeStamp", now, ALL);
    }
}

const COMPOSED_TYPES: &[&str] = &[
    "auxclick",
    "beforeinput",
    "blur",
    "click",
    "compositionend",
    "compositionstart",
    "compositionupdate",
    "contextmenu",
    "copy",
    "cut",
    "dblclick",
    "focus",
    "focusin",
    "focusout",
    "input",
    "keydown",
    "keypress",
    "keyup",
    "mousedown",
    "mousemove",
    "mouseout",
    "mouseover",
    "mouseup",
    "paste",
    "pointercancel",
    "pointerdown",
    "pointermove",
    "pointerout",
    "pointerover",
    "pointerup",
    "touchcancel",
    "touchend",
    "touchmove",
    "touchstart",
    "wheel",
];

pub(crate) fn bind_methods(scope: &mut Scope<'_>, event: &Value) {
    bind(scope, event, "preventDefault", 0, prevent_default);
    bind(scope, event, "stopPropagation", 0, stop_propagation);
    define_cancel_bubble(scope, event);
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str, target: Option<Element>) -> Value {
    let event = ffi::new_event(scope);
    let kind_value = scope.string(kind);
    set(scope, &event, "type", kind_value);
    let target = match target {
        Some(node) => ffi::wrap_node(scope, node),
        None => Value::null(),
    };
    set(scope, &event, "target", target);
    set(scope, &event, "defaultPrevented", Value::boolean(false));
    set(scope, &event, "bubbles", Value::boolean(true));
    set(scope, &event, "cancelable", Value::boolean(true));
    set(
        scope,
        &event,
        "composed",
        Value::boolean(COMPOSED_TYPES.contains(&kind)),
    );
    set(scope, &event, "eventPhase", Value::int(0));
    bind_methods(scope, &event);
    set(scope, &event, "_is_trusted", Value::boolean(true));
    bind(scope, &event, "stopImmediatePropagation", 0, stop_immediate);
    bind(scope, &event, "composedPath", 0, composed_path);
    event
}

pub(crate) fn composed_path(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let path = scope.new_array();
    if !truthy(scope, this, "_dispatching") {
        return Ok(path);
    }
    if let Some((nodes, window)) = ffi::dispatch_path(scope) {
        let mut index = 0u32;
        for node in nodes {
            let wrapper = ffi::wrap_node(scope, node);
            let _ = scope.set_index(&path, index, wrapper);
            index += 1;
        }
        if window {
            let global = scope.global();
            let _ = scope.set_index(&path, index, global);
        }
        return Ok(path);
    }
    let target = get(scope, this, "target");
    let Some(node) = ffi::unwrap_node(&target) else {
        let current = get(scope, this, "currentTarget");
        if current.is_object() {
            let _ = scope.set_index(&path, 0, current);
        }
        return Ok(path);
    };
    let mut index = 0u32;
    let mut saw_document = false;
    let mut cur = Some(node);
    while let Some(n) = cur {
        let wrapper = ffi::wrap_node(scope, n);
        let _ = scope.set_index(&path, index, wrapper);
        index += 1;
        saw_document |= n.kind() == Kind::Document;
        cur = n.parent();
    }
    if saw_document {
        let global = scope.global();
        let _ = scope.set_index(&path, index, global);
    }
    Ok(path)
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    let global = scope.global();
    let ctor = get(scope, &global, iface);
    if ctor.is_object() {
        let proto = get(scope, &ctor, "prototype");
        if proto.is_object() {
            let _ = scope.set_prototype(event, &proto);
        }
    }
}

pub(crate) fn empty_array(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.new_array())
}
