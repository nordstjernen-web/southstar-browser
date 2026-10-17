//! Southstar — sweeping orphaned subtrees no script can reach, releasing a subtree's pinned wrappers and loading the frames that waited for a quiet moment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

use southstar_dom::{Node, NsNode};

use crate::ffi::glue;
use crate::ffi::{self, Js};

const SWEEP_INTERVAL_US: i64 = 250_000;

pub(crate) fn sweep(js: Js) {
    if glue::orphan_count(js) == 0 {
        return;
    }
    let state = js.st();
    if state.eval_depth.get() > 0
        || state.callback_depth.get() > 0
        || state.dispatch_depth.get() > 0
    {
        return;
    }
    let now = ffi::monotonic_us();
    let last = state.last_orphan_sweep_us.get();
    if last != 0 && now - last < SWEEP_INTERVAL_US {
        return;
    }
    state.last_orphan_sweep_us.set(now);
    glue::run_gc(js);
    for root in glue::collect_orphans(js) {
        glue::free_orphan_subtree(js, root);
    }
}

pub(crate) fn unpin_subtree(js: Js, root: Node<'static>) {
    if js.pinned_set().is_null() {
        return;
    }
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut child = node.first_child();
        while let Some(current) = child {
            stack.push(current);
            child = current.next_sibling();
        }
        if node.has_js_wrapper() {
            glue::unpin(js, node.as_mut_ptr());
        }
    }
}

pub(crate) fn process_pending_iframes(js: Js) {
    if js.halted() || glue::pending_iframe_count(js) == 0 {
        return;
    }
    let state = js.st();
    if state.iframe_load_depth.get() > 0 || state.eval_depth.get() > 0 {
        return;
    }
    let mut loaded: HashSet<*const NsNode> = HashSet::new();
    while !js.halted() && glue::pending_iframe_count(js) > 0 {
        let iframe = glue::pending_iframe_first(js);
        if !loaded.insert(Node::ptr_or_null(iframe)) {
            break;
        }
        glue::load_first_pending_iframe(js, iframe);
    }
}
