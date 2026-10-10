//! Southstar — the select and option bindings: selected, selectedIndex, options, add, and keyboard option choice.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::VecDeque;

use southstar_dom::{children, controls, select};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Page};
use crate::{Element, JsResult, checkable, name_is, named, parent_select};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) fn options(select: Element) -> Vec<Element> {
    let mut out = Vec::new();
    for child in children(select) {
        if named(child, "option") {
            out.push(child);
        } else if named(child, "optgroup") {
            out.extend(children(child).filter(|o| named(*o, "option")));
        }
    }
    out
}

fn options_ignoring_case(select: Element) -> Vec<Element> {
    let mut out = Vec::new();
    for child in children(select) {
        if name_is(child, "option") {
            out.push(child);
        } else if name_is(child, "optgroup") {
            out.extend(children(child).filter(|o| name_is(*o, "option")));
        }
    }
    out
}

fn clear_selected(select: Element) {
    for option in options(select) {
        ffi::remove_attr(option, c"selected");
    }
}

fn choose(select: Element, chosen: Option<Element>) {
    clear_selected(select);
    match chosen {
        Some(option) => {
            ffi::set_attr(option, c"selected", b"");
            ffi::remove_attr(select, c"data-nd-noselect");
        }
        None => ffi::set_attr(select, c"data-nd-noselect", b"1"),
    }
}

pub(crate) fn select_value(page: Option<Page>, select: Element, value: &[u8]) {
    let chosen = options(select)
        .into_iter()
        .find(|o| select::option_value(*o) == value);
    choose(select, chosen);
    Page::mark_mutated(page);
}

pub(crate) fn value(select: Element) -> Vec<u8> {
    let option = if select.attr(c"multiple").is_some() {
        select::first_selected_option(select)
    } else {
        select::chosen_option(select)
    };
    option.map(select::option_value).unwrap_or_default()
}

pub(crate) fn selected(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(n) = ffi::element(this) else {
        return Ok(Value::boolean(false));
    };
    if n.attr(c"selected").is_some() {
        return Ok(Value::boolean(true));
    }
    if !named(n, "option") {
        return Ok(Value::boolean(false));
    }
    let chosen = parent_select(n)
        .is_some_and(|p| p.attr(c"multiple").is_none() && select::chosen_option(p) == Some(n));
    Ok(Value::boolean(chosen))
}

pub(crate) fn set_selected(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(n) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let page = Page::of(scope);
    let on = args.first().is_some_and(|v| scope.to_bool(v));
    let is_option = named(n, "option");
    let owner = if is_option { parent_select(n) } else { None };
    if let Some(sel) = owner {
        ffi::remove_attr(sel, c"data-nd-noselect");
        if on && sel.attr(c"multiple").is_none() {
            for option in options(sel).into_iter().filter(|o| *o != n) {
                ffi::remove_attr(option, c"selected");
            }
        }
    }
    if on {
        Page::set_attr_recorded(page, n, c"selected", b"");
    } else {
        Page::remove_attr_recorded(page, n, c"selected");
    }
    Ok(Value::undefined())
}

pub(crate) fn default_selected(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    checkable::default_flag(this, c"selected")
}

pub(crate) fn set_default_selected(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    checkable::set_default_flag(scope, this, args, c"selected")
}

fn selected_index_of(el: Option<Element>) -> i32 {
    let Some(sel) = el.filter(|e| named(*e, "select")) else {
        return -1;
    };
    let Some(chosen) = select::chosen_option(sel) else {
        return -1;
    };
    options(sel)
        .iter()
        .position(|o| *o == chosen)
        .map_or(-1, |i| i as i32)
}

pub(crate) fn selected_index(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::int(selected_index_of(ffi::element(this))))
}

