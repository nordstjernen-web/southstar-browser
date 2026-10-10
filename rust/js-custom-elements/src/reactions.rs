//! Southstar — custom element upgrades and the connected, disconnected and attributeChanged callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, Key, MAX_DEPTH, Page, WRITABLE_CONFIGURABLE, get, truthy};

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;
const PROTOTYPE_DEPTH: usize = 8;

pub(crate) fn key_for(js: Js, lower_name: &str, document: Option<Element>) -> Key {
    let document = match document {
        Some(doc) if doc.as_ptr() as usize != ffi::main_document(js) => doc.as_ptr() as usize,
        _ => 0,
    };
    Key {
        name: lower_name.to_owned(),
        document,
    }
}

fn node_document(scope: &mut Scope<'_>, js: Js, node: Element) -> Option<Element> {
    if let Some(doc) = southstar_dom::ancestors_and_self(node)
        .find(|p| p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0)
    {
        return Some(doc);
    }
    let wrapper = ffi::node_wrapper(scope, node);
    if wrapper.is_object() {
        let owner = get(scope, &wrapper, "__ndOwnerDoc");
        if let Some(doc) = ffi::unwrap_node(&owner) {
            return Some(doc);
        }
    }
    ffi::current_document(js)
}

fn in_template_content(node: Element) -> bool {
    southstar_dom::ancestors_and_self(node).any(|p| {
        p.flags() & FLAG_TEMPLATE_CONTENT != 0 || p.element_name() == Some(b"template".as_slice())
    })
}

fn attr(node: Element, name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    southstar_dom::attrs::get(node, &name).map(|v| v.to_string_lossy().into_owned())
}

pub(crate) fn log_error(scope: &mut Scope<'_>, js: Js, exception: &Value, callback: &str) {
    if exception.is_null() || exception.is_undefined() || !ffi::log_enabled(js) {
        return;
    }
    let Ok(message) = scope.to_string(exception) else {
        return;
    };
    let stack = get(scope, exception, "stack");
    let stack = if stack.is_undefined() || stack.is_null() {
        None
    } else {
        scope.to_string(&stack).ok()
    };
    let line = match stack {
        Some(stack) => format!("custom element {callback} threw: {message}\n{stack}"),
        None => format!("custom element {callback} threw: {message}"),
    };
    ffi::log_line(js, &line);
}

fn call_callback(
    scope: &mut Scope<'_>,
    js: Js,
    element: &Value,
    class: &Value,
    callback: &str,
    args: &[Value],
) {
    let proto = get(scope, class, "prototype");
    if !proto.is_object() {
        return;
    }
    let function = get(scope, &proto, callback);
    if scope.is_function(&function)
        && let Err(error) = scope.call(&function, element, args)
    {
        log_error(scope, js, &error, callback);
    }
}

pub(crate) fn observed_attributes(scope: &mut Scope<'_>, class: &Value) -> Value {
    let cached = get(scope, class, "__nd_ce_observed");
    if cached.is_array() {
        return cached;
    }
    get(scope, class, "observedAttributes")
}

fn observed_names(scope: &mut Scope<'_>, class: &Value) -> Option<Vec<Value>> {
    let observed = observed_attributes(scope, class);
    if !observed.is_array() {
        return None;
    }
    let length = get(scope, &observed, "length");
    let length = scope.to_int32(&length).unwrap_or(0);
    Some(
        (0..length.max(0) as u32)
            .map(|i| {
                scope
                    .get_index(&observed, i)
                    .unwrap_or_else(|_| Value::undefined())
            })
            .collect(),
    )
}

fn fire_connected_if_needed(
    scope: &mut Scope<'_>,
    js: Js,
    node: Element,
    element: &Value,
    class: &Value,
) {
    if !ffi::node_in_page(js, node) || truthy(scope, element, "__nd_ce_connected") {
        return;
    }
    let _ = scope.define(
        element,
        "__nd_ce_connected",
        Value::boolean(true),
        WRITABLE_CONFIGURABLE,
    );
    call_callback(scope, js, element, class, "connectedCallback", &[]);
}

