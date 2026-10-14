//! Southstar — the C ABI of the form-control helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode};
use crate::controls::{self};

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_used(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_none_or(controls::spellcheck_used))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_host(n: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { node(n) }.and_then(controls::spellcheck_host))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_is_one_line_text(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_some_and(controls::is_one_line_text))
}
