//! Southstar — the Rust API other crates call to dispatch events on a page, query its listeners and report its errors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};

pub use crate::ui_events::{KeyPress, Modifiers, Toggle};

use crate::Element;
use crate::ffi::{Js, NsJs, js_of};

fn live(js: *const NsJs) -> Option<Js> {
    let js = Js::of(js);
    (!js.is_null()).then_some(js)
}

fn element(node: Node<'_>) -> Option<Element> {
    node.downgrade().get()
}

fn on_page<R: Default>(js: *const NsJs, target: Node<'_>, f: impl FnOnce(Js, Element) -> R) -> R {
    match (live(js), element(target)) {
        (Some(js), Some(target)) => f(js, target),
        _ => R::default(),
    }
}

pub fn dispatch_event(js: *const NsJs, target: Node<'_>, kind: &str) -> (bool, bool) {
    on_page(js, target, |js, target| {
        crate::dispatch::dispatch_event(js, target, kind)
    })
}

pub fn dispatch_built_event(
    js: *const NsJs,
    target: Node<'_>,
    kind: &str,
    event: Value,
) -> (bool, bool) {
    match element(target) {
        Some(target) => crate::dispatch::dispatch_built(Js::of(js), target, kind, event),
        None => (false, false),
    }
}

pub fn dispatch_window_only_event(
    js: *const NsJs,
    document: Option<Node<'_>>,
    kind: &str,
    event: Value,
) -> bool {
    crate::dispatch::dispatch_window_only(Js::of(js), document.and_then(element), kind, event)
}

pub fn dispatch_key_event(
    js: *const NsJs,
    target: Node<'_>,
    kind: &str,
    press: &KeyPress<'_>,
) -> (bool, bool) {
    on_page(js, target, |js, target| {
        crate::ui_events::key_event(js, target, kind, press)
    })
}

pub fn dispatch_input_event(
    js: *const NsJs,
    target: Node<'_>,
    kind: &str,
    input_type: Option<&str>,
    data: Option<&str>,
) -> (bool, bool) {
    on_page(js, target, |js, target| {
        crate::ui_events::input_event(js, target, kind, input_type, data)
    })
}

pub fn dispatch_toggle_event(
    js: *const NsJs,
    target: Node<'_>,
    kind: &str,
    toggle: &Toggle<'_>,
) -> (bool, bool) {
    on_page(js, target, |js, target| {
        crate::ui_events::toggle_event(js, target, kind, toggle)
    })
}

pub fn path_has_active_listener(js: *const NsJs, target: Node<'_>, kind: &str) -> bool {
    on_page(js, target, |js, target| {
        crate::dispatch::path_has_active_listener(js, target, kind)
    })
}

pub fn node_has_click_handler(js: *const NsJs, target: Node<'_>) -> bool {
    on_page(js, target, |js, target| {
        ["click", "pointerdown", "mousedown", "touchstart"]
            .iter()
            .any(|kind| crate::dispatch::path_has_active_listener(js, target, kind))
    })
}

pub fn current_dispatch_path(scope: &Scope<'_>) -> Option<(Vec<Element>, bool)> {
    let js = js_of(scope);
    if js.is_null() {
        return None;
    }
    crate::peek(js, |page| {
        page.paths
            .last()
            .map(|path| (path.nodes.to_vec(), path.window))
    })
}

pub fn forward_body_content_handler(
    scope: &mut Scope<'_>,
    element_node: Node<'_>,
    name: &str,
    code: Option<&str>,
) {
    if let Some(element_node) = element(element_node) {
        crate::body_handlers::forward_content_handler(scope, element_node, name, code);
    }
}

pub fn report_error_event_in(
    js: *const NsJs,
    ctx: ContextHandle,
    message: Option<&str>,
    filename: Option<&str>,
    lineno: i32,
    colno: i32,
) {
    let Some(js) = live(js) else {
        return;
    };
    js.in_realm_scope(ctx, || {
        crate::errors::report_error_event(js, message, filename, lineno, colno, &Value::null());
    });
}

pub fn drop_pending_rejections(js: *const NsJs) {
    if let Some(js) = live(js) {
        crate::errors::drop_rejections(js);
    }
}

pub fn reset(js: *const NsJs) {
    let Some(js) = live(js) else {
        return;
    };
    let (listeners, pending, reported) = crate::with(js, |page| {
        (
            std::mem::take(&mut page.listeners),
            std::mem::take(&mut page.pending_rejections),
            std::mem::take(&mut page.reported_rejections),
        )
    });
    for listener in listeners.values().flatten() {
        listener.kill();
    }
    drop(listeners);
    drop(pending);
    drop(reported);
}

pub fn listener_count(js: *const NsJs) -> usize {
    live(js).map_or(0, |js| {
        crate::peek(js, |page| page.listeners.values().map(Vec::len).sum())
    })
}