fn setter_shadows(scope: &mut Scope<'_>, element: &Value, key: &Value) -> bool {
    let mut proto = scope
        .get_prototype(element)
        .unwrap_or_else(|_| Value::undefined());
    for _ in 0..PROTOTYPE_DEPTH {
        if !proto.is_object() {
            break;
        }
        if let Ok(Some(descriptor)) = scope.own_property(&proto, key) {
            return descriptor.accessor && scope.is_function(&descriptor.setter);
        }
        proto = scope
            .get_prototype(&proto)
            .unwrap_or_else(|_| Value::undefined());
    }
    false
}

fn reclaim_shadowed_props(scope: &mut Scope<'_>, element: &Value) {
    let Ok(keys) = scope.own_enumerable_keys(element) else {
        return;
    };
    for key in keys {
        let Ok(name) = scope.to_string(&key) else {
            continue;
        };
        if name.starts_with('_') {
            continue;
        }
        let value = match scope.own_property(element, &key) {
            Ok(Some(descriptor)) if !descriptor.accessor => descriptor.value,
            _ => continue,
        };
        if setter_shadows(scope, element, &key) {
            let _ = scope.delete(element, &name);
            let _ = scope.set(element, &name, value);
        }
    }
}

fn upgrade_with(
    scope: &mut Scope<'_>,
    js: Js,
    page: &Page,
    node: Element,
    class: &Value,
    depth: i32,
) {
    let id = node.as_ptr() as usize;
    if page.under_construction.borrow().contains(&id) {
        return;
    }
    let element = ffi::wrap_node(scope, node);
    if !element.is_object() {
        return;
    }
    let marker = get(scope, &element, "__nd_ce_class");
    if marker.same_object(class) {
        fire_connected_if_needed(scope, js, node, &element, class);
        return;
    }
    let proto = get(scope, class, "prototype");
    if proto.is_object() {
        let _ = scope.set_prototype(&element, &proto);
    }
    let _ = scope.define(
        &element,
        "__nd_ce_class",
        class.clone(),
        WRITABLE_CONFIGURABLE,
    );
    page.under_construction.borrow_mut().insert(id);

    let previous = page.upgrading.replace(Some(element.clone()));
    let constructed = scope.construct(class, &[]);
    let replaced = page.upgrading.replace(previous);
    drop(replaced);
    if let Err(error) = constructed {
        log_error(scope, js, &error, "constructor");
    }

    reclaim_shadowed_props(scope, &element);

    if let Some(names) = observed_names(scope, class) {
        page.in_attr_callback.set(page.in_attr_callback.get() + 1);
        for name in names {
            let Ok(attr_name) = scope.to_string(&name) else {
                continue;
            };
            if let Some(current) = attr(node, &attr_name) {
                let value = scope.string(&current);
                call_callback(
                    scope,
                    js,
                    &element,
                    class,
                    "attributeChangedCallback",
                    &[name, Value::null(), value],
                );
            }
        }
        page.in_attr_callback.set(page.in_attr_callback.get() - 1);
    }

    page.under_construction.borrow_mut().remove(&id);

    fire_connected_if_needed(scope, js, node, &element, class);

    for child in southstar_dom::children(node) {
        if ffi::node_in_page(js, child) {
            upgrade_all_rec(scope, js, page, child, depth + 1);
        }
    }
}

pub(crate) fn class_for_node(scope: &mut Scope<'_>, js: Js, node: Element) -> Option<Value> {
    let page = crate::existing_page(js)?;
    if page.is_empty() {
        return None;
    }
    let tag = node.element_name()?;
    let autonomous = tag.contains(&b'-');
    let name = if autonomous {
        String::from_utf8_lossy(tag).into_owned()
    } else {
        attr(node, "is")?
    };
    if name.is_empty() {
        return None;
    }
    let document = node_document(scope, js, node);
    let key = key_for(js, &name.to_ascii_lowercase(), document);
    let class = page.lookup(&key)?;
    let extends = get(scope, &class, "__nd_ce_extends");
    let matches = if autonomous {
        !extends.is_string()
    } else if extends.is_string() {
        scope
            .to_string(&extends)
            .is_ok_and(|e| e.as_bytes().eq_ignore_ascii_case(tag))
    } else {
        false
    };
    let still = page.lookup(&key).is_some_and(|c| c.same_object(&class));
    (matches && still).then_some(class)
}

