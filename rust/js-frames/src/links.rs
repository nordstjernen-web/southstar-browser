//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

type Native = Result<Value, Value>;
use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

pub struct JsDocumentLinks {
    pub realmdoc_deny_cookie: fn(&mut Scope<'_>, &Value),
}

static JS_DOCUMENT_LINKS: OnceLock<JsDocumentLinks> = OnceLock::new();

pub fn set_js_document_links(links: JsDocumentLinks) {
    let _ = JS_DOCUMENT_LINKS.set(links);
}

fn js_document_links() -> &'static JsDocumentLinks {
    JS_DOCUMENT_LINKS
        .get()
        .expect("southstar-js-document installs its links before the first page")
}

pub struct JsWindowLinks {
    pub js_new_window_named: fn(&mut Scope<'_>) -> Native,
    pub is_window_named: fn(&Value) -> bool,
    pub window_current_document_for: fn(&mut Scope<'_>, &Value) -> *mut NsNode,
    pub window_make_post_message: fn(&mut Scope<'_>, &Value) -> Value,
    pub window_bind_post_message: fn(&mut Scope<'_>, &Value),
}

static JS_WINDOW_LINKS: OnceLock<JsWindowLinks> = OnceLock::new();

pub fn set_js_window_links(links: JsWindowLinks) {
    let _ = JS_WINDOW_LINKS.set(links);
}

fn js_window_links() -> &'static JsWindowLinks {
    JS_WINDOW_LINKS
        .get()
        .expect("southstar-js-window installs its links before the first page")
}

pub(crate) fn nsr_realmdoc_deny_cookie(scope: &mut Scope<'_>, doc: &Value) {
    (js_document_links().realmdoc_deny_cookie)(scope, doc)
}

pub(crate) fn nsr_js_new_window_named(scope: &mut Scope<'_>) -> Native {
    (js_window_links().js_new_window_named)(scope)
}

pub(crate) fn nsr_is_window_named(value: &Value) -> bool {
    (js_window_links().is_window_named)(value)
}

pub(crate) fn nsr_window_current_document_for(
    scope: &mut Scope<'_>,
    window: &Value,
) -> *mut NsNode {
    (js_window_links().window_current_document_for)(scope, window)
}

pub(crate) fn nsr_window_make_post_message(scope: &mut Scope<'_>, window: &Value) -> Value {
    (js_window_links().window_make_post_message)(scope, window)
}

pub(crate) fn nsr_window_bind_post_message(scope: &mut Scope<'_>, global: &Value) {
    (js_window_links().window_bind_post_message)(scope, global)
}

pub struct JsDispatchLinks {
    pub element_dispatch_event: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub element_remove_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub element_add_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub window_dispatch_event: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub window_remove_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub window_add_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
}

static JS_DISPATCH_LINKS: OnceLock<JsDispatchLinks> = OnceLock::new();

pub fn set_js_dispatch_links(links: JsDispatchLinks) {
    let _ = JS_DISPATCH_LINKS.set(links);
}

fn js_dispatch_links() -> &'static JsDispatchLinks {
    JS_DISPATCH_LINKS
        .get()
        .expect("southstar-js-dispatch installs its links before the first page")
}

pub struct JsLifecycleLinks {
    pub window_cancel_animation_frame: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
    pub window_request_animation_frame: fn(&mut Scope<'_>, &Value, &[Value]) -> Native,
}

static JS_LIFECYCLE_LINKS: OnceLock<JsLifecycleLinks> = OnceLock::new();

pub fn set_js_lifecycle_links(links: JsLifecycleLinks) {
    let _ = JS_LIFECYCLE_LINKS.set(links);
}

fn js_lifecycle_links() -> &'static JsLifecycleLinks {
    JS_LIFECYCLE_LINKS
        .get()
        .expect("southstar-js-lifecycle installs its links before the first page")
}

#[allow(non_snake_case)]
pub(crate) fn nsr_element_dispatchEvent(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().element_dispatch_event)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_element_removeEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().element_remove_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_element_addEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().element_add_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_dispatchEvent(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().window_dispatch_event)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_removeEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().window_remove_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_addEventListener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_dispatch_links().window_add_event_listener)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_cancelAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_lifecycle_links().window_cancel_animation_frame)(scope, this, args)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_window_requestAnimationFrame(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Native {
    (js_lifecycle_links().window_request_animation_frame)(scope, this, args)
}
