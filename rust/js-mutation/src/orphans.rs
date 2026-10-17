//! Southstar — the orphan set: detached subtrees a page keeps alive until script can no longer reach them, and the child removals that feed it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{MAX_DEPTH, Node, children};

use crate::ffi::{self, Js};
use crate::{Element, key, record, tree, with_page};
use southstar_dom::WeakNode;

pub(crate) fn track(js: Js, node: Node<'_>) {
    with_page(js, |page| {
        if let Some(set) = page.orphans.as_mut() {
            set.insert(key(node));
        }
    });
}

pub(crate) fn untrack(js: Js, node: Element) {
    with_page(js, |page| {
        if let Some(set) = page.orphans.as_mut() {
            set.remove(&key(node));
        }
    });
}

pub(crate) fn tracked(js: Js, node: Element) -> bool {
    with_page(js, |page| {
        page.orphans
            .as_ref()
            .is_some_and(|set| set.contains(&key(node)))
    })
}

fn live(js: Js) -> bool {
    with_page(js, |page| page.orphans.is_some())
}

pub(crate) fn count(js: Js) -> usize {
    with_page(js, |page| page.orphans.as_ref().map_or(0, |set| set.len()))
}

pub(crate) fn orphan(js: Option<Js>, node: Element) {
    if let Some(js) = js
        && live(js)
    {
        track(js, node);
        return;
    }
    if let Some(js) = js {
        ffi::purge_subtree_rafs(js, node);
    }
    node.free_tree();
}

pub(crate) fn clear_children(js: Option<Js>, parent: Element) {
    let Some(js) = js else {
        tree::clear_children(parent);
        return;
    };
    for kid in children(parent).collect::<Vec<_>>() {
        kid.detach();
        record::index_child_changes(Some(js), parent, &[], &[kid]);
        track(js, kid);
    }
}

fn prune(js: Js, node: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    ffi::images_forget_node(js, node);
    ffi::dispatch_forget_node(js, node);
    for kid in children(node).collect::<Vec<_>>() {
        prune(js, kid, depth + 1);
    }
}

pub(crate) fn orphan_children(js: Option<Js>, parent: Element) {
    let Some(js) = js else {
        tree::clear_children(parent);
        return;
    };
    for kid in children(parent).collect::<Vec<_>>() {
        prune(js, kid, 0);
        kid.detach();
        record::index_child_changes(Some(js), parent, &[], &[kid]);
        orphan(Some(js), kid);
    }
}

pub(crate) fn replace_all(js: Option<Js>, parent: Element, added: Option<Element>) {
    let Some(js) = js else {
        tree::clear_children(parent);
        if let Some(added) = added {
            parent.append(added);
        }
        return;
    };
    let mut removed = Vec::new();
    while let Some(kid) = parent.first_child() {
        ffi::ce_disconnect_subtree(js, kid);
        kid.detach();
        track(js, kid);
        record::index_child_changes(Some(js), parent, &[], &[kid]);
        removed.push(kid);
    }
    if let Some(node) = added {
        parent.append(node);
        record::index_child_changes(Some(js), parent, added.as_slice(), &[]);
    }
    ffi::mark_childlist_dirty(parent, added);
    ffi::emit_child_list(
        Some(js),
        parent,
        added.as_ref().map(core::slice::from_ref),
        &removed,
        None,
        None,
    );
}

pub(crate) fn has_wrapper(root: Element) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.has_js_wrapper() {
            return true;
        }
        stack.extend(n.tpl_content());
        stack.extend(children(n));
    }
    false
}

pub(crate) fn purge_node_refs(js: Js, root: Element) {
    ffi::purge_media_rafs(js, root);
    ffi::focus_forget_subtree(js, root);
}

pub(crate) fn collect_unreachable(js: Js) -> Option<Vec<Element>> {
    let roots: Vec<Element> = with_page(js, |page| {
        page.orphans.as_mut().map(|set| {
            set.retain(|k| k.get().is_some());
            set.iter().filter_map(|k| k.get()).collect()
        })
    })?;
    let unreachable: Vec<Element> = roots
        .into_iter()
        .filter(|r| r.parent().is_none() && !has_wrapper(*r))
        .collect();
    for &r in &unreachable {
        untrack(js, r);
    }
    Some(unreachable)
}

pub(crate) fn take_all(js: Js) -> Vec<Element> {
    with_page(js, |page| {
        page.orphans
            .as_mut()
            .map(|set| set.drain().filter_map(WeakNode::get).collect())
            .unwrap_or_default()
    })
}

pub(crate) fn snapshot(js: Js) -> Option<Vec<Element>> {
    with_page(js, |page| {
        page.orphans
            .as_ref()
            .map(|set| set.iter().filter_map(|k| k.get()).collect())
    })
}

pub(crate) fn destroy(js: Js) {
    crate::remove_page(js);
}
