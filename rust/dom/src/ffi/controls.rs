//! Southstar — the C ABI of the form-control helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};

use super::node;
use crate::Node;
use crate::NsNode;
use crate::controls::{self};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_used(n: *const NsNode) -> GBoolean {
    // SAFETY: the C callers pass null or a node of this thread's store.
    glib::boolean(unsafe { node(n) }.is_none_or(controls::spellcheck_used))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_host(n: *const NsNode) -> *const NsNode {
    // SAFETY: the C callers pass null or a node of this thread's store.
    Node::ptr_or_null(unsafe { node(n) }.and_then(controls::spellcheck_host))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_is_one_line_text(n: *const NsNode) -> GBoolean {
    // SAFETY: the C callers pass null or a node of this thread's store.
    glib::boolean(unsafe { node(n) }.is_some_and(controls::is_one_line_text))
}
