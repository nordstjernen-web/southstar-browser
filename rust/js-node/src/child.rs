//! Southstar — the Node mutation methods: appendChild, insertBefore, removeChild, replaceChild and moveBefore.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, children};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::validity;
use crate::{
    Element, JsResult, detach_recorded, hierarchy_error, is_detached_document, is_document,
    nodes_inserted, not_found_error, record_fragment_emptied, record_move_removal, within,
};

const ANCESTOR: &str = "the new child is an inclusive ancestor of the parent";

fn place(js: Option<Js>, parent: Element, node: Element, reference: Option<Element>) {
    match reference.filter(|r| r.parent() == Some(parent)) {
        Some(reference) => ffi::insert_before_single(js, parent, node, reference),
        None => {
            if let Some(js) = js {
                ffi::unorphan(js, node);
            }
            node.detach();
            parent.append(node);
        }
    }
}

fn move_fragment_children(
    js: Option<Js>,
    parent: Element,
    fragment: Element,
    reference: Option<Element>,
) -> Vec<Element> {
    if let Some(js) = js {
        record_fragment_emptied(js, fragment);
    }
    let mut moved = Vec::new();
    let mut cursor = fragment.first_child();
    while let Some(c) = cursor {
        cursor = c.next_sibling();
        if let Some(js) = js {
            ffi::ce_disconnect_subtree(js, c);
        }
        if within(parent, c) {
            continue;
        }
        c.detach();
        place(js, parent, c, reference);
        if let Some(js) = js {
            ffi::index_child_change(js, parent, Some(c), None);
        }
        moved.push(c);
    }
    moved
}

fn emit_moved(
    js: Js,
    parent: Element,
    moved: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    if let Some(&first) = moved.first() {
        ffi::mark_childlist_dirty(parent, Some(first));
        ffi::emit_child_list(js, parent, moved, &[], previous, next);
    }
}

fn inserted(js: Js, parent: Element, root: Element) {
    if !ffi::in_template_content(parent) {
        ffi::run_inserted_scripts(js, root);
        ffi::ce_upgrade_subtree_all(js, root);
    }
}

fn node_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> JsResult<Element> {
    ffi::unwrap_node(&args[index])
        .ok_or_else(|| scope.type_error(&format!("Argument {} is not an object / Node", index + 1)))
}

fn reference_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<Option<Element>> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    ffi::unwrap_node(value)
        .map(Some)
        .ok_or_else(|| scope.type_error("Argument 2 is not an object / Node"))
}

