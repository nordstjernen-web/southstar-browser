//! Southstar — the popover attribute and state machine: show, hide and toggle, the auto and hint stacks, light dismiss and the toggle tasks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{
    Element, JsResult, arg, flat_inclusive_ancestors, has_attr, inclusive_ancestor, info,
    info_peek, is_flat_inclusive_descendant, is_html_element, is_named, peek, with,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopoverType {
    None,
    Auto,
    Manual,
    Hint,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Validity {
    Valid,
    WrongState,
    NotSupported,
    InvalidState,
}

const OPEN_ATTR: &CStr = c"data-nd-popover-open";

pub(crate) fn type_of_value(v: Option<&[u8]>) -> PopoverType {
    match v {
        None => PopoverType::None,
        Some(v) if v.is_empty() || v.eq_ignore_ascii_case(b"auto") => PopoverType::Auto,
        Some(v) if v.eq_ignore_ascii_case(b"hint") => PopoverType::Hint,
        Some(_) => PopoverType::Manual,
    }
}

pub(crate) fn type_of(el: Option<Element>) -> PopoverType {
    match el {
        Some(el) if is_html_element(Some(el)) => {
            type_of_value(el.attr(c"popover").map(CStr::to_bytes))
        }
        _ => PopoverType::None,
    }
}

pub(crate) fn is_showing(el: Element) -> bool {
    has_attr(el, OPEN_ATTR)
}

fn set_showing(js: Js, el: Element, showing: bool) {
    if is_showing(el) == showing {
        return;
    }
    if showing {
        ffi::set_attr(el, OPEN_ATTR, c"");
    } else {
        ffi::remove_attr(el, OPEN_ATTR);
    }
    ffi::mark_attr_dirty(el, OPEN_ATTR, (!showing).then_some(c""));
    js.mark_mutated();
    js.request_repaint();
}

pub(crate) fn validity(
    js: Js,
    el: Element,
    expect_showing: bool,
    expected_doc: Option<Element>,
) -> Validity {
    if type_of(Some(el)) == PopoverType::None
        && info_peek(js, el, |pi| pi.type_changing).unwrap_or(0) == 0
    {
        return Validity::NotSupported;
    }
    if is_showing(el) != expect_showing {
        return Validity::WrongState;
    }
    if !crate::in_active_document(js, el)
        || expected_doc.is_some_and(|doc| el.root() != doc)
        || (is_named(Some(el), b"dialog")
            && has_attr(el, c"open")
            && crate::dialog::is_modal(js, el))
    {
        return Validity::InvalidState;
    }
    Validity::Valid
}

pub(crate) fn throw(scope: &mut Scope<'_>, result: Validity) -> JsResult {
    match result {
        Validity::NotSupported => Err(ffi::dom_exception(
            scope,
            c"NotSupportedError",
            9,
            "Not supported on elements that do not have a valid value for the 'popover' attribute.",
        )),
        Validity::InvalidState => Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            "Invalid on popover elements which aren't connected, are changing state, or are open modal dialogs.",
        )),
        _ => Ok(Value::undefined()),
    }
}

pub(crate) fn opened_mode(js: Js, el: Element) -> PopoverType {
    if crate::in_stack(js, false, el) {
        PopoverType::Auto
    } else if crate::in_stack(js, true, el) {
        PopoverType::Hint
    } else {
        PopoverType::None
    }
}

pub(crate) fn topmost_auto_or_hint(js: Js) -> Option<Element> {
    peek(js, |page| page.hint.last().or(page.auto.last()).copied()).flatten()
}

