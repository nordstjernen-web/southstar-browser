//! Southstar — the dialog element: show, showModal, close and requestClose, closedby, the close watchers, the modal top layer and the cancel and close events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::index;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::popover::{self, is_showing, queue_toggle_task};
use crate::{
    Element, JsResult, arg, flat_inclusive_ancestors, has_attr, inclusive_ancestor, info,
    info_peek, is_connected, is_named, peek, with,
};

const RETURN_VALUE_KEY: &str = "__nd_dialogReturnValue";
const MODAL_ATTR: &CStr = c"data-nd-modal";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClosedBy {
    Any,
    CloseRequest,
    None,
}

pub(crate) fn is_modal(js: Js, dialog: Element) -> bool {
    info_peek(js, dialog, |pi| pi.dialog_modal).unwrap_or(false)
}

fn set_modal(js: Js, dialog: Element, modal: bool) {
    info(js, dialog, |pi| pi.dialog_modal = modal);
    if has_attr(dialog, MODAL_ATTR) == modal {
        return;
    }
    if modal {
        ffi::set_attr(dialog, MODAL_ATTR, c"");
    } else {
        ffi::remove_attr(dialog, MODAL_ATTR);
    }
    ffi::mark_attr_dirty(dialog, MODAL_ATTR, (!modal).then_some(c""));
    js.mark_mutated();
}

pub(crate) fn refresh_top_layer(js: Js) {
    let doc = js.current_document().map(Element::root);
    let modal = peek(js, |page| {
        page.modal_dialogs
            .iter()
            .rev()
            .find(|d| has_attr(**d, c"open") && Some(d.root()) == doc)
            .copied()
    })
    .flatten();
    js.set_active_modal(modal);
}

pub(crate) fn close_watcher_add(js: Js, el: Element) {
    if !peek(js, |page| page.close_watchers.contains(&el)).unwrap_or(false) {
        let el = ffi::note(el);
        with(js, |page| page.close_watchers.push(el));
    }
}

pub(crate) fn close_watcher_remove(js: Js, el: Element) {
    if peek(js, |_| ()).is_some() {
        with(js, |page| crate::remove_from(&mut page.close_watchers, el));
    }
}

fn is_close_watcher(js: Js, el: Element) -> bool {
    peek(js, |page| page.close_watchers.contains(&el)).unwrap_or(false)
}

fn sync_open_dialogs(js: Js) {
    let Some(doc) = js.current_document().map(Element::root) else {
        return;
    };
    let all: Vec<Element> = index::tag_lookup(doc, c"dialog", |list| {
        (0..list.len()).map(|i| list.get(i)).collect()
    })
    .unwrap_or_default();
    let mut insert_at = 0;
    for d in all {
        if !has_attr(d, c"open") || d.root() != doc || is_close_watcher(js, d) {
            continue;
        }
        let d = ffi::note(d);
        with(js, |page| page.close_watchers.insert(insert_at, d));
        insert_at += 1;
    }
}

fn closed_by(js: Js, dialog: Element) -> ClosedBy {
    if let Some(v) = dialog.attr(c"closedby").map(CStr::to_bytes) {
        if v.eq_ignore_ascii_case(b"any") {
            return ClosedBy::Any;
        }
        if v.eq_ignore_ascii_case(b"closerequest") {
            return ClosedBy::CloseRequest;
        }
        if v.eq_ignore_ascii_case(b"none") {
            return ClosedBy::None;
        }
    }
    if is_modal(js, dialog) {
        ClosedBy::CloseRequest
    } else {
        ClosedBy::None
    }
}

pub(crate) fn get_closed_by(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(el) = ffi::unwrap(this) else {
        return Ok(scope.string("none"));
    };
    let name = match closed_by(ffi::js_of(scope), el) {
        ClosedBy::Any => "any",
        ClosedBy::CloseRequest => "closerequest",
        ClosedBy::None => "none",
    };
    Ok(scope.string(name))
}

