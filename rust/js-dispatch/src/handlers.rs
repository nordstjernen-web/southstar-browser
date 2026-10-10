//! Southstar — event handlers: inline on* content attributes compiled per realm, the IDL handler properties and their accessors, the window's and document's handlers, and window.event while a listener runs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::{
    Element, JsResult, get, is_document, is_named, message_of, nearest_document, set, truthy,
    with_c_key, with_key,
};

pub(crate) const HANDLER_NAMES: &[&str] = &[
    "onabort",
    "onanimationcancel",
    "onanimationend",
    "onanimationiteration",
    "onanimationstart",
    "onauxclick",
    "onbeforeinput",
    "onbeforematch",
    "onbeforetoggle",
    "onblur",
    "oncancel",
    "oncanplay",
    "oncanplaythrough",
    "onchange",
    "onclick",
    "onclose",
    "oncommand",
    "oncontextlost",
    "oncontextmenu",
    "oncontextrestored",
    "oncopy",
    "oncuechange",
    "oncut",
    "ondblclick",
    "ondrag",
    "ondragend",
    "ondragenter",
    "ondragleave",
    "ondragover",
    "ondragstart",
    "ondrop",
    "ondurationchange",
    "onemptied",
    "onended",
    "onfocus",
    "onformdata",
    "ongamepadconnected",
    "ongamepaddisconnected",
    "ongotpointercapture",
    "oninput",
    "oninvalid",
    "onkeydown",
    "onkeypress",
    "onkeyup",
    "onload",
    "onloadeddata",
    "onloadedmetadata",
    "onloadstart",
    "onlostpointercapture",
    "onmousedown",
    "onmouseenter",
    "onmouseleave",
    "onmousemove",
    "onmouseout",
    "onmouseover",
    "onmouseup",
    "onpaste",
    "onpause",
    "onplay",
    "onplaying",
    "onpointercancel",
    "onpointerdown",
    "onpointerenter",
    "onpointerleave",
    "onpointermove",
    "onpointerout",
    "onpointerover",
    "onpointerrawupdate",
    "onpointerup",
    "onprogress",
    "onratechange",
    "onreset",
    "onresize",
    "onscroll",
    "onscrollend",
    "onsecuritypolicyviolation",
    "onseeked",
    "onseeking",
    "onselect",
    "onselectionchange",
    "onslotchange",
    "onstalled",
    "onsubmit",
    "onsuspend",
    "ontimeupdate",
    "ontoggle",
    "ontransitioncancel",
    "ontransitionend",
    "ontransitionrun",
    "ontransitionstart",
    "onvolumechange",
    "onwaiting",
    "onwebkitanimationend",
    "onwebkitanimationiteration",
    "onwebkitanimationstart",
    "onwebkittransitionend",
    "onwheel",
];

const COMPILED_PREFIX: &str = "\u{fffd}";
const SOURCE_PREFIX: &str = "\u{fffd}src:";

pub(crate) struct CurrentEvent {
    global: Value,
    previous: Value,
}

pub(crate) fn global_is_window(scope: &mut Scope<'_>, global: &Value) -> bool {
    scope.has_property(global, "event").unwrap_or(false)
}

pub(crate) fn push_current_event(
    scope: &mut Scope<'_>,
    function: &Value,
    event: &Value,
    in_shadow: bool,
) -> Option<CurrentEvent> {
    if in_shadow || !scope.is_function(function) {
        return None;
    }
    let realm = southstar_js_engine::quickjs::function_realm(scope, function).ok()?;
    let realm_global = ffi::global_of(realm);
    if !global_is_window(scope, &realm_global) {
        return None;
    }
    Some(set_current_event(scope, realm_global, event))
}

pub(crate) fn set_current_event(
    scope: &mut Scope<'_>,
    global: Value,
    event: &Value,
) -> CurrentEvent {
    let previous = get(scope, &global, "event");
    set(scope, &global, "event", event.clone());
    CurrentEvent { global, previous }
}

pub(crate) fn pop_current_event(scope: &mut Scope<'_>, guard: Option<CurrentEvent>) {
    if let Some(guard) = guard {
        set(scope, &guard.global, "event", guard.previous);
    }
}

