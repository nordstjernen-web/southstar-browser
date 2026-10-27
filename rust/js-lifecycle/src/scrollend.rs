//! Southstar — the scrollend events queued by scrolling and fired at the next animation frame.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, WeakNode};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Page, page, page_or_new};

#[derive(Default)]
pub(crate) struct Scrollend {
    targets: Vec<WeakNode>,
    document: bool,
}

pub(crate) fn clear(page: &Page) {
    let mut scrollend = page.scrollend.borrow_mut();
    scrollend.targets = Vec::new();
    scrollend.document = false;
}

pub(crate) fn queue(js: Js, el: Node<'_>) {
    if js.is_null() {
        return;
    }
    let el = el.downgrade();
    let page = page_or_new(js);
    let mut scrollend = page.scrollend.borrow_mut();
    if !scrollend.targets.contains(&el) {
        scrollend.targets.push(el);
    }
}

pub(crate) fn queue_document(js: Js) {
    if !js.is_null() {
        page_or_new(js).scrollend.borrow_mut().document = true;
    }
}

fn not_cancelable(scope: &mut Scope<'_>, event: &Value) {
    let _ = scope.set(event, "cancelable", Value::boolean(false));
}

pub(crate) fn flush(js: Js) {
    let ctx = js.ctx();
    if ctx.is_null() {
        return;
    }
    let Some(page) = page(js) else {
        return;
    };
    let (targets, document) = {
        let mut scrollend = page.scrollend.borrow_mut();
        if !scrollend.document && scrollend.targets.is_empty() {
            return;
        }
        (
            core::mem::take(&mut scrollend.targets),
            core::mem::take(&mut scrollend.document),
        )
    };
    for el in targets {
        let el = el.get();
        if !js.node_in_page(el) {
            continue;
        }
        let Some(event) = ctx.with(|scope| {
            let event = ffi::make_event(scope, c"scrollend", el);
            let _ = scope.set(&event, "bubbles", Value::boolean(false));
            not_cancelable(scope, &event);
            event
        }) else {
            continue;
        };
        js.dispatch_built(el, c"scrollend", event);
    }
    if !document {
        return;
    }
    let Some(event) = ctx.with(|scope| {
        let global = scope.global();
        let event = ffi::make_event(scope, c"scrollend", None);
        not_cancelable(scope, &event);
        let _ = scope.set(&event, "target", global);
        event
    }) else {
        return;
    };
    js.dispatch_window_only(js.current_document(), c"scrollend", event);
    let doc = js.current_document();
    if doc.is_some() {
        let Some(event) = ctx.with(|scope| {
            let event = ffi::make_event(scope, c"scrollend", doc);
            not_cancelable(scope, &event);
            event
        }) else {
            return;
        };
        js.dispatch_built(doc, c"scrollend", event);
    }
}
