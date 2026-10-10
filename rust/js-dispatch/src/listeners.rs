//! Southstar — the listeners registered on nodes, documents and windows: addEventListener's options, duplicate and aborted-signal checks, passive defaults and removal.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::Cell;
use std::rc::Rc;

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, FLAG_HAS_LISTENERS, JsResult, arg, get, is_named, same_object};

pub(crate) struct Listener {
    pub target: usize,
    pub kind: Box<str>,
    pub callback: Value,
    pub signal: Option<Value>,
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
    pub window_level: bool,
    dead: Cell<bool>,
}

impl Listener {
    pub fn is_dead(&self) -> bool {
        self.dead.get()
    }

    pub fn kill(&self) {
        self.dead.set(true);
    }

    pub fn signal_aborted(&self, scope: &mut Scope<'_>) -> bool {
        self.signal
            .as_ref()
            .is_some_and(|signal| signal.is_object() && crate::truthy(scope, signal, "aborted"))
    }
}

#[derive(Default)]
pub(crate) struct Options {
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
    pub passive_set: bool,
    pub signal: Option<Value>,
}

fn bool_option(scope: &mut Scope<'_>, opts: &Value, key: &str) -> Option<bool> {
    match scope.get(opts, key) {
        Ok(value) if value.is_undefined() => None,
        Ok(value) => Some(scope.to_bool(&value)),
        Err(_) => Some(false),
    }
}

pub(crate) fn parse_options(
    scope: &mut Scope<'_>,
    opts: &Value,
    strict_signal: bool,
) -> JsResult<Options> {
    let mut parsed = Options::default();
    if !opts.is_object() {
        parsed.capture = scope.to_bool(opts);
        return Ok(parsed);
    }
    if let Some(capture) = bool_option(scope, opts, "capture") {
        parsed.capture = capture;
    }
    if let Some(once) = bool_option(scope, opts, "once") {
        parsed.once = once;
    }
    if let Some(passive) = bool_option(scope, opts, "passive") {
        parsed.passive = passive;
        parsed.passive_set = true;
    }
    let signal = get(scope, opts, "signal");
    if signal.is_object() {
        parsed.signal = Some(signal);
    } else if strict_signal && !signal.is_undefined() {
        return Err(scope.type_error("signal must be an AbortSignal"));
    }
    Ok(parsed)
}

pub(crate) fn signal_is_aborted(scope: &mut Scope<'_>, signal: Option<&Value>) -> bool {
    signal.is_some_and(|signal| signal.is_object() && crate::truthy(scope, signal, "aborted"))
}

pub(crate) fn is_passive_default(kind: &str) -> bool {
    matches!(kind, "touchstart" | "touchmove" | "wheel" | "mousewheel")
}

pub(crate) fn own_listener(js: Js, target: usize, index: usize) -> Option<Rc<Listener>> {
    crate::with(js, |page| page.listeners.get(&target)?.get(index).cloned())
}

pub(crate) fn retire(js: Js, listener: &Rc<Listener>) {
    listener.kill();
    crate::with(js, |page| {
        if let Some(own) = page.listeners.get_mut(&listener.target) {
            own.retain(|l| !Rc::ptr_eq(l, listener));
            if own.is_empty() {
                page.listeners.remove(&listener.target);
            }
        }
    });
}

fn drop_dead(js: Js, target: usize) {
    crate::with(js, |page| {
        if let Some(own) = page.listeners.get_mut(&target) {
            own.retain(|listener| !listener.is_dead());
            if own.is_empty() {
                page.listeners.remove(&target);
            }
        }
    });
}

struct Registration<'a> {
    target: Element,
    kind: &'a str,
    callback: &'a Value,
    options: Options,
    window_level: bool,
}

fn is_duplicate(scope: &mut Scope<'_>, js: Js, reg: &Registration<'_>) -> bool {
    let target = reg.target.as_ptr() as usize;
    let mut found = false;
    let mut index = 0;
    while let Some(existing) = own_listener(js, target, index) {
        index += 1;
        if existing.is_dead() {
            continue;
        }
        let aborted = existing.signal_aborted(scope);
        if existing.is_dead() {
            continue;
        }
        if aborted {
            existing.kill();
            continue;
        }
        if existing.target == target
            && *existing.kind == *reg.kind
            && existing.window_level == reg.window_level
            && existing.capture == reg.options.capture
            && same_object(&existing.callback, reg.callback)
        {
            found = true;
            break;
        }
    }
    drop_dead(js, target);
    found
}

