//! Southstar — the ChildNode and ParentNode methods over node-or-string arguments: before, after, replaceWith, remove, append, prepend and replaceChildren.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, children};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::validity;
use crate::{
    Element, JsResult, detach_recorded, is_detached_document, is_document, nodes_inserted,
    record_fragment_emptied, record_move_removal, within,
};

fn text_node(scope: &mut Scope<'_>, value: &Value) -> Option<Element> {
    let mut bytes = scope.to_bytes(value).ok()?;
    if let Some(nul) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(nul);
    }
    Some(ffi::new_text(&bytes))
}

fn take_node(js: Option<Js>, node: Element, sequence: &mut Vec<Element>) {
    if is_detached_document(node) {
        if let Some(js) = js {
            record_fragment_emptied(js, node);
        }
        let mut cursor = node.first_child();
        while let Some(c) = cursor {
            cursor = c.next_sibling();
            c.detach();
            if let Some(js) = js {
                ffi::unorphan(js, c);
            }
            sequence.push(c);
        }
        return;
    }
    if node.parent().is_some() {
        if let Some(js) = js {
            ffi::iters_pre_remove(js, node);
            ffi::ce_disconnect_subtree(js, node);
            record_move_removal(js, node);
        }
        node.detach();
    }
    if let Some(js) = js {
        ffi::unorphan(js, node);
    }
    sequence.push(node);
}

fn convert(
    scope: &mut Scope<'_>,
    js: Option<Js>,
    args: &[Value],
    skip: impl Fn(Element) -> bool,
) -> Vec<Element> {
    let mut sequence = Vec::new();
    for arg in args {
        match ffi::unwrap_node(arg) {
            Some(node) if skip(node) => {}
            Some(node) => take_node(js, node, &mut sequence),
            None => sequence.extend(text_node(scope, arg)),
        }
    }
    sequence
}

fn args_contain(args: &[Value], node: Element) -> bool {
    args.iter().any(|arg| ffi::unwrap_node(arg) == Some(node))
}

fn record_inserted(js: Option<Js>, parent: Element, node: Element) {
    if let Some(js) = js {
        ffi::record_child_change(
            js,
            parent,
            Some(node),
            None,
            node.prev_sibling(),
            node.next_sibling(),
        );
    }
}

fn insert_all(js: Option<Js>, parent: Element, sequence: &[Element], anchor: Option<Element>) {
    for &node in sequence {
        if within(parent, node) {
            continue;
        }
        match anchor.filter(|a| a.parent() == Some(parent)) {
            Some(anchor) => ffi::insert_sibling_before(anchor, node),
            None => parent.append(node),
        }
        record_inserted(js, parent, node);
    }
}

fn finish(js: Option<Js>, parent: Element, sequence: &[Element]) {
    if let Some(js) = js {
        ffi::mark_mutated(js);
        nodes_inserted(js, parent, sequence);
    }
}

pub(crate) fn before(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(this_node) = ffi::unwrap_node(this).filter(|n| n.parent().is_some()) else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    let sequence = convert(scope, js, args, |node| {
        node == this_node || this_node.parent().is_some_and(|p| within(p, node))
    });
    let Some(parent) = this_node.parent() else {
        return Ok(Value::undefined());
    };
    for &node in &sequence {
        if this_node.parent().is_some_and(|p| within(p, node)) {
            continue;
        }
        ffi::insert_sibling_before(this_node, node);
        record_inserted(js, parent, node);
    }
    finish(js, parent, &sequence);
    Ok(Value::undefined())
}