fn toggle_task(scope: &mut Scope<'_>, _this: &Value, _args: &[Value], data: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(&arg(data, 0)) else {
        return Ok(Value::undefined());
    };
    let dialog = scope.to_bool(&arg(data, 1));
    let fired = with(js, |page| {
        let pi = page.info.get_mut(&crate::key(el))?;
        let tracker = if dialog {
            &mut pi.dialog_toggle
        } else {
            &mut pi.popover_toggle
        };
        if tracker.timer == 0 {
            return None;
        }
        Some(core::mem::take(tracker))
    });
    if let Some(fired) = fired {
        js.fire_toggle(
            el,
            c"toggle",
            open_state(fired.old_open),
            open_state(fired.new_open),
            false,
            fired.source,
        );
    }
    Ok(Value::undefined())
}

pub(crate) fn open_state(open: bool) -> &'static CStr {
    if open { c"open" } else { c"closed" }
}

pub(crate) fn queue_toggle_task(
    js: Js,
    el: Element,
    dialog: bool,
    old_open: bool,
    new_open: bool,
    source: Option<Element>,
) {
    let pending = info(js, el, |pi| {
        let tracker = if dialog {
            &mut pi.dialog_toggle
        } else {
            &mut pi.popover_toggle
        };
        let pending = (tracker.timer != 0).then_some((tracker.timer, tracker.old_open));
        tracker.timer = 0;
        pending
    });
    let old_open = match pending {
        Some((timer, pending_old)) => {
            js.timer_remove(timer);
            pending_old
        }
        None => old_open,
    };
    let timer = js
        .scope(|scope| {
            let wrapper = ffi::wrap(scope, el);
            let task = scope.bound_function("", 0, toggle_task, &[wrapper, Value::boolean(dialog)]);
            ffi::set_timeout(scope, task).unwrap_or(0)
        })
        .unwrap_or(0);
    let source = source.map(ffi::note);
    info(js, el, |pi| {
        let tracker = if dialog {
            &mut pi.dialog_toggle
        } else {
            &mut pi.popover_toggle
        };
        tracker.timer = timer;
        tracker.old_open = old_open;
        tracker.new_open = new_open;
        tracker.source = source;
    });
}

pub(crate) fn topmost_ancestor(js: Js, node: Element, source: Option<Element>) -> Option<Element> {
    let open: Vec<Element> = peek(js, |page| {
        page.auto.iter().chain(page.hint.iter()).copied().collect()
    })
    .unwrap_or_default();
    open.into_iter().rev().find(|p| {
        is_flat_inclusive_descendant(node, *p)
            || source.is_some_and(|s| is_flat_inclusive_descendant(s, *p))
    })
}

fn hide_stack_until(js: Js, endpoint: Option<Element>, hint: bool, focus_prev: bool, fire: bool) {
    let list: Vec<Element> = peek(js, |page| page.stack(hint).clone()).unwrap_or_default();
    if list.is_empty() {
        return;
    }
    let keep = endpoint
        .and_then(|e| list.iter().position(|p| *p == e))
        .map_or(0, |index| index + 1);
    let to_hide: Vec<Element> = list[keep..].iter().rev().copied().collect();
    let remain: Vec<Element> = list[..keep].to_vec();
    for p in to_hide {
        if crate::has_info(js, p) {
            hide(js, p, focus_prev, fire, None);
        }
    }
    let to_check: Vec<Element> =
        peek(js, |page| page.stack(hint).iter().rev().copied().collect()).unwrap_or_default();
    for p in to_check {
        if remain.contains(&p) {
            continue;
        }
        if crate::has_info(js, p) {
            hide(js, p, focus_prev, false, None);
        }
    }
}

pub(crate) fn hide_until(js: Js, endpoint: Option<Element>, focus_prev: bool, fire: bool) {
    let endpoint_is_hint = endpoint.is_some_and(|e| crate::in_stack(js, true, e));
    hide_stack_until(js, endpoint, true, focus_prev, fire);
    let auto_endpoint = if endpoint_is_hint {
        peek(js, |page| page.hint_parent).flatten()
    } else {
        endpoint
    };
    hide_stack_until(js, auto_endpoint, false, focus_prev, fire);
}