fn require_two(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<()> {
    if args.len() < 2 {
        return Err(scope.type_error("2 arguments required, but fewer present"));
    }
    Ok(())
}

pub(crate) fn append_child(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if args.is_empty() {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    }
    let child = node_arg(scope, args, 0)?;
    validity::pre_insert(scope, parent, child, None, true)?;
    let js = ffi::js_of(scope);
    let inert_parent = ffi::in_template_content(parent);
    if is_detached_document(child) {
        let previous = parent.last_child();
        let moved = move_fragment_children(js, parent, child, None);
        if let Some(js) = js {
            emit_moved(js, parent, &moved, previous, None);
            ffi::mark_mutated(js);
            nodes_inserted(js, parent, &moved);
        }
        return Ok(args[0].clone());
    }
    if let Some(js) = js {
        if child.parent().is_some() {
            ffi::iters_pre_remove(js, child);
        }
        ffi::ce_disconnect_subtree(js, child);
        if within(parent, child) {
            return Err(hierarchy_error(scope, ANCESTOR));
        }
        record_move_removal(js, child);
        ffi::unorphan(js, child);
    }
    parent.append(child);
    if let Some(js) = js {
        ffi::mark_mutated(js);
        ffi::record_child_change(
            js,
            parent,
            Some(child),
            None,
            child.prev_sibling(),
            child.next_sibling(),
        );
        if !inert_parent {
            ffi::run_inserted_scripts(js, child);
            ffi::ce_upgrade_subtree_all(js, child);
        }
    }
    Ok(args[0].clone())
}

pub(crate) fn remove_child(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if args.is_empty() {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    }
    let child = node_arg(scope, args, 0)?;
    if child.parent() != Some(parent) {
        return Err(not_found_error(
            scope,
            "the node to be removed is not a child of this node",
        ));
    }
    let js = ffi::js_of(scope);
    detach_recorded(js, child);
    if let Some(js) = js {
        ffi::mark_mutated(js);
    }
    Ok(args[0].clone())
}

pub(crate) fn insert_before(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    require_two(scope, args)?;
    let node = node_arg(scope, args, 0)?;
    let child_is_null = args[1].is_null() || args[1].is_undefined();
    let reference = reference_arg(scope, &args[1])?;
    validity::pre_insert(scope, parent, node, reference, child_is_null)?;
    if Some(node) == reference {
        return Ok(args[0].clone());
    }
    let js = ffi::js_of(scope);
    let inert_parent = ffi::in_template_content(parent);
    if is_detached_document(node) {
        let has_reference = reference.is_some_and(|r| r.parent() == Some(parent));
        let (previous, next) = match reference {
            Some(r) if has_reference => (r.prev_sibling(), Some(r)),
            _ => (parent.last_child(), None),
        };
        let moved = move_fragment_children(js, parent, node, reference);
        if let Some(js) = js {
            emit_moved(js, parent, &moved, previous, next);
            ffi::mark_mutated(js);
            inserted(js, parent, parent);
        }
        return Ok(args[0].clone());
    }
    if let Some(js) = js {
        ffi::ce_disconnect_subtree(js, node);
        if within(parent, node) {
            return Err(hierarchy_error(scope, ANCESTOR));
        }
        record_move_removal(js, node);
    }
    place(js, parent, node, reference);
    if let Some(js) = js {
        ffi::mark_mutated(js);
        ffi::record_child_change(
            js,
            parent,
            Some(node),
            None,
            node.prev_sibling(),
            node.next_sibling(),
        );
        if !inert_parent {
            ffi::run_inserted_scripts(js, node);
            ffi::ce_upgrade_subtree_all(js, node);
        }
    }
    Ok(args[0].clone())
}

pub(crate) fn replace_child(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    require_two(scope, args)?;
    let (Some(new_child), Some(old_child)) =
        (ffi::unwrap_node(&args[0]), ffi::unwrap_node(&args[1]))
    else {
        return Err(scope.type_error("Argument is not an object / Node"));
    };
    validity::pre_replace(scope, parent, new_child, old_child)?;
    let js = ffi::js_of(scope);
    if new_child == old_child {
        if let Some(js) = js {
            let previous = old_child.prev_sibling();
            let next = old_child.next_sibling();
            ffi::record_child_change(js, parent, None, Some(old_child), previous, next);
            ffi::record_child_change(js, parent, Some(old_child), None, previous, next);
            ffi::mark_mutated(js);
        }
        return Ok(args[1].clone());
    }
    if let Some(js) = js {
        ffi::iters_pre_remove(js, old_child);
    }
    let inert_parent = ffi::in_template_content(parent);
    if is_detached_document(new_child) {
        if let Some(js) = js {
            record_fragment_emptied(js, new_child);
        }
        let reference = old_child.next_sibling();
        let previous = old_child.prev_sibling();
        old_child.detach();
        if let Some(js) = js {
            ffi::ce_disconnect_subtree(js, old_child);
            ffi::orphan(js, old_child);
            ffi::record_child_change(js, parent, None, Some(old_child), previous, reference);
        }
        let mut cursor = new_child.first_child();
        while let Some(c) = cursor {
            cursor = c.next_sibling();
            if let Some(js) = js {
                ffi::ce_disconnect_subtree(js, c);
            }
            if within(parent, c) {
                continue;
            }
            c.detach();
            place(js, parent, c, reference);
            if let Some(js) = js {
                ffi::record_child_change(
                    js,
                    parent,
                    Some(c),
                    None,
                    c.prev_sibling(),
                    c.next_sibling(),
                );
            }
        }
        if let Some(js) = js {
            ffi::mark_mutated(js);
            inserted(js, parent, parent);
        }
        return Ok(args[1].clone());
    }
    if let Some(js) = js {
        if new_child.parent().is_some() {
            ffi::iters_pre_remove(js, new_child);
        }
        ffi::ce_disconnect_subtree(js, new_child);
        validity::pre_replace(scope, parent, new_child, old_child)?;
        record_move_removal(js, new_child);
    }
    new_child.detach();
    if let Some(js) = js {
        ffi::unorphan(js, new_child);
    }
    ffi::insert_sibling_before(old_child, new_child);
    old_child.detach();
    if let Some(js) = js {
        ffi::ce_disconnect_subtree(js, old_child);
        ffi::orphan(js, old_child);
        ffi::mark_mutated(js);
        ffi::record_child_change(
            js,
            parent,
            Some(new_child),
            Some(old_child),
            new_child.prev_sibling(),
            new_child.next_sibling(),
        );
        if !inert_parent {
            ffi::run_inserted_scripts(js, new_child);
            ffi::ce_upgrade_subtree_all(js, new_child);
        }
    }
    Ok(args[1].clone())
}

pub(crate) fn move_before(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(parent) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    require_two(scope, args)?;
    let node = node_arg(scope, args, 0)?;
    let child = reference_arg(scope, &args[1])?;
    if node.root() != parent.root() {
        return Err(hierarchy_error(
            scope,
            "node and parent are not in the same tree",
        ));
    }
    if within(parent, node) {
        return Err(hierarchy_error(
            scope,
            "node is an inclusive ancestor of the parent",
        ));
    }
    if !matches!(node.kind(), Kind::Element | Kind::Text | Kind::Comment) {
        return Err(hierarchy_error(
            scope,
            "node is not an Element or CharacterData",
        ));
    }
    if child.is_some_and(|c| c.parent() != Some(parent)) {
        return Err(not_found_error(
            scope,
            "the reference child is not a child of this node",
        ));
    }
    if is_document(parent)
        && node.kind() == Kind::Element
        && children(parent).any(|c| c.kind() == Kind::Element && c != node)
    {
        return Err(hierarchy_error(
            scope,
            "document may have only one element child",
        ));
    }
    if node.parent().is_none() {
        return Err(hierarchy_error(scope, "node has no parent"));
    }
    let js = ffi::js_of(scope);
    let reference = if child == Some(node) {
        node.next_sibling()
    } else {
        child
    };
    if let Some(js) = js {
        ffi::iters_pre_remove(js, node);
        record_move_removal(js, node);
    }
    match reference.filter(|r| r.parent() == Some(parent)) {
        Some(reference) => ffi::insert_before_single(js, parent, node, reference),
        None => {
            node.detach();
            if let Some(js) = js {
                ffi::unorphan(js, node);
            }
            parent.append(node);
        }
    }
    if let Some(js) = js {
        ffi::mark_mutated(js);
        ffi::record_child_change(
            js,
            parent,
            Some(node),
            None,
            node.prev_sibling(),
            node.next_sibling(),
        );
    }
    Ok(Value::undefined())
}
