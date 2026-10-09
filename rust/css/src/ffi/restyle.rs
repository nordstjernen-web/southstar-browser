//! Southstar — the C ABI of restyle invalidation: the selector dependencies prepared for a style pass, the dirty elements it recomputes, and the marks a DOM mutation leaves.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::slice;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use super::selector_view::SheetRef;
use crate::restyle;

unsafe fn bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_restyle_prepare(
    ua: *const c_void,
    author: *const *const c_void,
    n_author: usize,
    sig: u64,
) -> GBoolean {
    let author: &[*const c_void] = if author.is_null() || n_author == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(author, n_author) }
    };
    let sheets: Vec<SheetRef<'_>> = core::iter::once(ua)
        .chain(author.iter().copied())
        .filter_map(|sheet| unsafe { SheetRef::from_ptr(sheet) })
        .collect();
    glib::boolean(restyle::prepare(&sheets, sig))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_restyle_dirty(node: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(node) }.is_some_and(restyle::is_dirty))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_restyle_dirty_clear() {
    restyle::clear_dirty();
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