pub(crate) fn call_on_handler(
    scope: &mut Scope<'_>,
    handler: &Value,
    this: &Value,
    kind: &str,
    event: &Value,
    window_like: bool,
) -> (JsResult, bool) {
    let guard = push_current_event(scope, handler, event, false);
    if window_like && kind == "error" && truthy(scope, event, "__ndErrorEvent") {
        let args = [
            get(scope, event, "message"),
            get(scope, event, "filename"),
            get(scope, event, "lineno"),
            get(scope, event, "colno"),
            get(scope, event, "error"),
        ];
        let ret = scope.call(handler, this, &args);
        if let Ok(value) = &ret
            && value.is_bool()
            && scope.to_bool(value)
        {
            ffi::mark_default_prevented(scope, event);
        }
        pop_current_event(scope, guard);
        return (ret, true);
    }
    let ret = scope.call(handler, this, core::slice::from_ref(event));
    pop_current_event(scope, guard);
    (ret, false)
}

fn returned_false(scope: &mut Scope<'_>, value: &Value) -> bool {
    value.is_bool() && !scope.to_bool(value)
}

pub(crate) fn is_window_handler_holder(node: Element) -> bool {
    is_named(Some(node), b"body") || is_named(Some(node), b"frameset")
}

pub(crate) fn is_window_reflected(kind: &str) -> bool {
    matches!(
        kind,
        "load"
            | "unload"
            | "beforeunload"
            | "pageshow"
            | "pagehide"
            | "hashchange"
            | "popstate"
            | "resize"
            | "storage"
            | "offline"
            | "online"
            | "message"
            | "afterprint"
            | "beforeprint"
            | "languagechange"
            | "error"
    )
}

pub(crate) fn compile_inline(scope: &mut Scope<'_>, body: &str) -> JsResult {
    let source = ["(function(event){\n", body, "\n})"].concat();
    scope.eval_script(&source, "<inline>")
}

pub(crate) fn fire_inline(js: Js, target: Element, kind: &str, event: &Value) -> bool {
    if !target.is_element() {
        return false;
    }
    let has_body = with_c_key(&["on", kind], |name| {
        target.attr(name).is_some_and(|body| !body.is_empty())
    });
    if !has_body {
        return false;
    }
    let target_doc = nearest_document(target.parent());
    let frame = target_doc.and_then(|doc| doc.parent());
    let mut realm = frame.map_or(core::ptr::null_mut(), |frame| js.frame_context(frame));
    if realm.is_null() {
        if frame.is_some_and(|frame| js.iframe_is_cross_origin(frame)) {
            return false;
        }
        realm = js.main_realm();
        if realm.is_null() {
            realm = js.ctx();
        }
    }
    let saved_ctx = js.ctx();
    let saved_doc = js.current_document();
    js.set_realm(realm, target_doc.or(saved_doc));
    let fired = fire_inline_in_realm(js, target, kind, event);
    js.set_realm(saved_ctx, saved_doc);
    fired
}

fn fire_inline_in_realm(js: Js, target: Element, kind: &str, event: &Value) -> bool {
    let attr_name = ["on", kind].concat();
    let body = with_c_key(&[&attr_name], |name| {
        target
            .attr(name)
            .map(|body| body.to_string_lossy().into_owned())
    });
    let Some(body) = body.filter(|body| !body.is_empty()) else {
        return false;
    };
    if !js.inline_handlers_allowed() {
        js.log(&format!("CSP blocked: inline event handler {attr_name}"));
        return false;
    }
    js.scope(|scope| {
        let function = match compile_inline(scope, &body) {
            Ok(function) => function,
            Err(exception) => {
                if let Some(message) = message_of(scope, &exception) {
                    js.log(&format!("JS error compiling {attr_name}: {message}"));
                }
                return false;
            }
        };
        let holder = is_window_handler_holder(target);
        let this = if holder && is_window_reflected(kind) {
            scope.global()
        } else {
            ffi::wrap(scope, target)
        };
        let previous = get(scope, event, "currentTarget");
        set(scope, event, "currentTarget", this.clone());
        let (ret, special) = call_on_handler(scope, &function, &this, kind, event, holder);
        set(scope, event, "currentTarget", previous);
        match ret {
            Err(exception) => {
                if let Some(message) = message_of(scope, &exception) {
                    js.log(&format!("JS error in {attr_name}: {message}"));
                }
            }
            Ok(value) => {
                if !special && returned_false(scope, &value) {
                    ffi::mark_default_prevented(scope, event);
                }
            }
        }
        true
    })
    .unwrap_or(false)
}

