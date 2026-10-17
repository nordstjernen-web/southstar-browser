//! Southstar — unrecorded tree edits: freeing an element's children and marking parsed scripts as already started.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{MAX_DEPTH, children};

use crate::{Element, record};

const SCRIPT_ALREADY_STARTED: &core::ffi::CStr = c"data-nd-script-already-started";

pub(crate) fn clear_children(parent: Element) {
    let doc = parent.root();
    for kid in children(parent).collect::<Vec<_>>() {
        kid.detach();
        if doc != kid {
            record::index_removed(doc, kid);
        }
        kid.free_tree();
    }
}

pub(crate) fn mark_scripts_already_started(root: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if root.element_name() == Some(b"script") {
        crate::ffi::set_attr(root, SCRIPT_ALREADY_STARTED, c"1");
    }
    for kid in children(root).collect::<Vec<_>>() {
        mark_scripts_already_started(kid, depth + 1);
    }
}
