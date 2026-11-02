//! Southstar — form-associated custom elements: the definition's formAssociated flag and the formAssociatedCallback and formDisabledCallback reactions to form owner and disabled-state changes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{ancestors, children, controls, index, internals};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::reactions::call_callback;
use crate::{Element, MAX_DEPTH, get, truthy};

pub(crate) const FORM_ASSOCIATED: &str = "__nd_ce_form_associated";

pub(crate) fn mark(scope: &mut Scope<'_>, node: Element, class: &Value) {
    if truthy(scope, class, FORM_ASSOCIATED) {
        internals::mark_form_associated(node);
        let page = southstar_js_runtime::api::Page::current(scope);
        southstar_js_collections::qcache_invalidate(page);
    }
}

fn is_form(node: Element) -> bool {
    node.element_name() == Some(b"form".as_slice())
}

fn owner_within(node: Element, top: Element) -> Option<Element> {
    if let Some(id) = node.attr(c"form") {
        return index::find_by_id(top, id).filter(|form| is_form(*form));
    }
    if node == top {
        return None;
    }
    let mut passed_top = false;
    ancestors(node)
        .take_while(|p| {
            let inside = !passed_top;
            passed_top = *p == top;
            inside
        })
        .find(|p| is_form(*p))
}

fn update(
    scope: &mut Scope<'_>,
    js: Js,
    node: Element,
    owner: Option<Element>,
    disabled: bool,
    upgraded: bool,
) {
    let element = ffi::node_wrapper(scope, node);
    let class = get(scope, &element, "__nd_ce_class");
    if !class.is_object() {
        return;
    }
    let previous = internals::swap_owner(node, owner).and_then(|weak| weak.get());
    if previous != owner && (!upgraded || owner.is_some()) {
        let form = owner.map_or_else(Value::null, |form| ffi::wrap_node(scope, form));
        call_callback(
            scope,
            js,
            &element,
            &class,
            "formAssociatedCallback",
            &[form],
        );
    }
    let was_disabled = internals::swap_disabled(node, disabled);
    if was_disabled != disabled && (!upgraded || disabled) {
        call_callback(
            scope,
            js,
            &element,
            &class,
            "formDisabledCallback",
            &[Value::boolean(disabled)],
        );
    }
}

pub(crate) fn upgraded(scope: &mut Scope<'_>, js: Js, node: Element) {
    if internals::is_form_associated(node) {
        let owner = controls::form_owner(node);
        update(
            scope,
            js,
            node,
            owner,
            controls::effectively_disabled(node),
            true,
        );
    }
}

pub(crate) fn refresh(scope: &mut Scope<'_>, js: Js, node: Element) {
    if internals::is_form_associated(node) {
        let owner = controls::form_owner(node);
        update(
            scope,
            js,
            node,
            owner,
            controls::effectively_disabled(node),
            false,
        );
    }
}

pub(crate) fn removing(scope: &mut Scope<'_>, js: Js, node: Element, top: Element) {
    if internals::is_form_associated(node) {
        let owner = owner_within(node, top);
        let disabled = controls::disabled_within(node, top);
        update(scope, js, node, owner, disabled, false);
    }
}

fn refresh_descendants(scope: &mut Scope<'_>, js: Js, node: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(node) {
        refresh(scope, js, child);
        refresh_descendants(scope, js, child, depth + 1);
    }
}

pub(crate) fn attribute_changed(js: Js, node: Element, attr_name: &str) {
    if attr_name.eq_ignore_ascii_case("id") && is_form(node) {
        ffi::main_scope(js, |scope| refresh_descendants(scope, js, node.root(), 0));
        return;
    }
    let disabled = attr_name.eq_ignore_ascii_case("disabled");
    if !disabled && !attr_name.eq_ignore_ascii_case("form") {
        return;
    }
    let fieldset = disabled && node.element_name() == Some(b"fieldset".as_slice());
    if !fieldset && !internals::is_form_associated(node) {
        return;
    }
    ffi::main_scope(js, |scope| {
        if fieldset {
            refresh_descendants(scope, js, node, 0);
        } else {
            refresh(scope, js, node);
        }
    });
}
