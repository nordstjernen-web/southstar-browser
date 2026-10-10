//! Southstar — the tree walks that find a subtree's scripts to run, its unstarted scripts and its stylesheet links still to load.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{MAX_DEPTH, Node};

use crate::ffi::{hidden_child, is_named};
use crate::{
    ALREADY_STARTED, Task, link_is_loadable_stylesheet, schedule_for, skipped_by_nomodule,
    type_supported,
};

fn skip(n: Node, depth: i32) -> bool {
    depth >= MAX_DEPTH || (depth > 0 && hidden_child(n))
}

pub(crate) fn mark_scripts_already_started(root: Node) {
    fn walk(n: Node, depth: i32) {
        if skip(n, depth) {
            return;
        }
        if is_named(n, c"script") {
            crate::mark(n, ALREADY_STARTED);
            return;
        }
        for c in southstar_dom::children(n) {
            walk(c, depth + 1);
        }
    }
    walk(root, 0);
}

pub(crate) fn collect_script_tasks(root: Node<'_>) -> Vec<Task<'_>> {
    fn walk<'a>(n: Node<'a>, out: &mut Vec<Task<'a>>, depth: i32) {
        if skip(n, depth) {
            return;
        }
        if is_named(n, c"script") {
            if n.attr(ALREADY_STARTED).is_some() {
                return;
            }
            if !type_supported(n) || skipped_by_nomodule(n) {
                crate::mark(n, ALREADY_STARTED);
                return;
            }
            out.push(Task {
                node: n,
                schedule: schedule_for(n),
            });
            return;
        }
        if is_named(n, c"template") {
            return;
        }
        for c in southstar_dom::children(n) {
            walk(c, out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, 0);
    out
}

pub(crate) fn has_pending_script(root: Node) -> bool {
    fn walk(n: Node, depth: i32) -> bool {
        if skip(n, depth) {
            return false;
        }
        if is_named(n, c"script") && n.attr(ALREADY_STARTED).is_none() {
            return true;
        }
        if is_named(n, c"template") {
            return false;
        }
        southstar_dom::children(n).any(|c| walk(c, depth + 1))
    }
    walk(root, 0)
}

pub(crate) fn pending_stylesheets(root: Node<'_>) -> Vec<Node<'_>> {
    fn walk<'a>(n: Node<'a>, out: &mut Vec<Node<'a>>, depth: i32) {
        if skip(n, depth) {
            return;
        }
        if link_is_loadable_stylesheet(n) {
            out.push(n);
            return;
        }
        if is_named(n, c"template") {
            return;
        }
        for c in southstar_dom::children(n) {
            walk(c, out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, 0);
    out
}