fn register(js: Js, reg: Registration<'_>, passive: bool) {
    let target = reg.target.as_ptr() as usize;
    let listener = Rc::new(Listener {
        target,
        kind: reg.kind.into(),
        callback: reg.callback.clone(),
        signal: reg.options.signal,
        capture: reg.options.capture,
        once: reg.options.once,
        passive,
        window_level: reg.window_level,
        dead: Cell::new(false),
    });
    crate::with(js, |page| {
        page.listeners.entry(target).or_default().push(listener)
    });
}

struct Request {
    kind: String,
    callback: Value,
    options: Options,
}

fn read_request(
    scope: &mut Scope<'_>,
    args: &[Value],
    strict_signal: bool,
) -> JsResult<Option<Request>> {
    if args.len() < 2 {
        return Ok(None);
    }
    let kind = scope.to_string(&args[0])?;
    let options = if args.len() >= 3 {
        parse_options(scope, &args[2], strict_signal)?
    } else {
        Options::default()
    };
    Ok(Some(Request {
        kind,
        callback: arg(args, 1),
        options,
    }))
}

fn accepts(scope: &mut Scope<'_>, request: &Request) -> bool {
    request.callback.is_object() && !signal_is_aborted(scope, request.options.signal.as_ref())
}

pub(crate) fn element_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let Some(request) = read_request(scope, args, true)? else {
        return Ok(Value::undefined());
    };
    if !accepts(scope, &request) {
        return Ok(Value::undefined());
    }
    let passive_set = request.options.passive_set;
    let mut passive = request.options.passive;
    let reg = Registration {
        target: el,
        kind: &request.kind,
        callback: &request.callback,
        options: request.options,
        window_level: false,
    };
    if is_duplicate(scope, js, &reg) {
        return Ok(Value::undefined());
    }
    if !passive_set
        && is_passive_default(reg.kind)
        && let Some(doc) = js.current_document()
    {
        let html = if is_named(Some(doc), b"html") {
            Some(doc)
        } else {
            ffi::first_element(doc, c"html")
        };
        let body = ffi::first_element(doc, c"body");
        if Some(el) == html || Some(el) == body {
            passive = true;
        }
    }
    register(js, reg, passive);
    el.add_flags(FLAG_HAS_LISTENERS);
    ffi::arm_invalidate(el);
    Ok(Value::undefined())
}

fn remove_listener(
    scope: &mut Scope<'_>,
    target: Element,
    args: &[Value],
    window_level: bool,
) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let Some(request) = read_request(scope, args, false)? else {
        return Ok(Value::undefined());
    };
    let key = target.as_ptr() as usize;
    crate::with(js, |page| {
        let Some(own) = page.listeners.get_mut(&key) else {
            return;
        };
        if let Some(index) = own.iter().position(|l| {
            !l.is_dead()
                && *l.kind == *request.kind
                && l.window_level == window_level
                && l.capture == request.options.capture
                && same_object(&l.callback, &request.callback)
        }) {
            own.remove(index).kill();
        }
        if own.is_empty() {
            page.listeners.remove(&key);
        }
    });
    Ok(Value::undefined())
}

pub(crate) fn element_remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    match ffi::unwrap(this) {
        Some(el) => remove_listener(scope, el, args, false),
        None => Ok(Value::undefined()),
    }
}

fn document_level_add(
    scope: &mut Scope<'_>,
    target: Option<Element>,
    args: &[Value],
    window_level: bool,
) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(target) = target else {
        return Ok(Value::undefined());
    };
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let Some(request) = read_request(scope, args, true)? else {
        return Ok(Value::undefined());
    };
    if !accepts(scope, &request) {
        return Ok(Value::undefined());
    }
    let passive = request.options.passive
        || (!request.options.passive_set && is_passive_default(&request.kind));
    let reg = Registration {
        target,
        kind: &request.kind,
        callback: &request.callback,
        options: request.options,
        window_level,
    };
    if is_duplicate(scope, js, &reg) {
        return Ok(Value::undefined());
    }
    register(js, reg, passive);
    ffi::arm_invalidate(target);
    Ok(Value::undefined())
}

pub(crate) fn document_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let target = ffi::document_root_for(scope, this);
    document_level_add(scope, target, args, false)
}

pub(crate) fn window_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let target = ffi::window_document_for(scope, this);
    document_level_add(scope, target, args, true)
}

pub(crate) fn document_remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    match ffi::document_root_for(scope, this) {
        Some(target) => remove_listener(scope, target, args, false),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn window_remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    match ffi::window_document_for(scope, this) {
        Some(target) => remove_listener(scope, target, args, true),
        None => Ok(Value::undefined()),
    }
}
