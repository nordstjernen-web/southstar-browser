//! Southstar — checkedness, indeterminate, radio groups and checkbox/radio activation behaviour.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::VecDeque;

use southstar_dom::{Kind, ancestors_and_self, children, controls};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Page};
use crate::{Element, JsResult, form_owner, labels, named, type_is, type_of};

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_INPUT_INDETERMINATE: u32 = 1 << 18;

pub(crate) struct ClickState {
    pub(crate) checked: bool,
    pub(crate) indeterminate: bool,
    pub(crate) checked_radio: Option<Element>,
}

pub(crate) fn kind(el: Element) -> i32 {
    if !named(el, "input") {
        return 0;
    }
    if type_is(el, "checkbox") {
        1
    } else if type_is(el, "radio") {
        2
    } else {
        0
    }
}

pub(crate) fn is_indeterminate(el: Element) -> bool {
    el.flags() & FLAG_INPUT_INDETERMINATE != 0
}

fn restyle(el: Element) {
    ffi::mark_restyle_dirty(el.parent().unwrap_or(el));
}

pub(crate) fn set_indeterminate(page: Option<Page>, el: Element, on: bool) {
    if on {
        el.add_flags(FLAG_INPUT_INDETERMINATE);
    } else {
        el.remove_flags(FLAG_INPUT_INDETERMINATE);
    }
    Page::mark_mutated(page);
    restyle(el);
}

pub(crate) fn set_checkedness(page: Option<Page>, el: Element, checked: bool) {
    let value: &[u8] = if checked { b"1" } else { b"0" };
    Page::set_attr_recorded(page, el, c"data-nd-checked", value);
}

pub(crate) fn clear_radio_group(page: Option<Page>, radio: Element) -> Option<Element> {
    let group = radio.attr(c"name")?.to_bytes().to_vec();
    let doc = page
        .and_then(Page::current_document)
        .unwrap_or_else(|| radio.root());
    let owner = form_owner(radio);
    let mut checked = None;
    let mut queue = VecDeque::from([doc]);
    while let Some(n) = queue.pop_front() {
        if n != radio
            && named(n, "input")
            && type_of(n).is_some()
            && type_is(n, "radio")
            && n.attr(c"name").is_some_and(|nm| nm.to_bytes() == group)
            && form_owner(n) == owner
        {
            if controls::is_checked(n) {
                checked = Some(n);
            }
            set_checkedness(page, n, false);
        }
        queue.extend(children(n));
    }
    checked
}

pub(crate) fn pre_click(page: Option<Page>, el: Element, kind: i32) -> ClickState {
    let mut state = ClickState {
        checked: controls::is_checked(el),
        indeterminate: is_indeterminate(el),
        checked_radio: None,
    };
    if kind == 1 {
        set_checkedness(page, el, !state.checked);
        set_indeterminate(page, el, false);
    } else if !state.checked {
        state.checked_radio = clear_radio_group(page, el);
        set_checkedness(page, el, true);
    }
    state
}

fn connected(el: Element) -> bool {
    ancestors_and_self(el).any(|p| p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0)
}

pub(crate) fn post_click(
    page: Option<Page>,
    el: Element,
    kind: i32,
    state: &ClickState,
    prevented: bool,
) {
    if prevented {
        if kind == 1 {
            set_checkedness(page, el, state.checked);
            set_indeterminate(page, el, state.indeterminate);
        } else if !state.checked {
            set_checkedness(page, el, false);
            if let Some(radio) = state.checked_radio.filter(|r| self::kind(*r) == 2) {
                clear_radio_group(page, radio);
                set_checkedness(page, radio, true);
            }
        }
        return;
    }
    if kind == 2 && state.checked {
        return;
    }
    if !connected(el) {
        return;
    }
    if let Some(page) = page {
        page.dispatch_event(el, c"input");
        page.dispatch_event(el, c"change");
    }
}

pub(crate) fn click_activate(page: Page, node: Element) -> bool {
    let mut control = None;
    for cur in ancestors_and_self(node) {
        if named(cur, "label") {
            control = labels::control_in_document(cur, page.current_document());
            break;
        }
        if named(cur, "input") {
            control = Some(cur);
            break;
        }
    }
    let Some(control) = control.filter(|c| named(*c, "input")) else {
        return false;
    };
    if controls::effectively_disabled(control) || southstar_dom::tree::effectively_inert(control) {
        return false;
    }
    if type_of(control).is_none() {
        return false;
    }
    if type_is(control, "checkbox") {
        set_checkedness(Some(page), control, !controls::is_checked(control));
    } else if type_is(control, "radio") {
        clear_radio_group(Some(page), control);
        set_checkedness(Some(page), control, true);
    } else {
        return false;
    }
    page.dispatch_event(control, c"input");
    page.dispatch_event(control, c"change");
    Page::mark_mutated(Some(page));
    true
}

pub(crate) fn checked(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(
        ffi::element(this).is_some_and(controls::is_checked),
    ))
}

pub(crate) fn set_checked(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(el) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let page = Page::of(scope);
    let on = args.first().is_some_and(|v| scope.to_bool(v));
    if on {
        if type_is(el, "radio") {
            clear_radio_group(page, el);
        }
        ffi::set_attr(el, c"data-nd-checked", b"1");
    } else {
        ffi::set_attr(el, c"data-nd-checked", b"0");
    }
    Page::mark_mutated(page);
    restyle(el);
    Ok(Value::undefined())
}

pub(crate) fn indeterminate(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(
        ffi::element(this).is_some_and(is_indeterminate),
    ))
}

pub(crate) fn set_indeterminate_prop(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if let Some(el) = ffi::element(this) {
        let on = args.first().is_some_and(|v| scope.to_bool(v));
        set_indeterminate(Page::of(scope), el, on);
    }
    Ok(Value::undefined())
}

pub(crate) fn default_flag(this: &Value, attr: &core::ffi::CStr) -> JsResult {
    Ok(Value::boolean(
        ffi::element(this).is_some_and(|el| el.attr(attr).is_some()),
    ))
}

pub(crate) fn set_default_flag(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    attr: &core::ffi::CStr,
) -> JsResult {
    if let Some(el) = ffi::element(this) {
        let page = Page::of(scope);
        if args.first().is_some_and(|v| scope.to_bool(v)) {
            Page::set_attr_recorded(page, el, attr, b"");
        } else {
            Page::remove_attr_recorded(page, el, attr);
        }
    }
    Ok(Value::undefined())
}

pub(crate) fn default_checked(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    default_flag(this, c"checked")
}

pub(crate) fn set_default_checked(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    set_default_flag(scope, this, args, c"checked")
}
