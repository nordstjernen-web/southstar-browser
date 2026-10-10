//! Southstar — form submission and reset: requestSubmit(), submit(), reset(), submit and reset buttons, method=dialog and the SubmitEvent constructor.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{ancestors_and_self, children, controls};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Page};
use crate::validity::first_invalid;
use crate::{
    Element, JsResult, MAX_DEPTH, form_owner, name_is, named, non_empty, text_is, text_is_any,
    type_of,
};

const SANDBOX_BLOCKED: &core::ffi::CStr =
    c"Blocked form submission: sandboxed iframe without allow-forms";

pub(crate) fn is_submit_trigger(node: Element) -> bool {
    if node.element_name().is_none() {
        return false;
    }
    let ty = type_of(node);
    if name_is(node, "button") {
        if text_is(ty, "submit") {
            return true;
        }
        if text_is_any(ty, &["reset", "button"]) {
            return false;
        }
        return node.attr(c"command").is_none()
            && node.attr(c"commandfor").is_none()
            && !node.parent().is_some_and(|p| named(p, "select"));
    }
    name_is(node, "input") && text_is_any(ty, &["submit", "image"])
}

pub(crate) fn is_reset_trigger(node: Element) -> bool {
    node.element_name().is_some()
        && text_is(type_of(node), "reset")
        && (name_is(node, "button") || name_is(node, "input"))
}

fn close_dialog_form(page: Option<Page>, form: Element, submitter: Option<Element>) -> bool {
    let mut method = form.attr(c"method");
    if let Some(form_method) = submitter.and_then(|s| non_empty(s.attr(c"formmethod"))) {
        method = Some(form_method);
    }
    if !text_is(method, "dialog") {
        return false;
    }
    let Some(dialog) = ancestors_and_self(form).find(|n| named(*n, "dialog")) else {
        return false;
    };
    let return_value = submitter
        .filter(|s| is_submit_trigger(*s))
        .and_then(|s| s.attr(c"value"));
    Page::close_dialog(page, dialog, return_value);
    true
}

fn skips_validation(form: Element, submitter: Option<Element>) -> bool {
    form.attr(c"novalidate").is_some()
        || submitter.is_some_and(|s| s.attr(c"formnovalidate").is_some())
}

fn validation_allows_submit(page: Page, form: Element, submitter: Option<Element>) -> bool {
    if skips_validation(form, submitter) {
        return true;
    }
    let Some(bad) = first_invalid(form, form.root(), 0) else {
        return true;
    };
    page.dispatch_event(bad, c"invalid");
    false
}

fn submit_or_block(page: Page, form: Element, submitter: Option<Element>) {
    if ffi::sandbox_blocks_forms(form) {
        page.log_line(SANDBOX_BLOCKED);
    } else {
        page.submit_form(form, submitter);
    }
}

pub(crate) fn request_submit(
    scope: &mut Scope<'_>,
    form: Option<Element>,
    submitter: Option<Element>,
) {
    let (Some(form), Some(page)) = (form, Page::of(scope)) else {
        return;
    };
    if !page.node_in_page(form) || !validation_allows_submit(page, form, submitter) {
        return;
    }
    let (_, prevented) = page.dispatch_submit(form, submitter);
    if !prevented && !close_dialog_form(Some(page), form, submitter) {
        submit_or_block(page, form, submitter);
    }
}

fn reset_owned_outputs(page: Option<Page>, form: Element, scan: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if named(scan, "output")
        && form_owner(scan) == Some(form)
        && scan.attr(c"data-nd-output-dirty").is_some()
    {
        let default = scan
            .attr(c"data-nd-output-default")
            .map(|d| d.to_bytes().to_vec())
            .unwrap_or_default();
        Page::clear_children(page, scan);
        if !default.is_empty() {
            ffi::append_text(scan, &default);
        }
        ffi::remove_attr(scan, c"data-nd-output-dirty");
        ffi::remove_attr(scan, c"data-nd-output-default");
    }
    for child in children(scan) {
        reset_owned_outputs(page, form, child, depth + 1);
    }
}

pub(crate) fn reset_form(scope: &mut Scope<'_>, form: Option<Element>) {
    let Some(form) = form else { return };
    let page = Page::of(scope);
    if let Some(page) = page {
        if page.dispatch_event(form, c"reset") {
            return;
        }
    }
    let doc = page
        .and_then(Page::current_document)
        .unwrap_or_else(|| form.root());
    controls::reset_owned_controls(form, Some(doc));
    reset_owned_outputs(page, form, doc, 0);
    Page::mark_mutated(page);
}

pub(crate) fn request_submit_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let form = ffi::element(this);
    let (Some(form), Some(_)) = (form, Page::of(scope)) else {
        return Ok(Value::undefined());
    };
    if !named(form, "form") {
        return Ok(Value::undefined());
    }
    let mut submitter = None;
    if let Some(arg) = args.first().filter(|a| !a.is_undefined() && !a.is_null()) {
        let Some(el) = ffi::element(arg) else {
            return Err(scope.type_error("requestSubmit submitter must be an Element"));
        };
        if !is_submit_trigger(el) {
            return Err(scope.type_error("requestSubmit submitter must be a submit button"));
        }
        if form_owner(el) != Some(form) {
            return Err(ffi::not_found(scope, "submitter is not owned by this form"));
        }
        submitter = Some(el);
    }
    request_submit(scope, Some(form), submitter);
    Ok(Value::undefined())
}

pub(crate) fn submit_method(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(form), Some(page)) = (ffi::element(this), Page::of(scope)) else {
        return Ok(Value::undefined());
    };
    if !name_is(form, "form") {
        return Ok(Value::undefined());
    }
    let submitter = args.first().and_then(ffi::element);
    if !close_dialog_form(Some(page), form, submitter) {
        submit_or_block(page, form, submitter);
    }
    Ok(Value::undefined())
}

pub(crate) fn reset_method(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if let Some(form) = ffi::element(this).filter(|el| name_is(*el, "form")) {
        reset_form(scope, Some(form));
    }
    Ok(Value::undefined())
}

pub(crate) fn submit_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let event = ffi::event_ctor(scope, this, args)?;
    let mut submitter = Value::null();
    if let Some(init) = args.get(1).filter(|init| init.is_object()) {
        submitter = scope.get(init, "submitter")?;
        if submitter.is_undefined() {
            submitter = Value::null();
        }
    }
    scope.set(&event, "submitter", submitter)?;
    Ok(event)
}
