//! Southstar — restyle invalidation: the marks a DOM mutation leaves for the next style pass, as the C ABI the bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use crate::restyle;

unsafe fn bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mark_restyle_dirty(parent: *const NsNode) {
    if let Some(parent) = unsafe { Node::from_ptr(parent) } {
        restyle::mark_restyle_dirty(parent);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mark_childlist_dirty(parent: *const NsNode, added: *const NsNode) {
    if let Some(parent) = unsafe { Node::from_ptr(parent) } {
        restyle::mark_childlist_dirty(parent, unsafe { Node::from_ptr(added) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mark_attr_dirty(
    target: *const NsNode,
    name: *const c_char,
    old_value: *const c_char,
) {
    if let Some(target) = unsafe { Node::from_ptr(target) } {
        restyle::mark_attr_dirty(target, unsafe { bytes(name) }, unsafe { bytes(old_value) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_attr_may_affect_style(
    _target: *const NsNode,
    name: *const c_char,
) -> GBoolean {
    glib::boolean(restyle::attr_may_affect_style(unsafe { bytes(name) }))
}