pub(crate) fn set_selected_index(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(sel) = ffi::element(this).filter(|e| named(*e, "select")) else {
        return Ok(Value::undefined());
    };
    let wanted = args.first().map_or(-1, |v| scope.to_int32(v).unwrap_or(0));
    let chosen = usize::try_from(wanted)
        .ok()
        .and_then(|i| options(sel).get(i).copied());
    choose(sel, chosen);
    Page::mark_mutated(Page::of(scope));
    Ok(Value::undefined())
}

fn options_owner(scope: &mut Scope<'_>, this: &Value) -> Value {
    scope
        .get(this, "__ns_select")
        .unwrap_or_else(|_| Value::undefined())
}

fn options_selected_index(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let owner = options_owner(scope, this);
    selected_index(scope, &owner, args)
}

fn options_set_selected_index(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let owner = options_owner(scope, this);
    set_selected_index(scope, &owner, args)
}

fn option_array(scope: &mut Scope<'_>, items: &[Element]) -> JsResult {
    let array = scope.new_array();
    for (i, item) in items.iter().enumerate() {
        let wrapped = ffi::wrap(scope, Some(*item));
        scope.set_index(&array, i as u32, wrapped)?;
    }
    let item = ffi::array_item_function(scope);
    scope.define(&array, "item", item, HIDDEN)?;
    let named_item = ffi::array_named_item_function(scope);
    scope.define(&array, "namedItem", named_item, HIDDEN)?;
    Ok(array)
}

pub(crate) fn options_getter(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let el = ffi::element(this);
    let items = el
        .filter(|e| named(*e, "select"))
        .map(options)
        .unwrap_or_default();
    let array = option_array(scope, &items)?;
    if let Some(el) = el {
        let owner = ffi::wrap(scope, Some(el));
        scope.define(&array, "__ns_select", owner, HIDDEN)?;
        let getter = scope.function("selectedIndex", 0, options_selected_index);
        let setter = scope.function("selectedIndex", 1, options_set_selected_index);
        scope.define_accessor(
            &array,
            "selectedIndex",
            Some(&getter),
            Some(&setter),
            Attributes {
                writable: false,
                enumerable: true,
                configurable: true,
            },
        )?;
    }
    Ok(array)
}

pub(crate) fn selected_options(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let items = match ffi::element(this).filter(|e| named(*e, "select")) {
        Some(sel) if sel.attr(c"multiple").is_none() => {
            select::chosen_option(sel).into_iter().collect()
        }
        Some(sel) => options(sel)
            .into_iter()
            .filter(|o| o.attr(c"selected").is_some())
            .collect(),
        None => Vec::new(),
    };
    option_array(scope, &items)
}

pub(crate) fn item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::null());
    };
    let index = scope.to_int32(arg).unwrap_or(0);
    let (Ok(index), Some(el)) = (usize::try_from(index), ffi::element(this)) else {
        return Ok(Value::null());
    };
    match options_ignoring_case(el).get(index) {
        Some(option) => Ok(ffi::wrap(scope, Some(*option))),
        None => Ok(Value::null()),
    }
}

pub(crate) fn named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first().filter(|a| a.is_string()) else {
        return Ok(Value::null());
    };
    let Ok(name) = scope.to_bytes(arg).map(crate::until_nul) else {
        return Ok(Value::null());
    };
    let Some(el) = ffi::element(this) else {
        return Ok(Value::null());
    };
    let matches = |o: Element, attr| o.attr(attr).is_some_and(|v| v.to_bytes() == name);
    let mut queue = VecDeque::from([el]);
    while let Some(n) = queue.pop_front() {
        for child in children(n).filter(|c| c.element_name().is_some()) {
            if name_is(child, "option") {
                if matches(child, c"id") || matches(child, c"name") {
                    return Ok(ffi::wrap(scope, Some(child)));
                }
            } else {
                queue.push_back(child);
            }
        }
    }
    Ok(Value::null())
}

