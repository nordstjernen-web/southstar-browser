//! Southstar — the DOM's C ABI for the crates that still reach it by symbol.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use crate::{Node, NsNode};

mod attrs;
mod controls;
mod index;
mod node;
mod select;
mod serialize;
mod tree;

/// # Safety
///
/// `n` is null or a node pointer this thread's store handed out.
unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    // SAFETY: guaranteed by the caller.
    unsafe { Node::from_ptr(n) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: the C callers pass null or a NUL-terminated string that outlives the call.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}
