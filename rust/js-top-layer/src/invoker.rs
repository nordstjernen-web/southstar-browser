//! Southstar — invokers: popovertarget and commandfor buttons, their element reflection, the command event and button activation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::index;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::dialog::{self, Command as DialogCommand};
use crate::ffi::{self, Js};
use crate::popover::{self, PopoverType, Validity, is_showing, type_of};
use crate::{
    AttrRef, Element, JsResult, arg, attr_is, inclusive_ancestor, is_connected, is_html_element,
    is_named, peek, tree_root, with,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Unknown,
    Custom,
    TogglePopover,
    ShowPopover,
    HidePopover,
    Close,
    RequestClose,
    ShowModal,
}

const COMMANDS: [(Command, &str); 6] = [
    (Command::TogglePopover, "toggle-popover"),
    (Command::ShowPopover, "show-popover"),
    (Command::HidePopover, "hide-popover"),
    (Command::Close, "close"),
    (Command::RequestClose, "request-close"),
    (Command::ShowModal, "show-modal"),
];

fn command_of(v: Option<&[u8]>) -> Command {
    let Some(v) = v else {
        return Command::Unknown;
    };
    if v.starts_with(b"--") {
        return Command::Custom;
    }
    COMMANDS
        .iter()
        .find(|(_, name)| v.eq_ignore_ascii_case(name.as_bytes()))
        .map_or(Command::Unknown, |(cmd, _)| *cmd)
}

fn command_attr(el: Element) -> Command {
    command_of(el.attr(c"command").map(CStr::to_bytes))
}

fn is_popover_command(cmd: Command) -> bool {
    matches!(
        cmd,
        Command::TogglePopover | Command::ShowPopover | Command::HidePopover
    )
}

fn command_is_valid(cmd: Command, target: Element) -> bool {
    match cmd {
        Command::Unknown => false,
        Command::Custom => true,
        _ if !is_html_element(Some(target)) => false,
        _ if is_popover_command(cmd) => true,
        _ => is_named(Some(target), b"dialog"),
    }
}

pub(crate) fn set_explicit(js: Js, owner: Element, attr: &'static CStr, target: Option<Element>) {
    let index = peek(js, |page| {
        page.attr_refs
            .iter()
            .position(|r| r.owner == owner && r.attr == attr)
    })
    .flatten();
    match (target, index) {
        (None, Some(index)) => {
            with(js, |page| page.attr_refs.remove(index));
        }
        (None, None) => {}
        (Some(target), Some(index)) => {
            let target = ffi::note(target);
            with(js, |page| page.attr_refs[index].target = target);
        }
        (Some(target), None) => {
            let owner = ffi::note(owner);
            let target = ffi::note(target);
            with(js, |page| {
                page.attr_refs.push(AttrRef {
                    owner,
                    attr,
                    target,
                })
            });
        }
    }
}

fn associated_element(js: Js, el: Element, attr: &'static CStr) -> Option<Element> {
    let explicit = peek(js, |page| {
        page.attr_refs
            .iter()
            .find(|r| r.owner == el && r.attr == attr)
            .map(|r| r.target)
    })
    .flatten();
    if let Some(t) = explicit {
        t.parent()?;
        let root = tree_root(t);
        return core::iter::successors(el.parent(), |p| p.parent())
            .any(|p| p == root)
            .then_some(t);
    }
    let id = el.attr(attr).filter(|id| !id.is_empty())?;
    index::find_by_id(tree_root(el), id).filter(|hit| hit.is_element())
}

fn attr_element_get(scope: &mut Scope<'_>, this: &Value, attr: &'static CStr) -> JsResult {
    let js = ffi::js_of(scope);
    let target = ffi::unwrap(this).and_then(|el| associated_element(js, el, attr));
    Ok(ffi::wrap_or_null(scope, target))
}

fn attr_element_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    attr: &'static CStr,
) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    if val.is_null() || val.is_undefined() {
        js.remove_attr_recorded(el, attr);
        set_explicit(js, el, attr, None);
        return Ok(Value::undefined());
    }
    let Some(target) = ffi::unwrap(val).filter(|t| t.is_element()) else {
        return Err(
            scope.type_error("Failed to set the attribute element: value is not of type 'Element'")
        );
    };
    js.set_attr_recorded(el, attr, b"");
    set_explicit(js, el, attr, Some(target));
    Ok(Value::undefined())
}

pub(crate) fn get_popover_target(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    attr_element_get(scope, this, c"popovertarget")
}

pub(crate) fn set_popover_target(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    attr_element_set(scope, this, &arg(args, 0), c"popovertarget")
}

pub(crate) fn get_command_for(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    attr_element_get(scope, this, c"commandfor")
}

pub(crate) fn set_command_for(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    attr_element_set(scope, this, &arg(args, 0), c"commandfor")
}

pub(crate) fn get_command(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let v = ffi::unwrap(this).and_then(|el| el.attr(c"command"));
    let name = match command_of(v.map(CStr::to_bytes)) {
        Command::Unknown => return Ok(scope.string("")),
        Command::Custom => return Ok(scope.string_from_bytes(v.map_or(&[][..], CStr::to_bytes))),
        cmd => COMMANDS
            .iter()
            .find(|(c, _)| *c == cmd)
            .map_or("", |(_, name)| name),
    };
    Ok(scope.string(name))
}