pub(crate) fn fire_property(js: Js, target: Element, kind: &str, event: &Value) -> bool {
    if !target.is_element() || !target.has_js_wrapper() {
        return false;
    }
    let lowered;
    let base = if kind.starts_with("webkit") {
        lowered = kind.to_ascii_lowercase();
        lowered.as_str()
    } else {
        kind
    };
    js.scope(|scope| {
        let wrapper = ffi::wrap(scope, target);
        let handler = with_key(&["on", base], |name| get(scope, &wrapper, name));
        let source = with_key(&[SOURCE_PREFIX, "on", base], |key| {
            get(scope, &wrapper, key)
        });
        if source.is_string() || !scope.is_function(&handler) {
            return false;
        }
        let previous = get(scope, event, "currentTarget");
        set(scope, event, "currentTarget", wrapper.clone());
        let holder = is_window_handler_holder(target);
        let (ret, special) = call_on_handler(scope, &handler, &wrapper, kind, event, holder);
        set(scope, event, "currentTarget", previous);
        match ret {
            Err(exception) => {
                if let Some(message) = message_of(scope, &exception) {
                    js.log(&format!("JS error in on{kind}: {message}"));
                }
            }
            Ok(value) => {
                if !special && returned_false(scope, &value) {
                    ffi::mark_default_prevented(scope, event);
                }
            }
        }
        true
    })
    .unwrap_or(false)
}

pub(crate) fn fire_window_level(js: Js, kind: &str, event: &Value, at_target: bool) -> bool {
    if is_window_reflected(kind) && !at_target {
        return false;
    }
    js.scope(|scope| {
        let global = scope.global();
        let document = get(scope, &global, "document");
        let handler = with_key(&["on", kind], |name| get(scope, &document, name));
        if !scope.is_function(&handler) {
            return false;
        }
        match scope.call(&handler, &document, core::slice::from_ref(event)) {
            Err(exception) => {
                if let Some(message) = message_of(scope, &exception) {
                    js.log(&format!("JS error in on{kind}: {message}"));
                }
            }
            Ok(value) => {
                if returned_false(scope, &value) {
                    ffi::mark_default_prevented(scope, event);
                }
            }
        }
        true
    })
    .unwrap_or(false)
}

pub(crate) fn fire_window_property(
    js: Js,
    target: Option<Element>,
    kind: &str,
    event: &Value,
) -> bool {
    let reflected = is_window_reflected(kind);
    let current_doc = js.current_document();
    if reflected && target != current_doc {
        return false;
    }
    let mut ctx = js.ctx();
    let mut frame_realm = core::ptr::null_mut();
    if let Some(doc) = nearest_document(target)
        && doc.parent().is_some()
    {
        frame_realm = js.realm_for_node(doc);
        if frame_realm.is_null() {
            return false;
        }
        ctx = frame_realm;
    }
    if ctx.is_null() {
        return false;
    }
    let mut fired = ffi::in_context(ctx, |scope| {
        let global = scope.global();
        let mut handler =
            ffi::suspend_named(|| with_key(&["on", kind], |name| get(scope, &global, name)));
        if !frame_realm.is_null()
            && scope.is_function(&handler)
            && ffi::function_realm(scope, &handler) != frame_realm
        {
            handler = Value::undefined();
        }
        if !scope.is_function(&handler) {
            return false;
        }
        let (ret, special) = js
            .scope(|outer| call_on_handler(outer, &handler, &global, kind, event, true))
            .unwrap_or((Ok(Value::undefined()), false));
        match ret {
            Err(exception) => {
                crate::errors::report_handler_exception(js, scope, Some(kind), &exception)
            }
            Ok(value) => {
                if !special && returned_false(scope, &value) {
                    ffi::mark_default_prevented(scope, event);
                }
            }
        }
        js.microtask_checkpoint();
        true
    });
    if reflected
        && let Some(body) = current_doc.and_then(|doc| ffi::first_element(doc, c"body"))
        && fire_inline(js, body, kind, event)
    {
        fired = true;
    }
    fired
}