pub(crate) fn set_closed_by(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    let value = scope.to_bytes(&arg(args, 0))?;
    ffi::js_of(scope).set_attr_recorded(el, c"closedby", &value);
    Ok(Value::undefined())
}

fn close_watcher_enabled(js: Js, dialog: Element) -> bool {
    info_peek(js, dialog, |pi| pi.enable_request_close).unwrap_or(false)
        || closed_by(js, dialog) != ClosedBy::None
}

fn queued_event_task(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
    data: &[Value],
) -> JsResult {
    let js = ffi::js_of(scope);
    let el = ffi::unwrap(&arg(data, 0));
    let kind = scope.to_string(&arg(data, 1)).ok();
    if let (Some(el), Some(kind)) = (el, kind)
        && !js.is_null()
        && !js.halted()
        && !js.in_pump()
    {
        let kind = CString::new(kind).unwrap_or_default();
        let event = ffi::make_event(scope, &kind, el);
        let _ = scope.set(&event, "bubbles", Value::boolean(false));
        let _ = scope.set(&event, "cancelable", Value::boolean(false));
        js.dispatch_built(el, &kind, event);
    }
    Ok(Value::undefined())
}

fn queue_event_task(js: Js, el: Element, kind: &str) {
    js.scope(|scope| {
        let wrapper = ffi::wrap(scope, el);
        let kind = scope.string(kind);
        let task = scope.bound_function("", 0, queued_event_task, &[wrapper, kind]);
        let _ = ffi::set_timeout(scope, task);
    });
}

fn fire_simple_event(js: Js, el: Element, kind: &CStr, cancelable: bool) -> bool {
    if js.halted() || js.in_pump() {
        return false;
    }
    let prevented = js
        .scope(|scope| {
            let event = ffi::make_event(scope, kind, el);
            let _ = scope.set(&event, "bubbles", Value::boolean(false));
            let _ = scope.set(&event, "cancelable", Value::boolean(cancelable));
            js.dispatch_built(el, kind, event)
        })
        .unwrap_or(false);
    cancelable && prevented
}

fn store_return_value(scope: &mut Scope<'_>, wrapper: &Value, rv: &[u8]) {
    let rv = scope.string_from_bytes(rv);
    let _ = scope.define(wrapper, RETURN_VALUE_KEY, rv, Attributes::METHOD);
}

pub(crate) fn get_return_value(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let v = scope.get(this, RETURN_VALUE_KEY)?;
    if v.is_string() {
        return Ok(v);
    }
    Ok(scope.string(""))
}

pub(crate) fn set_return_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    match scope.to_string(&arg(args, 0)) {
        Ok(s) => {
            store_return_value(scope, this, s.as_bytes());
            Ok(Value::undefined())
        }
        Err(error) => {
            store_return_value(scope, this, b"");
            Err(error)
        }
    }
}

fn close_steps(js: Js, dialog: Element, result: Option<&CStr>, source: Option<Element>) {
    if !has_attr(dialog, c"open") {
        return;
    }
    js.fire_toggle(dialog, c"beforetoggle", c"open", c"closed", false, source);
    if !has_attr(dialog, c"open") {
        return;
    }
    queue_toggle_task(js, dialog, true, true, false, source);
    js.remove_attr_recorded(dialog, c"open");
    let was_modal = is_modal(js, dialog);
    set_modal(js, dialog, false);
    with(js, |page| {
        crate::remove_from(&mut page.modal_dialogs, dialog)
    });
    refresh_top_layer(js);
    if let Some(result) = result {
        js.scope(|scope| {
            let wrapper = ffi::wrap(scope, dialog);
            store_return_value(scope, &wrapper, result.to_bytes());
        });
    }
    let prev = info(js, dialog, |pi| {
        pi.request_rv = None;
        pi.request_source = None;
        pi.dialog_prev_focus.take()
    });
    if let Some(prev) = prev
        && (was_modal || (js.focused().is_some() && inclusive_ancestor(dialog, js.focused())))
    {
        crate::focus::run_focusing_steps(js, prev);
    }
    if js.focused().is_some() && inclusive_ancestor(dialog, js.focused()) {
        js.set_focus(None);
    }
    queue_event_task(js, dialog, "close");
    js.mark_mutated();
}