fn hide_steps(
    js: Js,
    el: Element,
    focus_prev: bool,
    fire: bool,
    source: Option<Element>,
) -> Validity {
    let v = validity(js, el, true, None);
    if v != Validity::Valid {
        return v;
    }
    let nested = info(js, el, |pi| core::mem::replace(&mut pi.hiding, true));
    let fire = fire && !nested;
    with(js, |page| page.hiding_count += 1);
    let mut result = Validity::Valid;
    let in_auto = crate::in_stack(js, false, el);
    let in_hint = crate::in_stack(js, true, el);
    if in_auto || in_hint {
        if in_hint {
            hide_stack_until(js, Some(el), true, focus_prev, fire);
        }
        if peek(js, |page| page.hint_parent == Some(el)).unwrap_or(false) {
            hide_stack_until(js, None, true, focus_prev, fire);
        }
        if in_auto {
            hide_stack_until(js, Some(el), false, focus_prev, fire);
        }
        result = validity(js, el, true, None);
    }
    if result == Validity::Valid && fire {
        js.fire_toggle(el, c"beforetoggle", c"open", c"closed", false, source);
        result = validity(js, el, true, None);
    }
    info(js, el, |_| ());
    if result == Validity::Valid {
        info(js, el, |pi| pi.trigger = None);
        with(js, |page| {
            crate::remove_from(&mut page.auto, el);
            crate::remove_from(&mut page.hint, el);
            crate::remove_from(&mut page.close_watchers, el);
        });
        set_showing(js, el, false);
        with(js, |page| {
            if page.hint_parent == Some(el) || page.hint.is_empty() {
                page.hint_parent = None;
            }
        });
        if fire {
            queue_toggle_task(js, el, false, true, false, source);
        }
        let (had_prev, prev) = info(js, el, |pi| {
            let had = pi.has_prev_focus;
            let prev = pi.prev_focus;
            if had {
                pi.prev_focus = None;
                pi.has_prev_focus = false;
            }
            (had, prev)
        });
        if had_prev && focus_prev && js.focused().is_some() && inclusive_ancestor(el, js.focused())
        {
            match prev {
                None => js.set_focus(None),
                Some(prev) if !inclusive_ancestor(el, Some(prev)) => {
                    crate::focus::run_focusing_steps(js, prev)
                }
                Some(_) => {}
            }
        }
        info(js, el, |_| ());
    }
    if !nested {
        info(js, el, |pi| pi.hiding = false);
    }
    with(js, |page| page.hiding_count -= 1);
    result
}

pub(crate) fn hide(
    js: Js,
    el: Element,
    focus_prev: bool,
    fire: bool,
    source: Option<Element>,
) -> Validity {
    let _hold = js.hold(el);
    hide_steps(js, el, focus_prev, fire, source)
}

