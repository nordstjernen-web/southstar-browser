//! Southstar — the C ABI of the option and select helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_glib as glib;

use super::node;
use crate::Node;
use crate::NsNode;
use crate::select;

/// # Safety
///
/// `n` is null or a node pointer this thread's store handed out.
unsafe fn dup_or_empty(n: *const NsNode, text: fn(Node) -> Vec<u8>) -> *mut c_char {
    // SAFETY: guaranteed by the caller.
    glib::strdup(&unsafe { node(n) }.map(text).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_option_label_dup(option: *const NsNode) -> *mut c_char {
    // SAFETY: the C callers pass null or a node of this thread's store.
    unsafe { dup_or_empty(option, select::option_label) }
}
