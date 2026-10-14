//! Southstar — the C ABI of text collection, serialization, the debug dump and image maps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode};
use crate::serialize::{self};

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_embedded_doc(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_some_and(serialize::is_embedded_doc))
}