fn show_steps(js: Js, el: Element, source: Option<Element>) -> Validity {
    if peek(js, |page| page.showing || page.hiding_count > 0).unwrap_or(false) {
        return Validity::InvalidState;
    }
    let v = validity(js, el, false, None);
    if v != Validity::Valid {
        return v;
    }
    let doc = el.root();
    with(js, |page| page.showing = true);
    let stop = |v: Validity| {
        with(js, |page| page.showing = false);
        v
    };
    if js.fire_toggle(el, c"beforetoggle", c"closed", c"open", true, source) {
        return stop(Validity::Valid);
    }
    let v = validity(js, el, false, Some(doc));
    if v != Validity::Valid {
        return stop(v);
    }
    let mut restore_focus = false;
    let original = type_of(Some(el));
    let mut effective = original;
    let mut ancestor = None;
    if original == PopoverType::Auto || original == PopoverType::Hint {
        ancestor = topmost_ancestor(js, el, source);
        if let Some(a) = ancestor
            && effective == PopoverType::Auto
            && opened_mode(js, a) == PopoverType::Hint
        {
            effective = PopoverType::Hint;
        }
        let hold_ancestor = ancestor.and_then(|a| js.hold(a));
        hide_stack_until(js, ancestor, true, restore_focus, true);
        if effective == PopoverType::Auto {
            hide_stack_until(js, ancestor, false, restore_focus, true);
        }
        drop(hold_ancestor);
        if original != type_of(Some(el)) {
            return stop(Validity::InvalidState);
        }
        let v = validity(js, el, false, Some(doc));
        if v != Validity::Valid {
            return stop(v);
        }
        if topmost_auto_or_hint(js).is_none() {
            restore_focus = true;
        }
        with(js, |page| {
            page.stack_mut(effective == PopoverType::Hint).push(el);
        });
        crate::dialog::close_watcher_add(js, el);
    }
    info(js, el, |pi| {
        pi.prev_focus = None;
        pi.has_prev_focus = false;
    });
    let original_focus = js.focused();
    if effective == PopoverType::Hint
        && let Some(a) = ancestor
        && opened_mode(js, a) == PopoverType::Auto
    {
        let a = ffi::note(a);
        with(js, |page| page.hint_parent = Some(a));
    }
    set_showing(js, el, true);
    let trigger = source.map(ffi::note);
    info(js, el, |pi| pi.trigger = trigger);
    crate::focus::popover_focusing_steps(js, el);
    if restore_focus && type_of(Some(el)) != PopoverType::None {
        let prev = original_focus.map(ffi::note);
        info(js, el, |pi| {
            pi.has_prev_focus = true;
            pi.prev_focus = prev;
        });
    } else {
        info(js, el, |_| ());
    }
    with(js, |page| page.showing = false);
    queue_toggle_task(js, el, false, false, true, source);
    Validity::Valid
}

pub(crate) fn show(js: Js, el: Element, source: Option<Element>) -> Validity {
    let _hold = js.hold(el);
    show_steps(js, el, source)
}

fn source_option(scope: &mut Scope<'_>, options: &Value) -> JsResult<Option<Element>> {
    if !options.is_object() {
        return Ok(None);
    }
    let v = scope.get(options, "source")?;
    if v.is_undefined() {
        return Ok(None);
    }
    let n = ffi::unwrap(&v);
    if !is_html_element(n) {
        return Err(scope.type_error(
            "Failed to read the 'source' property: value is not of type 'HTMLElement'",
        ));
    }
    Ok(n)
}

pub(crate) fn show_popover(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let source = match args.first() {
        Some(options) => source_option(scope, options)?,
        None => None,
    };
    let result = show(js, el, source);
    throw(scope, result)
}

pub(crate) fn hide_popover(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let result = hide(js, el, true, true, None);
    throw(scope, result)
}

pub(crate) fn toggle_popover(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this).filter(|_| !js.is_null()) else {
        return Ok(Value::boolean(false));
    };
    let mut force = None;
    let mut source = None;
    match args.first() {
        Some(options) if options.is_object() => {
            let fv = scope.get(options, "force")?;
            if !fv.is_undefined() {
                force = Some(scope.to_bool(&fv));
            }
            source = source_option(scope, options)?;
        }
        Some(v) if !v.is_undefined() && !v.is_null() => force = Some(scope.to_bool(v)),
        _ => {}
    }
    let result = if is_showing(el) && force != Some(true) {
        hide(js, el, true, true, None)
    } else if force != Some(false) {
        show(js, el, source)
    } else {
        validity(js, el, is_showing(el), None)
    };
    if matches!(result, Validity::NotSupported | Validity::InvalidState) {
        return throw(scope, result);
    }
    Ok(Value::boolean(is_showing(el)))
}

pub(crate) fn get_popover(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(match type_of(ffi::unwrap(this)) {
        PopoverType::Auto => scope.string("auto"),
        PopoverType::Manual => scope.string("manual"),
        PopoverType::Hint => scope.string("hint"),
        PopoverType::None => Value::null(),
    })
}

