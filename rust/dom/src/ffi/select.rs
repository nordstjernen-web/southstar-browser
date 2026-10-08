//! Southstar — the C ABI of the option and select helpers declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_glib as glib;

use super::{Node, NsNode};
use crate::select;

fn dup_or_empty(n: *const NsNode, text: fn(Node) -> Vec<u8>) -> *mut c_char {
    glib::strdup(&unsafe { Node::from_ptr(n) }.map(text).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_option_text_dup(option: *const NsNode) -> *mut c_char {
    dup_or_empty(option, select::option_text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_option_label_dup(option: *const NsNode) -> *mut c_char {
    dup_or_empty(option, select::option_label)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_option_value_dup(option: *const NsNode) -> *mut c_char {
    dup_or_empty(option, select::option_value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_select_first_selected_option(select: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { Node::from_ptr(select) }.and_then(select::first_selected_option))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_select_chosen_option(select: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { Node::from_ptr(select) }.and_then(select::chosen_option))
}
