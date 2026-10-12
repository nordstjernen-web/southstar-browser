//! Southstar — freeing a realm: every per-page state's teardown in order, the pinned wrappers, the orphaned subtrees and the engine.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::Element;
use crate::ffi::Js;

pub(crate) fn free(js: Js) {
    js.free_doc_ready_states();
    js.teardown_document_states();
    js.free_page_strings();
    js.teardown_page_services();
    js.stop_observer_tick();
    js.teardown_loaders_and_attributes();
    release_pinned_wrappers(js);
    js.free_pinned_set();
    free_orphans(js);
    js.teardown_input_states();
    js.free_realm_values();
    js.teardown_registries();
    js.free_partition();
    js.teardown_style_states();
    if js.is_active() {
        js.clear_active();
    }
    js.teardown_windows();
    js.free_window_values();
    js.teardown_dispatch();
    js.free_engine();
}

pub(crate) fn release_pinned_wrappers(js: Js) {
    for node in js.take_pinned_wrappers() {
        js.detach_wrapper(node, true);
    }
}

fn owned_by_document(js: Js, node: Element) -> bool {
    let document = js.current_document();
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if Some(current.as_ptr()) == document || js.orphan_tracked(current) {
            return true;
        }
        ancestor = current.parent();
    }
    false
}

fn free_orphans(js: Js) {
    let Some(orphans) = js.orphans_snapshot() else {
        return;
    };
    let mut roots = Vec::new();
    for node in orphans.into_iter().flatten() {
        js.detach_wrapper(node, false);
        if !owned_by_document(js, node) {
            roots.push(node);
        }
    }
    js.teardown_mutation();
    for root in roots {
        js.free_node(root);
    }
}