pub(crate) fn set_popover(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    let val = arg(args, 0);
    if val.is_null() || val.is_undefined() {
        js.remove_attr_recorded(el, c"popover");
        return Ok(Value::undefined());
    }
    let s = scope.to_bytes(&val)?;
    js.set_attr_recorded(el, c"popover", &s);
    Ok(Value::undefined())
}

fn nearest_open(js: Js, node: Element) -> Option<Element> {
    flat_inclusive_ancestors(node)
        .find(|c| c.is_element() && is_showing(*c) && opened_mode(js, *c) != PopoverType::None)
}

fn nearest_target(js: Js, node: Element) -> Option<Element> {
    flat_inclusive_ancestors(node).find_map(|c| {
        if !c.is_element() {
            return None;
        }
        let t = if is_named(Some(c), b"button") {
            crate::invoker::button_target_popover(js, c)
        } else {
            crate::invoker::popover_target_element(js, c)
        };
        let ty = type_of(t);
        t.filter(|t| matches!(ty, PopoverType::Auto | PopoverType::Hint) && is_showing(*t))
    })
}

fn stack_position(js: Js, el: Option<Element>) -> usize {
    let Some(el) = el else {
        return 0;
    };
    peek(js, |page| {
        if let Some(index) = page.hint.iter().position(|p| *p == el) {
            index + page.auto.len() + 1
        } else if let Some(index) = page.auto.iter().position(|p| *p == el) {
            index + 1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

fn topmost_clicked(js: Js, node: Element) -> Option<Element> {
    let clicked = nearest_open(js, node);
    let target = nearest_target(js, node);
    if stack_position(js, clicked) > stack_position(js, target) {
        clicked
    } else {
        target
    }
}

pub(crate) fn light_dismiss(js: Js, target: Element, up: bool) {
    if topmost_auto_or_hint(js).is_none() {
        return;
    }
    let clicked = topmost_clicked(js, target);
    if !up {
        let clicked = clicked.map(ffi::note);
        with(js, |page| page.popover_pointerdown = clicked);
        return;
    }
    let same = with(js, |page| page.popover_pointerdown.take() == clicked);
    if same {
        hide_until(js, clicked, false, true);
    }
}

pub(crate) fn removing_steps(js: Js, el: Element) {
    if !crate::has_info(js, el) {
        return;
    }
    if is_named(Some(el), b"dialog") {
        crate::dialog::removing_steps(js, el);
    }
    if is_showing(el) {
        hide(js, el, false, false, None);
    }
}

pub(crate) fn attr_changed(
    js: Js,
    el: Element,
    attr: &[u8],
    old_value: Option<&CStr>,
    new_value: Option<&CStr>,
) {
    if attr.eq_ignore_ascii_case(b"open") && is_named(Some(el), b"dialog") {
        if new_value.is_none() && old_value.is_some() {
            crate::dialog::close_watcher_remove(js, el);
        } else if new_value.is_some() && old_value.is_none() && crate::is_connected(el) {
            crate::dialog::close_watcher_add(js, el);
        }
        return;
    }
    if attr.eq_ignore_ascii_case(b"popovertarget") {
        crate::invoker::set_explicit(js, el, c"popovertarget", None);
        return;
    }
    if attr.eq_ignore_ascii_case(b"commandfor") {
        crate::invoker::set_explicit(js, el, c"commandfor", None);
        return;
    }
    if !attr.eq_ignore_ascii_case(b"popover") || !is_html_element(Some(el)) || !is_showing(el) {
        return;
    }
    if type_of_value(old_value.map(CStr::to_bytes)) == type_of_value(new_value.map(CStr::to_bytes))
    {
        return;
    }
    info(js, el, |pi| pi.type_changing += 1);
    hide(js, el, true, true, None);
    if crate::has_info(js, el) {
        info(js, el, |pi| pi.type_changing -= 1);
    }
}