pub(crate) fn add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(sel), Some(opt)) = (ffi::element(this), args.first().and_then(ffi::element)) else {
        return Ok(Value::undefined());
    };
    let page = Page::of(scope);
    let before = match args.get(1) {
        Some(b) if !b.is_null() && !b.is_undefined() && b.is_number() => {
            let index = scope.to_int32(b).unwrap_or(0);
            usize::try_from(index)
                .ok()
                .and_then(|i| children(sel).filter(|c| name_is(*c, "option")).nth(i))
        }
        Some(b) if !b.is_null() && !b.is_undefined() => ffi::element(b),
        _ => None,
    };
    if opt.parent().is_some() {
        ffi::node_remove(opt);
    }
    match before.filter(|b| b.parent() == Some(sel)) {
        Some(before) => ffi::insert_before_single(page, sel, opt, before),
        None => ffi::append_child(sel, opt),
    }
    Page::mark_mutated(page);
    Ok(Value::undefined())
}

pub(crate) fn choose_option(page: Page, option: Element) -> bool {
    let Some(sel) = parent_select(option) else {
        return false;
    };
    if controls::effectively_disabled(option) {
        return false;
    }
    clear_selected(sel);
    ffi::set_attr(option, c"selected", b"");
    ffi::remove_attr(sel, c"data-nd-noselect");
    page.dispatch_event(sel, c"input");
    page.dispatch_event(sel, c"change");
    Page::mark_mutated(Some(page));
    true
}

pub(crate) fn toggle_option(page: Page, option: Element) -> bool {
    let Some(sel) = parent_select(option) else {
        return false;
    };
    if controls::effectively_disabled(option) {
        return false;
    }
    if option.attr(c"selected").is_some() {
        ffi::remove_attr(option, c"selected");
    } else {
        ffi::set_attr(option, c"selected", b"");
    }
    ffi::remove_attr(sel, c"data-nd-noselect");
    page.dispatch_event(sel, c"input");
    page.dispatch_event(sel, c"change");
    Page::mark_mutated(Some(page));
    true
}

fn current_index(sel: Element, opts: &[Element]) -> Option<usize> {
    opts.iter()
        .rposition(|o| o.attr(c"selected").is_some())
        .or_else(|| {
            let chosen = select::chosen_option(sel)?;
            opts.iter().position(|o| *o == chosen)
        })
}

pub(crate) fn step(page: Page, sel: Element, dir: i32) -> bool {
    if !named(sel, "select") {
        return false;
    }
    let opts = options(sel);
    if opts.is_empty() {
        return false;
    }
    let len = opts.len() as i64;
    let mut idx = match current_index(sel, &opts) {
        Some(cur) => cur as i64,
        None if dir > 0 => -1,
        None => len,
    };
    for _ in 0..opts.len() {
        idx += if dir > 0 { 1 } else { -1 };
        if idx < 0 || idx >= len {
            break;
        }
        let option = opts[idx as usize];
        if !controls::effectively_disabled(option) {
            return choose_option(page, option);
        }
    }
    false
}

pub(crate) fn edge(page: Page, sel: Element, last: bool) -> bool {
    if !named(sel, "select") {
        return false;
    }
    let opts = options(sel);
    let pick = if last {
        opts.iter()
            .rev()
            .find(|o| !controls::effectively_disabled(**o))
    } else {
        opts.iter().find(|o| !controls::effectively_disabled(**o))
    };
    pick.is_some_and(|o| choose_option(page, *o))
}

pub(crate) fn typeahead(page: Page, sel: Element, key: &[u8]) -> bool {
    if key.is_empty() || !named(sel, "select") {
        return false;
    }
    let opts = options(sel);
    if opts.is_empty() {
        return false;
    }
    let start = current_index(sel, &opts).map_or(-1, |c| c as i64);
    let wanted = ffi::utf8_casefold(key);
    let len = opts.len() as i64;
    for n in 1..=len {
        let option = opts[((start + n) % len) as usize];
        if controls::effectively_disabled(option) {
            continue;
        }
        let label = ffi::utf8_casefold(&select::option_label(option));
        if label.starts_with(&wanted) {
            return choose_option(page, option);
        }
    }
    false
}