pub(crate) fn after(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(this_node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let Some(parent) = this_node.parent() else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    let viable_next = core::iter::successors(this_node.next_sibling(), |n| n.next_sibling())
        .find(|&n| !args_contain(args, n));
    let sequence = convert(scope, js, args, |node| {
        within(parent, node) && node != this_node
    });
    insert_all(js, parent, &sequence, viable_next);
    finish(js, parent, &sequence);
    Ok(Value::undefined())
}

pub(crate) fn replace_with(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(this_node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let Some(parent) = this_node.parent() else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    let self_in_args = args_contain(args, this_node);
    let viable_prev = core::iter::successors(this_node.prev_sibling(), |n| n.prev_sibling())
        .find(|&n| !args_contain(args, n));
    let sequence = convert(scope, js, args, |node| {
        within(parent, node) && node != this_node
    });
    let anchor = if self_in_args {
        viable_prev.map_or(parent.first_child(), Node::next_sibling)
    } else {
        Some(this_node)
    };
    insert_all(js, parent, &sequence, anchor);
    if !self_in_args {
        detach_recorded(js, this_node);
    }
    finish(js, parent, &sequence);
    Ok(Value::undefined())
}

fn select_option_at(select: Element, index: i32) -> Option<Element> {
    let mut i = 0;
    for c in children(select) {
        if c.element_name() == Some(b"option") {
            if i == index {
                return Some(c);
            }
            i += 1;
        } else if c.element_name() == Some(b"optgroup") {
            for option in children(c).filter(|cc| cc.element_name() == Some(b"option")) {
                if i == index {
                    return Some(option);
                }
                i += 1;
            }
        }
    }
    None
}

pub(crate) fn remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let node = ffi::unwrap_node(this);
    let js = ffi::js_of(scope);
    if let Some(select) = node.filter(|n| n.element_name() == Some(b"select"))
        && args.first().is_some_and(|a| !a.is_undefined())
    {
        let index = scope.to_int32(&args[0])?;
        if index >= 0
            && let Some(target) = select_option_at(select, index)
        {
            detach_recorded(js, target);
            if let Some(js) = js {
                ffi::mark_mutated(js);
            }
        }
        return Ok(Value::undefined());
    }
    let Some(node) = node.filter(|n| n.parent().is_some()) else {
        return Ok(Value::undefined());
    };
    detach_recorded(js, node);
    if let Some(js) = js {
        ffi::mark_mutated(js);
    }
    Ok(Value::undefined())
}

pub(crate) fn append(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    validity::document_batch(scope, parent, args)?;
    for child in args.iter().filter_map(ffi::unwrap_node) {
        validity::pre_insert(scope, parent, child, None, true)?;
    }
    let sequence = convert(scope, js, args, |node| within(parent, node));
    insert_all(js, parent, &sequence, None);
    finish(js, parent, &sequence);
    Ok(Value::undefined())
}

pub(crate) fn prepend(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    validity::document_batch(scope, parent, args)?;
    for child in args.iter().filter_map(ffi::unwrap_node) {
        let first = parent.first_child();
        validity::pre_insert(scope, parent, child, first, first.is_none())?;
    }
    let sequence = convert(scope, js, args, |node| within(parent, node));
    insert_all(js, parent, &sequence, parent.first_child());
    finish(js, parent, &sequence);
    Ok(Value::undefined())
}

fn take_replacement(js: Option<Js>, parent: Element, node: Element, added: &mut Vec<Element>) {
    if is_detached_document(node) {
        if let Some(js) = js {
            record_fragment_emptied(js, node);
        }
        let mut cursor = node.first_child();
        while let Some(c) = cursor {
            cursor = c.next_sibling();
            if let Some(js) = js {
                ffi::ce_disconnect_subtree(js, c);
            }
            c.detach();
            if let Some(js) = js {
                ffi::unorphan(js, c);
            }
            added.push(c);
        }
        return;
    }
    if let Some(js) = js {
        ffi::ce_disconnect_subtree(js, node);
        if node.parent() != Some(parent) {
            record_move_removal(js, node);
        }
        ffi::unorphan(js, node);
    }
    node.detach();
    added.push(node);
}

pub(crate) fn replace_children(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    if is_document(parent) {
        validity::document_arguments(scope, args)?;
    } else {
        for child in args.iter().filter_map(ffi::unwrap_node) {
            validity::pre_insert(scope, parent, child, None, true)?;
        }
    }
    let original: Vec<Element> = children(parent).collect();
    let mut added = Vec::new();
    for arg in args {
        match ffi::unwrap_node(arg) {
            Some(child) if within(parent, child) => {}
            Some(child) => take_replacement(js, parent, child, &mut added),
            None => added.extend(text_node(scope, arg)),
        }
    }
    while let Some(old) = parent.first_child() {
        match js {
            Some(js) => {
                ffi::ce_disconnect_subtree(js, old);
                old.detach();
                ffi::orphan(js, old);
                ffi::index_child_change(js, parent, None, Some(old));
            }
            None => old.detach(),
        }
    }
    for &node in &added {
        if within(parent, node) {
            continue;
        }
        if let Some(js) = js {
            ffi::unorphan(js, node);
        }
        parent.append(node);
        if let Some(js) = js {
            ffi::index_child_change(js, parent, Some(node), None);
        }
    }
    if let Some(js) = js {
        ffi::mark_childlist_dirty(parent, None);
        if !added.is_empty() || !original.is_empty() {
            ffi::emit_child_list(js, parent, &added, &original, None, None);
        }
        ffi::mark_mutated(js);
        nodes_inserted(js, parent, &added);
    }
    Ok(Value::undefined())
}
