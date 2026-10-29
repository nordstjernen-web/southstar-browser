//! Southstar — activation behaviour: what a click does to a link, label, summary, button, submit or reset control, and which elements have one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::ancestors;
use southstar_js_engine::Scope;

use crate::Element;
use crate::ffi::{self, Js};

const JAVASCRIPT_SCHEME: &[u8] = b"javascript:";

fn named(el: Element, tag: &str) -> bool {
    el.element_name() == Some(tag.as_bytes())
}

fn named_any(el: Element, tags: &[&str]) -> bool {
    tags.iter().any(|tag| named(el, tag))
}

fn has_attr(el: Element, name: &core::ffi::CStr) -> bool {
    el.attr(name).is_some()
}

fn input_type_is_hidden(el: Element) -> bool {
    el.attr(c"type")
        .is_some_and(|t| t.to_bytes().eq_ignore_ascii_case(b"hidden"))
}

fn is_interactive_content(el: Element) -> bool {
    if !el.is_element() || el.name().is_none() {
        return false;
    }
    if named_any(el, &["a", "area"]) {
        return has_attr(el, c"href");
    }
    if named(el, "input") {
        return !input_type_is_hidden(el);
    }
    if named_any(el, &["audio", "video"]) {
        return has_attr(el, c"controls");
    }
    if named_any(el, &["img", "object"]) {
        return has_attr(el, c"usemap");
    }
    named_any(
        el,
        &[
            "button", "details", "embed", "iframe", "label", "select", "textarea",
        ],
    )
}

pub(crate) fn has_activation_behavior(el: Element) -> bool {
    if !el.is_element() || el.name().is_none() {
        return false;
    }
    if named_any(el, &["a", "area"]) {
        return has_attr(el, c"href");
    }
    if named(el, "input") {
        return !input_type_is_hidden(el);
    }
    if named_any(el, &["button", "label"]) {
        return true;
    }
    named(el, "summary") && ffi::summary_toggle_target(el).is_some()
}

pub(crate) fn click_activation_target(el: Element) -> Option<Element> {
    core::iter::once(el)
        .chain(ancestors(el))
        .find(|&cur| has_activation_behavior(cur))
}

fn activate_label(js: Js, label: Element, target: Element) {
    let mut cursor = Some(target);
    while let Some(cur) = cursor.filter(|&c| c != label) {
        if is_interactive_content(cur) {
            return;
        }
        cursor = cur.parent();
    }
    let by_for = label
        .attr(c"for")
        .filter(|id| !id.is_empty())
        .zip(js.current_document())
        .and_then(|(id, doc)| southstar_dom::index::find_by_id(doc, id))
        .filter(|&t| ffi::is_labelable(t));
    let Some(control) = by_for.or_else(|| ffi::first_labelable_descendant(label)) else {
        return;
    };
    if control == target || ffi::is_disabled_form_control(control) {
        return;
    }
    if js.has_context() {
        crate::click::click_with_activation(js, control);
    }
}

fn follow_in_iframe(scope: &mut Scope<'_>, el: Element, href: &[u8]) -> bool {
    let Some(iframe) = ancestors(el).find(|&p| named(p, "iframe")) else {
        return false;
    };
    if !iframe.has_js_wrapper() || href.is_empty() {
        return false;
    }
    let window = ffi::frame_realm_window(scope, iframe);
    if !window.is_object() {
        return false;
    }
    let Ok(location) = scope.get(&window, "location") else {
        return false;
    };
    if !location.is_object() {
        return false;
    }
    let href = scope.string_from_bytes(href);
    let _ = scope.set(&location, "href", href);
    true
}

fn run_javascript_url(js: Js, act: Element, code: &[u8]) {
    match ancestors(act).find(|&p| named(p, "iframe")) {
        None => {
            if js.inline_scripts_allowed() {
                js.eval_source(code, c"javascript-url");
            }
        }
        Some(iframe) => {
            if !ffi::frame_sandbox_blocks_scripts(act) {
                js.eval_in_frame(iframe, code, c"javascript-url");
            }
        }
    }
}

fn activate_link(scope: &mut Scope<'_>, js: Js, act: Element) {
    let href = act.attr(c"href").map(|h| h.to_bytes().to_vec());
    if let Some(code) = href
        .as_deref()
        .and_then(|h| h.strip_prefix(JAVASCRIPT_SCHEME))
    {
        let code = ffi::uri_unescape(code).unwrap_or_else(|| code.to_vec());
        run_javascript_url(js, act, &code);
        return;
    }
    let href = href.unwrap_or_default();
    if follow_in_iframe(scope, act, &href) || href.is_empty() {
        return;
    }
    let resolved = ffi::anchor_resolved_href(js, act);
    if let Some(filename) = act.attr(c"download").map(CString::from) {
        let url = resolved.as_deref().unwrap_or(&href);
        if js.request_download(url, &filename) {
            return;
        }
    }
    if resolved
        .as_deref()
        .is_some_and(|url| js.anchor_fragment_navigate(url))
    {
        return;
    }
    js.navigate(&href);
}

pub(crate) fn activation_behavior(scope: &mut Scope<'_>, act: Element, target: Element) {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return;
    }
    if named(act, "summary") {
        js.activate_summary(act);
        return;
    }
    if named(act, "label") {
        activate_label(js, act, target);
        return;
    }
    if named_any(act, &["a", "area"]) {
        activate_link(scope, js, act);
        return;
    }
    if ffi::is_disabled_form_control(act) {
        return;
    }
    if named(act, "button") {
        js.button_activation(act, target);
        return;
    }
    if ffi::is_submit_trigger(act) {
        if let Some(form) = js.form_owner_for(act) {
            ffi::request_submit_form(scope, form, act);
        }
    } else if ffi::is_reset_trigger(act)
        && let Some(form) = js.form_owner_for(act)
    {
        ffi::reset_form(scope, form);
    }
    js.popover_target_activation(act, target);
}
