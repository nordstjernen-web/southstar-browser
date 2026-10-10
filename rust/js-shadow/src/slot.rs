//! Southstar — slot assignment: assignedNodes and assignedElements with flatten, and the assigned slot of a node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::VecDeque;

use southstar_dom::Kind;
use southstar_js_engine::{Scope, Value};

use crate::ffi;
use crate::{Element, JsResult, is_shadow_root, is_slot};

const MAX_FIRST_NAMED_DEPTH: i32 = 512;
const MAX_FLATTEN_DEPTH: i32 = 64;

fn name_of(node: Element, attr: &core::ffi::CStr) -> &'static [u8] {
    node.attr(attr).map_or(&b""[..], |name| name.to_bytes())
}

fn enclosing_root(slot: Element) -> Option<Element> {
    southstar_dom::ancestors_and_self(slot).find(|&n| is_shadow_root(Some(n)))
}

fn find_host(slot: Element) -> Option<Element> {
    enclosing_root(slot).and_then(Element::parent)
}

fn first_named(root: Element, name: &[u8], depth: i32) -> Option<Element> {
    if depth >= MAX_FIRST_NAMED_DEPTH {
        return None;
    }
    if is_slot(Some(root)) && name_of(root, c"name") == name {
        return Some(root);
    }
    southstar_dom::children(root)
        .filter(|&c| !is_shadow_root(Some(c)))
        .find_map(|c| first_named(c, name, depth + 1))
}

fn collect_direct(nodes: &mut Vec<Element>, slot: Element, elements_only: bool) {
    let Some(host) = find_host(slot) else {
        return;
    };
    let slot_name = name_of(slot, c"name");
    let Some(root) = enclosing_root(slot) else {
        return;
    };
    if first_named(root, slot_name, 0) != Some(slot) {
        return;
    }
    for child in southstar_dom::children(host) {
        if is_shadow_root(Some(child)) {
            continue;
        }
        let assigned = match child.kind() {
            Kind::Element => name_of(child, c"slot") == slot_name,
            Kind::Text => !elements_only && slot_name.is_empty(),
            _ => false,
        };
        if assigned {
            nodes.push(child);
        }
    }
}

fn collect_flattened(nodes: &mut Vec<Element>, slot: Element, elements_only: bool, depth: i32) {
    if depth >= MAX_FLATTEN_DEPTH || find_host(slot).is_none() {
        return;
    }
    let mut direct = Vec::new();
    collect_direct(&mut direct, slot, elements_only);
    if direct.is_empty() {
        direct.extend(southstar_dom::children(slot).filter(|c| match c.kind() {
            Kind::Element => true,
            Kind::Text => !elements_only,
            _ => false,
        }));
    }
    for node in direct {
        if is_slot(Some(node)) && find_host(node).is_some() {
            collect_flattened(nodes, node, elements_only, depth + 1);
        } else {
            nodes.push(node);
        }
    }
}

fn flatten_option(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<bool> {
    match args.first().filter(|v| v.is_object()) {
        Some(options) => {
            let flatten = scope.get(options, "flatten")?;
            Ok(scope.to_bool(&flatten))
        }
        None => Ok(false),
    }
}

fn assigned(scope: &mut Scope<'_>, this: &Value, args: &[Value], elements_only: bool) -> JsResult {
    let array = scope.new_array();
    let Some(slot) = ffi::unwrap_node(this).filter(|&n| is_slot(Some(n))) else {
        return Ok(array);
    };
    let mut nodes = Vec::new();
    if flatten_option(scope, args)? {
        collect_flattened(&mut nodes, slot, elements_only, 0);
    } else {
        collect_direct(&mut nodes, slot, elements_only);
    }
    for (index, node) in (0u32..).zip(nodes) {
        let wrapper = ffi::wrap_node(scope, node);
        scope.set_index(&array, index, wrapper)?;
    }
    Ok(array)
}

pub(crate) fn assigned_nodes(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    assigned(scope, this, args, false)
}

pub(crate) fn assigned_elements(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    assigned(scope, this, args, true)
}

pub(crate) fn assigned_slot_node(node: Element) -> Option<Element> {
    let root = crate::shadow_child(node.parent()?)?;
    let wanted = if node.kind() == Kind::Element {
        name_of(node, c"slot")
    } else {
        b""
    };
    let mut queue = VecDeque::from([root]);
    while let Some(candidate) = queue.pop_front() {
        if is_slot(Some(candidate)) && name_of(candidate, c"name") == wanted {
            return Some(candidate);
        }
        queue.extend(southstar_dom::children(candidate).filter(|c| c.kind() == Kind::Element));
    }
    None
}

pub(crate) fn get_assigned_slot(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let Some(root) = node.parent().and_then(crate::shadow_child) else {
        return Ok(Value::null());
    };
    if crate::is_closed(root) {
        return Ok(Value::null());
    }
    Ok(assigned_slot_node(node).map_or_else(Value::null, |slot| ffi::wrap_node(scope, slot)))
}