pub(crate) fn close(js: Js, dialog: Element, result: Option<&CStr>, source: Option<Element>) {
    let _hold = js.hold(dialog);
    close_steps(js, dialog, result, source);
}

fn request_close_watcher(js: Js, dialog: Element, can_prevent: bool) -> bool {
    if !is_close_watcher(js, dialog) || !close_watcher_enabled(js, dialog) {
        return true;
    }
    if info(js, dialog, |pi| pi.running_cancel) {
        return true;
    }
    let _hold = js.hold(dialog);
    info(js, dialog, |pi| pi.running_cancel = true);
    let prevented = fire_simple_event(js, dialog, c"cancel", can_prevent);
    info(js, dialog, |pi| pi.running_cancel = false);
    if prevented {
        js.consume_user_activation();
    } else if is_close_watcher(js, dialog) && close_watcher_enabled(js, dialog) {
        close_watcher_remove(js, dialog);
        let (rv, source) = info(js, dialog, |pi| (pi.request_rv.clone(), pi.request_source));
        close_steps(js, dialog, rv.as_deref(), source);
    }
    !prevented
}

fn request_close(js: Js, dialog: Element, rv: Option<&CStr>, source: Option<Element>) {
    if !has_attr(dialog, c"open") || !is_connected(dialog) {
        return;
    }
    sync_open_dialogs(js);
    let source = source.map(ffi::note);
    info(js, dialog, |pi| {
        pi.enable_request_close = true;
        pi.request_rv = rv.map(CStr::to_owned);
        pi.request_source = source;
    });
    request_close_watcher(js, dialog, true);
    if crate::has_info(js, dialog) {
        info(js, dialog, |pi| pi.enable_request_close = false);
    }
}

fn invalid_state(js: Js, message: &str) -> Value {
    js.scope(|scope| ffi::dom_exception(scope, c"InvalidStateError", 11, message))
        .unwrap_or_else(Value::undefined)
}

pub(crate) fn show_modal(js: Js, dialog: Element, source: Option<Element>) -> JsResult<()> {
    let open = has_attr(dialog, c"open");
    if open && is_modal(js, dialog) {
        return Ok(());
    }
    if open {
        return Err(invalid_state(
            js,
            "The element already has an 'open' attribute, and therefore cannot be opened as a modal dialog.",
        ));
    }
    if !crate::in_active_document(js, dialog) {
        return Err(invalid_state(
            js,
            "The element is not connected to an active document.",
        ));
    }
    if is_showing(dialog) {
        return Err(invalid_state(
            js,
            "The dialog is already open as a popover.",
        ));
    }
    let prevented = js.fire_toggle(dialog, c"beforetoggle", c"closed", c"open", true, source);
    if prevented || has_attr(dialog, c"open") || !is_connected(dialog) || is_showing(dialog) {
        return Ok(());
    }
    queue_toggle_task(js, dialog, true, false, true, source);
    set_modal(js, dialog, true);
    js.set_attr_recorded(dialog, c"open", b"");
    let dialog_noted = ffi::note(dialog);
    with(js, |page| {
        crate::remove_from(&mut page.modal_dialogs, dialog);
        page.modal_dialogs.push(dialog_noted);
    });
    refresh_top_layer(js);
    let prev = js.focused().map(ffi::note);
    info(js, dialog, |pi| pi.dialog_prev_focus = prev);
    popover::hide_until(js, popover::topmost_ancestor(js, dialog, None), false, true);
    crate::focus::dialog_focusing_steps(js, dialog);
    js.mark_mutated();
    Ok(())
}