fn upgrade_named_rec(
    scope: &mut Scope<'_>,
    js: Js,
    page: &Page,
    root: Element,
    target: &Key,
    depth: i32,
) {
    if depth >= MAX_DEPTH || in_template_content(root) {
        return;
    }
    if root.element_name().is_some() {
        let wanted = page.lookup(target);
        let class = class_for_node(scope, js, root);
        if let (Some(wanted), Some(class)) = (wanted, class)
            && wanted.same_object(&class)
        {
            upgrade_with(scope, js, page, root, &class, depth);
        }
    }
    for child in southstar_dom::children(root) {
        upgrade_named_rec(scope, js, page, child, target, depth + 1);
    }
}

pub(crate) fn upgrade_subtree_named(js: Js, page: &Page, root: Element, target: &Key) {
    if !ffi::node_in_page(js, root) {
        return;
    }
    ffi::main_scope(js, |scope| {
        upgrade_named_rec(scope, js, page, root, target, 0)
    });
}

fn upgrade_all_rec(scope: &mut Scope<'_>, js: Js, page: &Page, root: Element, depth: i32) {
    if depth >= MAX_DEPTH || page.is_empty() || in_template_content(root) {
        return;
    }
    if root.element_name().is_some()
        && let Some(class) = class_for_node(scope, js, root)
    {
        upgrade_with(scope, js, page, root, &class, depth);
    }
    for child in southstar_dom::children(root) {
        upgrade_all_rec(scope, js, page, child, depth + 1);
    }
}

pub(crate) fn upgrade_root(js: Js, root: Element) {
    let Some(page) = crate::existing_page(js) else {
        return;
    };
    ffi::main_scope(js, |scope| upgrade_all_rec(scope, js, &page, root, 0));
}

pub(crate) fn upgrade_subtree_all(js: Js, root: Element) {
    if ffi::node_in_page(js, root) {
        upgrade_root(js, root);
    }
}

pub(crate) fn upgrade_subtree_detached(js: Js, root: Element) {
    upgrade_root(js, root);
}

pub(crate) fn upgrade_element(js: Js, node: Element) {
    let Some(page) = crate::existing_page(js) else {
        return;
    };
    ffi::main_scope(js, |scope| {
        if let Some(class) = class_for_node(scope, js, node) {
            upgrade_with(scope, js, &page, node, &class, 0);
        }
    });
}

fn disconnect_rec(scope: &mut Scope<'_>, js: Js, root: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if root.is_element() {
        ffi::popover_removing(js, root);
        let element = ffi::node_wrapper(scope, root);
        if element.is_object() && truthy(scope, &element, "__nd_ce_connected") {
            let class = get(scope, &element, "__nd_ce_class");
            let _ = scope.define(
                &element,
                "__nd_ce_connected",
                Value::boolean(false),
                WRITABLE_CONFIGURABLE,
            );
            if class.is_object() {
                call_callback(scope, js, &element, &class, "disconnectedCallback", &[]);
            }
        }
    }
    for child in southstar_dom::children(root) {
        disconnect_rec(scope, js, child, depth + 1);
    }
}

pub(crate) fn disconnect_subtree(js: Js, root: Element) {
    ffi::main_scope(js, |scope| disconnect_rec(scope, js, root, 0));
}

pub(crate) fn attribute_changed(
    js: Js,
    node: Node<'static>,
    attr_name: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
) {
    let Some(page) = crate::existing_page(js) else {
        return;
    };
    if page.in_attr_callback.get() != 0 || !ffi::wrapper_pinned(js, node) {
        return;
    }
    ffi::main_scope(js, |scope| {
        let element = ffi::node_wrapper(scope, node);
        if !element.is_object() {
            return;
        }
        let class = get(scope, &element, "__nd_ce_class");
        if !class.is_object() {
            return;
        }
        let watched = observed_names(scope, &class).is_some_and(|names| {
            names.iter().any(|name| {
                scope
                    .to_string(name)
                    .is_ok_and(|n| n.eq_ignore_ascii_case(attr_name))
            })
        });
        if !watched {
            return;
        }
        page.in_attr_callback.set(page.in_attr_callback.get() + 1);
        let name = scope.string(attr_name);
        let old = old_value.map_or_else(Value::null, |v| scope.string(v));
        let new = new_value.map_or_else(Value::null, |v| scope.string(v));
        call_callback(
            scope,
            js,
            &element,
            &class,
            "attributeChangedCallback",
            &[name, old, new],
        );
        page.in_attr_callback.set(page.in_attr_callback.get() - 1);
    });
}
