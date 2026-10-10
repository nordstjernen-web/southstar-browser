//! Southstar — click(), synthetic click activation and keyboard activation of links, buttons, checkables and summaries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::controls::effectively_disabled;
use southstar_dom::tree::effectively_inert;
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult, has_attr, is_named};

const FLAG_CLICK_IN_PROGRESS: u32 = 1 << 17;

fn with_synthetic<R>(js: Js, f: impl FnOnce() -> R) -> R {
    crate::with(js, |page| page.synthetic_clicks += 1);
    let result = f();
    crate::with(js, |page| page.synthetic_clicks -= 1);
    result
}

pub(crate) fn click_method(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() || el.flags() & FLAG_CLICK_IN_PROGRESS != 0 {
        return Ok(Value::undefined());
    }
    el.add_flags(FLAG_CLICK_IN_PROGRESS);
    with_synthetic(js, || click_with_activation(js, el));
    el.remove_flags(FLAG_CLICK_IN_PROGRESS);
    Ok(Value::undefined())
}

pub(crate) fn click_with_activation(js: Js, el: Element) {
    js.scope(|scope| dispatch_click(scope, js, el));
}

pub(crate) fn synthetic_activation(js: Js, act: Element, target: Element) {
    with_synthetic(js, || {
        js.scope(|scope| ffi::activation_behavior(scope, act, target))
    });
}

fn dispatch_click(scope: &mut Scope<'_>, js: Js, el: Element) {
    if effectively_inert(el) || ffi::is_disabled_form_control(el) {
        return;
    }
    let act = ffi::click_activation_target(el).filter(|&act| act == el || !effectively_inert(act));
    let kind = ffi::checkable_kind(act);
    let state = act
        .filter(|_| kind != 0)
        .map(|act| js.checkable_pre_click(act, kind));
    let event = ffi::make_event(scope, c"click", el);
    let _ = scope.set(&event, "composed", Value::boolean(true));
    if crate::peek(js, |page| page.synthetic_clicks) > 0 {
        let _ = scope.set(&event, "_is_trusted", Value::boolean(false));
    }
    let prevented = js.dispatch_built(el, c"click", event);
    if let (Some(act), Some(state)) = (act, state) {
        js.checkable_post_click(act, kind, &state, prevented);
        return;
    }
    if let Some(act) = act.filter(|_| !prevented) {
        ffi::activation_behavior(scope, act, el);
    }
}

fn is_link(el: Element) -> bool {
    (is_named(Some(el), b"a") || is_named(Some(el), b"area")) && has_attr(el, c"href")
}

pub(crate) fn keyboard_activate(js: Js, el: Element, key: &[u8], keyup: bool) -> bool {
    if !el.is_element() {
        return false;
    }
    let enter = key == b"Enter";
    let space = key == b" ";
    if (!enter || keyup) && (!space || !keyup) {
        return false;
    }
    if effectively_inert(el) || effectively_disabled(el) {
        return false;
    }
    let summary = ffi::summary_toggle_target(el).is_some();
    let button = ffi::is_button(el);
    let activates = if enter {
        is_link(el) || button || summary
    } else {
        button || ffi::checkable_kind(Some(el)) != 0 || summary
    };
    if !activates {
        return false;
    }
    click_with_activation(js, el);
    true
}

pub(crate) fn keyboard_activates(el: Element, key: &[u8]) -> bool {
    key == b" "
        && (ffi::is_button(el)
            || ffi::checkable_kind(Some(el)) != 0
            || ffi::summary_toggle_target(el).is_some())
}