pub(crate) fn show(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    if has_attr(el, c"open") {
        if !is_modal(js, el) {
            return Ok(Value::undefined());
        }
        return Err(invalid_state(
            js,
            "The dialog is already open as a modal dialog.",
        ));
    }
    let prevented = js.fire_toggle(el, c"beforetoggle", c"closed", c"open", true, None);
    if prevented || has_attr(el, c"open") {
        return Ok(Value::undefined());
    }
    queue_toggle_task(js, el, true, false, true, None);
    js.set_attr_recorded(el, c"open", b"");
    let prev = js.focused().map(ffi::note);
    info(js, el, |pi| pi.dialog_prev_focus = prev);
    popover::hide_until(js, popover::topmost_ancestor(js, el, None), false, true);
    crate::focus::dialog_focusing_steps(js, el);
    js.mark_mutated();
    Ok(Value::undefined())
}

pub(crate) fn show_modal_method(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let _hold = this.clone();
    show_modal(js, el, None).map(|()| Value::undefined())
}

fn optional_string(scope: &mut Scope<'_>, args: &[Value]) -> Option<CString> {
    let value = args.first().filter(|v| !v.is_undefined())?;
    let s = scope.to_bytes(value).unwrap_or_default();
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    Some(CString::new(&s[..end]).unwrap_or_default())
}

pub(crate) fn close_method(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let rv = optional_string(scope, args);
    close(js, el, rv.as_deref(), None);
    Ok(Value::undefined())
}

pub(crate) fn request_close_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let rv = optional_string(scope, args);
    let _hold = this.clone();
    request_close(js, el, rv.as_deref(), None);
    Ok(Value::undefined())
}

pub(crate) fn command_steps(js: Js, dialog: Element, source: Element, close_kind: Command) {
    if is_showing(dialog) {
        return;
    }
    let open = has_attr(dialog, c"open");
    let value = source.attr(c"value").map(CStr::to_owned);
    match close_kind {
        Command::Close if open => close(js, dialog, value.as_deref(), Some(source)),
        Command::RequestClose if open => request_close(js, dialog, value.as_deref(), Some(source)),
        Command::ShowModal if !open => {
            let _ = show_modal(js, dialog, Some(source));
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Close,
    RequestClose,
    ShowModal,
}

fn nearest_clicked(target: Element) -> Option<Element> {
    flat_inclusive_ancestors(target)
        .find(|c| is_named(Some(*c), b"dialog") && has_attr(*c, c"open"))
}

fn topmost_open(js: Js) -> Option<Element> {
    peek(js, |page| {
        page.close_watchers
            .iter()
            .rev()
            .find(|d| is_named(Some(**d), b"dialog"))
            .copied()
    })
    .flatten()
}

pub(crate) fn light_dismiss(js: Js, target: Element, up: bool) {
    if topmost_open(js).is_none() {
        return;
    }
    let ancestor = nearest_clicked(target);
    if !up {
        let ancestor = ancestor.map(ffi::note);
        with(js, |page| page.dialog_pointerdown = ancestor);
        return;
    }
    let same = with(js, |page| page.dialog_pointerdown.take() == ancestor);
    if !same {
        return;
    }
    let Some(topmost) = topmost_open(js) else {
        return;
    };
    if ancestor == Some(topmost) || closed_by(js, topmost) != ClosedBy::Any {
        return;
    }
    request_close_watcher(js, topmost, false);
}

pub(crate) fn removing_steps(js: Js, dialog: Element) {
    let had_info = crate::has_info(js, dialog);
    if has_attr(dialog, c"open") {
        close_watcher_remove(js, dialog);
    }
    if with(js, |page| {
        crate::remove_from(&mut page.modal_dialogs, dialog)
    }) {
        refresh_top_layer(js);
    }
    if had_info {
        set_modal(js, dialog, false);
    }
}

pub(crate) fn process_close_request(js: Js) -> bool {
    sync_open_dialogs(js);
    let Some(top) = peek(js, |page| page.close_watchers.last().copied()).flatten() else {
        return false;
    };
    if !is_named(Some(top), b"dialog") || is_showing(top) {
        popover::hide(js, top, true, true, None);
        return true;
    }
    if !close_watcher_enabled(js, top) {
        return false;
    }
    request_close_watcher(js, top, js.has_transient_activation());
    true
}