pub(crate) fn set_command(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    let s = scope.to_bytes(&arg(args, 0))?;
    ffi::js_of(scope).set_attr_recorded(el, c"command", &s);
    Ok(Value::undefined())
}

pub(crate) fn is_button(el: Option<Element>) -> bool {
    let Some(el) = el else {
        return false;
    };
    if is_named(Some(el), b"button") {
        return true;
    }
    if !is_named(Some(el), b"input") {
        return false;
    }
    ["submit", "reset", "image", "button"]
        .iter()
        .any(|t| attr_is(el, c"type", t.as_bytes()))
}

fn type_is_auto(el: Element) -> bool {
    !attr_is(el, c"type", b"submit")
        && !attr_is(el, c"type", b"reset")
        && !attr_is(el, c"type", b"button")
}

pub(crate) fn popover_target_element(js: Js, node: Element) -> Option<Element> {
    if !is_button(Some(node)) || ffi::effectively_disabled(node) {
        return None;
    }
    if ffi::is_submit_trigger(node) && js.form_owner(node).is_some() {
        return None;
    }
    let target = associated_element(js, node, c"popovertarget");
    target.filter(|t| type_of(Some(*t)) != PopoverType::None)
}

pub(crate) fn button_target_popover(js: Js, node: Element) -> Option<Element> {
    if ffi::effectively_disabled(node) {
        return None;
    }
    let Some(target) = associated_element(js, node, c"commandfor") else {
        return popover_target_element(js, node);
    };
    if js.form_owner(node).is_some()
        && (ffi::is_submit_trigger(node) || attr_is(node, c"type", b"reset") || type_is_auto(node))
    {
        return None;
    }
    if !is_popover_command(command_attr(node)) {
        return None;
    }
    (type_of(Some(target)) != PopoverType::None).then_some(target)
}

pub(crate) fn popover_target_activation(js: Js, node: Element, event_target: Option<Element>) {
    let Some(target) = popover_target_element(js, node) else {
        return;
    };
    if inclusive_ancestor(target, event_target)
        && inclusive_ancestor(node, Some(target))
        && target != node
    {
        return;
    }
    let showing = is_showing(target);
    if attr_is(node, c"popovertargetaction", b"show") && showing {
        return;
    }
    if attr_is(node, c"popovertargetaction", b"hide") && !showing {
        return;
    }
    if showing {
        popover::hide(js, target, true, true, Some(node));
    } else if popover::validity(js, target, false, None) == Validity::Valid {
        popover::show(js, target, Some(node));
    }
}

fn fire_command_event(js: Js, target: Element, command: &[u8], source: Element) -> bool {
    if js.halted() || js.in_pump() {
        return true;
    }
    let prevented = js
        .scope(|scope| {
            let event = ffi::make_event(scope, c"command", target);
            let _ = scope.set(&event, "bubbles", Value::boolean(false));
            let _ = scope.set(&event, "cancelable", Value::boolean(true));
            let _ = scope.set(&event, "composed", Value::boolean(true));
            let command = scope.string_from_bytes(command);
            let readonly = Attributes {
                writable: false,
                enumerable: true,
                configurable: true,
            };
            let _ = scope.define(&event, "command", command, readonly);
            let source = ffi::wrap(scope, source);
            ffi::define_event_source(scope, &event, source);
            ffi::adopt_interface(scope, &event, c"CommandEvent");
            js.dispatch_built(target, c"command", event)
        })
        .unwrap_or(false);
    !prevented
}

fn run_command(js: Js, button: Element, target: Element) {
    let attr: Option<CString> = button.attr(c"command").map(CStr::to_owned);
    let cmd = command_of(attr.as_deref().map(CStr::to_bytes));
    if !command_is_valid(cmd, target) {
        return;
    }
    let _hold = js.hold(target);
    let command = attr.as_deref().map_or(&[][..], CStr::to_bytes);
    let proceed = fire_command_event(js, target, command, button);
    if !proceed || !is_connected(target) || cmd == Command::Custom {
        return;
    }
    let valid = |showing| popover::validity(js, target, showing, None) == Validity::Valid;
    match cmd {
        Command::HidePopover => {
            if valid(true) {
                popover::hide(js, target, true, true, Some(button));
            }
        }
        Command::TogglePopover => {
            if valid(false) {
                popover::show(js, target, Some(button));
            } else if valid(true) {
                popover::hide(js, target, true, true, Some(button));
            }
        }
        Command::ShowPopover => {
            if valid(false) {
                popover::show(js, target, Some(button));
            }
        }
        _ if is_named(Some(target), b"dialog") => {
            let kind = match cmd {
                Command::Close => DialogCommand::Close,
                Command::RequestClose => DialogCommand::RequestClose,
                Command::ShowModal => DialogCommand::ShowModal,
                _ => return,
            };
            dialog::command_steps(js, target, button, kind);
        }
        _ => {}
    }
}

pub(crate) fn button_activation(js: Js, button: Element, event_target: Option<Element>) {
    if let Some(form) = js.form_owner(button) {
        if ffi::is_submit_trigger(button) {
            js.scope(|scope| ffi::submit_or_reset(scope, form, Some(button)));
            return;
        }
        if attr_is(button, c"type", b"reset") {
            js.scope(|scope| ffi::submit_or_reset(scope, form, None));
            return;
        }
        if type_is_auto(button) {
            return;
        }
    }
    match associated_element(js, button, c"commandfor") {
        Some(target) => run_command(js, button, target),
        None => popover_target_activation(js, button, event_target),
    }
}
