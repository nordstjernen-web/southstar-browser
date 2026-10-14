//! Southstar — the C ABI of attribute lookup, setting and removal.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::{c_str, node};
use crate::NsNode;
use crate::attrs::{self, NsName};
use crate::{Attr, Node};

/// # Safety
///
/// `n` is null or a node pointer this thread's store handed out.
unsafe fn element<'a>(n: *const NsNode) -> Option<Node<'a>> {
    // SAFETY: guaranteed by the caller.
    unsafe { node(n) }.filter(|n| n.is_element())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean {
    glib::boolean(c_str(name).is_some_and(|n| attrs::is_internal(n.to_bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_attr_len(
    el: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
    len: isize,
) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let (Some(el), Some(name)) = (unsafe { element(el) }, c_str(name)) else {
        return;
    };
    let value = match usize::try_from(len) {
        Ok(len) if !value.is_null() => {
            // SAFETY: the C callers pass a value of at least `len` bytes.
            Some(unsafe { glib::slice(value.cast(), len) })
        }
        _ => c_str(value).map(|v| v.to_bytes()),
    };
    attrs::set_attribute(el, name, Some(value.unwrap_or_default()));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_attr(
    el: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let (Some(el), Some(name)) = (unsafe { element(el) }, c_str(name)) {
        attrs::set_attribute(
            el,
            name,
            Some(c_str(value).map_or(&[][..], |v| v.to_bytes())),
        );
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_attr_ns(
    el: *mut NsNode,
    namespace_uri: *const c_char,
    prefix: *const c_char,
    local_name: *const c_char,
    name: *const c_char,
    value: *const c_char,
) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let (Some(el), Some(local_name)) = (unsafe { element(el) }, c_str(local_name)) else {
        return;
    };
    let qname = NsName {
        namespace_uri: c_str(namespace_uri),
        prefix: c_str(prefix),
        local_name,
        name: c_str(name),
    };
    attrs::set_ns(el, qname, c_str(value));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_remove_attr_ns(
    el: *mut NsNode,
    namespace_uri: *const c_char,
    local_name: *const c_char,
) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let (Some(el), Some(local_name)) = (unsafe { element(el) }, c_str(local_name)) {
        attrs::remove_ns(el, c_str(namespace_uri), local_name);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_attr(
    el: *const NsNode,
    name: *const c_char,
) -> *const c_char {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let (Some(el), Some(name)) = (unsafe { node(el) }, c_str(name)) else {
        return ptr::null();
    };
    attrs::find(el, name).map_or(ptr::null(), Attr::value_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_attr_len(
    el: *const NsNode,
    name: *const c_char,
    out_len: *mut usize,
) -> *const c_char {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let found = match (unsafe { node(el) }, c_str(name)) {
        (Some(el), Some(name)) => attrs::find(el, name),
        _ => None,
    };
    if !out_len.is_null() {
        // SAFETY: the C callers pass null or a writable length.
        unsafe { *out_len = found.map_or(0, |a| a.value_len() as usize) };
    }
    found.map_or(ptr::null(), Attr::value_ptr)
}