pub(crate) fn has_own_unprefixed(js: Js, scope: &mut Scope<'_>, cur: Element, base: &str) -> bool {
    if cur.is_element() && cur.has_js_wrapper() {
        let wrapper = ffi::wrap(scope, cur);
        let handler = with_key(&["on", base], |name| get(scope, &wrapper, name));
        if scope.is_function(&handler) {
            return true;
        }
    }
    let key = cur.as_ptr() as usize;
    crate::peek(js, |page| {
        page.listeners.get(&key).is_some_and(|own| {
            own.iter()
                .any(|l| !l.is_dead() && !l.window_level && *l.kind == *base)
        })
    })
}

pub(crate) fn fires_at_document(
    js: Js,
    cur: Element,
    kind: &str,
    event: &Value,
    at_target: bool,
) -> bool {
    is_document(cur) && fire_window_level(js, kind, event, at_target)
}

pub(crate) fn install_props(scope: &mut Scope<'_>, target: &Value) {
    for name in HANDLER_NAMES {
        if !scope.has_property(target, name).unwrap_or(false) {
            set(scope, target, name, Value::null());
        }
    }
}

pub(crate) fn install_accessors(scope: &mut Scope<'_>, proto: &Value) {
    if !proto.is_object() {
        return;
    }
    for (index, name) in HANDLER_NAMES.iter().enumerate() {
        let (getter, setter) = ffi::handler_accessors(scope, name, index);
        let _ = scope.define_accessor(
            proto,
            name,
            Some(&getter),
            Some(&setter),
            Attributes::CONFIGURABLE,
        );
    }
}

pub(crate) fn on_get(scope: &mut Scope<'_>, this: &Value, index: usize) -> JsResult {
    let Some(name) = HANDLER_NAMES.get(index) else {
        return Ok(Value::null());
    };
    let body = ffi::unwrap(this).and_then(|el| {
        with_c_key(&[name], |attr| {
            el.attr(attr)
                .map(|body| body.to_string_lossy().into_owned())
        })
    });
    let compiled = with_key(&[COMPILED_PREFIX, name], |key| get(scope, this, key));
    if scope.is_function(&compiled) {
        let compiled_from = with_key(&[SOURCE_PREFIX, name], |key| get(scope, this, key));
        let stale = compiled_from.is_string()
            && match scope.to_string(&compiled_from) {
                Ok(was) => body.as_deref() != Some(was.as_str()),
                Err(_) => true,
            };
        if !stale {
            return Ok(compiled);
        }
    }
    if compiled.is_null() {
        return Ok(Value::null());
    }
    let Some(body) = body else {
        return Ok(Value::null());
    };
    let js = ffi::js_of(scope);
    if !js.is_null() && !js.inline_handlers_allowed() {
        return Ok(Value::null());
    }
    let Ok(function) = compile_inline(scope, &body) else {
        return Ok(Value::null());
    };
    with_key(&[COMPILED_PREFIX, name], |key| {
        set(scope, this, key, function.clone())
    });
    let source = scope.string(&body);
    with_key(&[SOURCE_PREFIX, name], |key| set(scope, this, key, source));
    Ok(function)
}

pub(crate) fn on_set(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    index: usize,
) -> JsResult {
    let Some(name) = HANDLER_NAMES.get(index) else {
        return Ok(Value::undefined());
    };
    let value = crate::arg(args, 0);
    let stored = if scope.is_function(&value) {
        value
    } else {
        Value::null()
    };
    with_key(&[COMPILED_PREFIX, name], |key| {
        set(scope, this, key, stored)
    });
    with_key(&[SOURCE_PREFIX, name], |key| {
        set(scope, this, key, Value::undefined())
    });
    Ok(Value::undefined())
}
