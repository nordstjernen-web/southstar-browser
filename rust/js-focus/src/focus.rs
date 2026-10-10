//! Southstar — focusable areas, the focus update steps with their events, sequential navigation and the focus(), blur() and hasFocus() bindings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::controls::effectively_disabled;
use southstar_dom::tree::effectively_inert;
use southstar_dom::{Kind, children};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{
    Element, Guard, JsResult, attr_is, has_attr, in_template_content, inclusive_ancestor, is_named,
    owner_document, top_document,
};

const MAX_DEPTH: i32 = 512;

pub(crate) fn tabindex(el: Element) -> Option<i32> {
    let text = el.attr(c"tabindex")?.to_bytes();
    Some(parse_tabindex(text))
}

fn parse_tabindex(text: &[u8]) -> i32 {
    let mut i = text
        .iter()
        .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(text.len());
    let negative = match text.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut value: i64 = 0;
    while let Some(d) = text.get(i).filter(|b| b.is_ascii_digit()) {
        value = (value * 10 + i64::from(d - b'0')).min(i64::from(i32::MAX) + 1);
        i += 1;
    }
    if i == start {
        return 0;
    }
    if text[i..].iter().any(|&b| b != b' ') {
        return 0;
    }
    let value = if negative { -value } else { value };
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn contenteditable(el: Element) -> bool {
    el.attr(c"contenteditable")
        .is_some_and(|ce| !ce.to_bytes().eq_ignore_ascii_case(b"false"))
}

pub(crate) fn is_focusable(el: Element) -> bool {
    let Some(name) = el.element_name() else {
        return false;
    };
    if effectively_disabled(el) || effectively_inert(el) || has_attr(el, c"hidden") {
        return false;
    }
    if has_attr(el, c"tabindex") {
        return true;
    }
    match name {
        b"a" | b"area" => has_attr(el, c"href"),
        b"button" | b"select" | b"textarea" | b"iframe" | b"summary" => true,
        b"input" => !attr_is(el, c"type", b"hidden"),
        _ => contenteditable(el),
    }
}

fn accepts_text_entry(el: Element) -> bool {
    if is_named(Some(el), b"textarea") {
        return true;
    }
    if is_named(Some(el), b"input") {
        const NON_TEXT: [&[u8]; 10] = [
            b"button",
            b"checkbox",
            b"color",
            b"file",
            b"hidden",
            b"image",
            b"radio",
            b"range",
            b"reset",
            b"submit",
        ];
        return !el.attr(c"type").is_some_and(|t| {
            NON_TEXT
                .iter()
                .any(|n| t.to_bytes().eq_ignore_ascii_case(n))
        });
    }
    contenteditable(el)
}

pub(crate) fn update_focus_visible(js: Js) {
    let (el, pointer) = crate::peek(js, |page| (page.focused, page.pointer_input));
    let visible = el.filter(|&el| !pointer || accepts_text_entry(el));
    ffi::set_focus_visible(visible);
}

fn focused_document(js: Js) -> Option<Element> {
    crate::peek(js, |page| page.focused_doc).or_else(|| top_document(js.current_document()))
}

fn dispatch_focus_event(js: Js, target: Element, kind: &CStr, related: Option<Element>) {
    if js.halted() || js.in_pump() {
        return;
    }
    js.scope(|scope| {
        let event = ffi::make_event(scope, kind, target);
        let (bubbles, cancelable) = ffi::event_flags(kind);
        let related = ffi::wrap_or_null(scope, related);
        let _ = scope.set(&event, "bubbles", Value::boolean(bubbles));
        let _ = scope.set(&event, "cancelable", Value::boolean(cancelable));
        let _ = scope.set(&event, "relatedTarget", related);
        js.dispatch_built(target, kind, event);
    });
}

fn guard(js: Js, index: usize, slot: usize) -> Option<Element> {
    crate::peek(js, |page| page.guards.get(index).and_then(|g| g[slot]))
}

fn has_focus_now(js: Js) -> bool {
    crate::focused(js).is_some()
}

fn window_doc(doc: Option<Element>) -> Option<Element> {
    doc.filter(|d| d.parent().is_some())
}

pub(crate) fn set_focus_in(js: Js, el: Option<Element>, doc: Option<Element>) {
    let old_doc = focused_document(js);
    let new_doc = match el {
        Some(el) => owner_document(el),
        None => doc.or(old_doc),
    };
    let old = crate::focused(js);
    if old == el && new_doc == old_doc {
        return;
    }
    let same_doc = new_doc == old_doc;
    let slots: Guard = [old, el, old_doc, new_doc];
    slots.iter().flatten().for_each(|&n| ffi::note(n));
    let index = crate::with(js, |page| {
        page.guards.push(slots);
        page.focused = None;
        page.guards.len() - 1
    });
    run_focus_update(js, index, same_doc);
    crate::with(js, |page| page.guards.truncate(index));
}

fn run_focus_update(js: Js, index: usize, same_doc: bool) {
    update_focus_visible(js);
    js.mark_mutated();
    if let Some(old) = guard(js, index, 0) {
        js.commit_change(old);
        if has_focus_now(js) {
            return;
        }
    }
    if let Some(old) = guard(js, index, 0) {
        let related = if same_doc { guard(js, index, 1) } else { None };
        dispatch_focus_event(js, old, c"blur", related);
        if guard(js, index, 0).is_some() {
            let related = if same_doc { guard(js, index, 1) } else { None };
            dispatch_focus_event(js, old, c"focusout", related);
        }
        if has_focus_now(js) {
            return;
        }
    }
    if !same_doc {
        let Some(new_doc) = guard(js, index, 3) else {
            return;
        };
        crate::with(js, |page| page.focused_doc = window_doc(Some(new_doc)));
        if let Some(old_doc) = guard(js, index, 2) {
            js.fire_window_focus_event(old_doc, c"blur");
        }
        if has_focus_now(js) {
            return;
        }
        let Some(new_doc) = guard(js, index, 3) else {
            return;
        };
        js.fire_window_focus_event(new_doc, c"focus");
        if has_focus_now(js) {
            return;
        }
    }
    let Some(el) = guard(js, index, 1) else {
        return;
    };
    let new_doc = guard(js, index, 3);
    crate::with(js, |page| {
        page.focused = Some(el);
        page.focused_doc = window_doc(new_doc);
    });
    update_focus_visible(js);
    crate::with(js, |page| page.nav_start = None);
    let related = if same_doc { guard(js, index, 0) } else { None };
    dispatch_focus_event(js, el, c"focus", related);
    if guard(js, index, 1).is_some() {
        let related = if same_doc { guard(js, index, 0) } else { None };
        dispatch_focus_event(js, el, c"focusin", related);
    }
}

pub(crate) fn set_focused_node(js: Js, el: Option<Element>) {
    if crate::focused(js) == el {
        return;
    }
    crate::with(js, |page| page.focused = el);
    update_focus_visible(js);
    js.mark_mutated();
}

pub(crate) fn focus_from_pointer(js: Js, target: Option<Element>) {
    let focus = target.and_then(|t| {
        southstar_dom::ancestors_and_self(t)
            .take_while(|a| a.kind() != Kind::Document)
            .find(|&a| is_focusable(a))
    });
    set_focus_in(js, focus, target.and_then(owner_document));
    let Some(target) = target.filter(|_| focus.is_none()) else {
        return;
    };
    crate::with(js, |page| page.nav_start = Some(target));
    ffi::note(target);
}

#[derive(Clone, Copy)]
struct Candidate {
    node: Element,
    tabindex: i32,
    order: u32,
}

fn is_candidate(c: Element) -> bool {
    is_focusable(c) && !tabindex(c).is_some_and(|ti| ti < 0)
}

fn collect_candidates(root: Element, out: &mut Vec<Candidate>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for c in children(root) {
        if !c.is_element() || in_template_content(c) {
            continue;
        }
        if is_candidate(c) {
            out.push(Candidate {
                node: c,
                tabindex: tabindex(c).unwrap_or(0),
                order: out.len() as u32,
            });
        }
        collect_candidates(c, out, depth + 1);
    }
}

fn candidates_before(root: Element, target: Element, count: &mut u32, depth: i32) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    for c in children(root) {
        if c == target {
            return true;
        }
        if !c.is_element() || in_template_content(c) {
            continue;
        }
        if is_candidate(c) {
            *count += 1;
        }
        if candidates_before(c, target, count, depth + 1) {
            return true;
        }
    }
    false
}

fn index_from_start(
    scope: Element,
    start: Option<Element>,
    cands: &[Candidate],
    backward: bool,
) -> Option<usize> {
    let mut before = 0;
    if !candidates_before(scope, start?, &mut before, 0) {
        return None;
    }
    let mut found = None;
    for (i, c) in cands.iter().enumerate() {
        if c.tabindex > 0 {
            continue;
        }
        if !backward && c.order >= before {
            return Some(i);
        }
        if backward && c.order < before {
            found = Some(i);
        }
    }
    found
}

fn focus_scope(js: Js) -> Option<Element> {
    let doc = js.current_document()?;
    match js.active_modal() {
        Some(modal) if inclusive_ancestor(Some(doc), Some(modal)) => Some(modal),
        _ => Some(doc),
    }
}

pub(crate) fn sequential_target(js: Js, backward: bool) -> Option<Element> {
    let scope = focus_scope(js)?;
    let mut cands = Vec::new();
    collect_candidates(scope, &mut cands, 0);
    if cands.is_empty() {
        return None;
    }
    cands.sort_by_key(|c| (if c.tabindex > 0 { c.tabindex } else { i32::MAX }, c.order));
    let focused = crate::focused(js);
    let current = cands.iter().position(|c| Some(c.node) == focused);
    let len = cands.len();
    let next = match current {
        None => {
            let start = crate::peek(js, |page| page.nav_start);
            index_from_start(scope, start, &cands, backward).unwrap_or(if backward {
                len - 1
            } else {
                0
            })
        }
        Some(0) if backward => len - 1,
        Some(cur) if backward => cur - 1,
        Some(cur) => (cur + 1) % len,
    };
    Some(cands[next].node)
}

pub(crate) fn focus_method(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() || !is_focusable(el) {
        return Ok(Value::undefined());
    }
    if !inclusive_ancestor(top_document(js.current_document()), Some(el)) {
        return Ok(Value::undefined());
    }
    set_focus_in(js, Some(el), None);
    Ok(Value::undefined())
}

pub(crate) fn blur_method(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() || crate::focused(js) != Some(el) {
        return Ok(Value::undefined());
    }
    set_focus_in(js, None, None);
    Ok(Value::undefined())
}

pub(crate) fn has_focus(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::boolean(false));
    }
    let doc = ffi::unwrap(this)
        .filter(|n| n.kind() == Kind::Document)
        .or_else(|| js.current_document());
    let Some(doc) = doc else {
        return Ok(Value::boolean(false));
    };
    Ok(Value::boolean(inclusive_ancestor(
        Some(doc),
        focused_document(js),
    )))
}
