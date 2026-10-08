//! Southstar — the C ABI of attribute lookup, setting, removal, bloom bits and class tokens declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::memory::{return_if_fail, value_dup};
use super::{Attr, Node, NsAttr, NsNode, c_str};
use crate::attrs::{self, NsName};
use crate::node::KIND_ELEMENT;

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn element<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { node(n) }.filter(|n| n.is_element())
}

fn attr_ptr(attr: Option<Attr>) -> *const NsAttr {
    attr.map_or(ptr::null(), |a| a.attr.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean {
    glib::boolean(c_str(name).is_some_and(|n| attrs::is_internal(n.to_bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_local_name(attr: *const NsAttr) -> *const c_char {
    Attr::link(attr).map_or(c"", attrs::local_name).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_value_dup_len(value: *const c_char, len: usize) -> *mut c_char {
    value_dup(value, len)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_attr_len(
    el: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
    len: isize,
) {
    let function = c"ns_element_set_attr_len";
    let Some(el) = (unsafe { node(el) }) else {
        return return_if_fail(function, c"el != NULL");
    };
    if el.kind_raw() != KIND_ELEMENT {
        return return_if_fail(function, c"el->kind == NS_NODE_ELEMENT");
    }
    let Some(name) = c_str(name) else {
        return return_if_fail(function, c"name != NULL");
    };
    let len = match usize::try_from(len) {
        Ok(len) => len,
        Err(_) => c_str(value).map_or(0, |v| v.to_bytes().len()),
    };
    let value = (!value.is_null()).then(|| unsafe { glib::slice(value.cast(), len) });
    attrs::set_len(el, name, value, len);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_attr(
    el: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
) {
    unsafe { ns_element_set_attr_len(el, name, value, -1) };
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
    let function = c"ns_element_set_attr_ns";
    let Some(el) = (unsafe { node(el) }) else {
        return return_if_fail(function, c"el != NULL");
    };
    if el.kind_raw() != KIND_ELEMENT {
        return return_if_fail(function, c"el->kind == NS_NODE_ELEMENT");
    }
    let Some(local_name) = c_str(local_name) else {
        return return_if_fail(function, c"local_name != NULL");
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
pub unsafe extern "C" fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char) {
    if let (Some(el), Some(name)) = (element(el), c_str(name)) {
        attrs::remove(el, name);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_remove_attr_ns(
    el: *mut NsNode,
    namespace_uri: *const c_char,
    local_name: *const c_char,
) {
    if let (Some(el), Some(local_name)) = (element(el), c_str(local_name)) {
        attrs::remove_ns(el, c_str(namespace_uri), local_name);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_append_attr_borrow(
    el: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
) {
    if let (Some(el), Some(name)) = (element(el), c_str(name)) {
        attrs::append_borrowed(el, name, c_str(value));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_has_class(
    el: *const NsNode,
    name: *const c_char,
    len: usize,
) -> GBoolean {
    let name = unsafe { glib::slice(name.cast(), len) };
    glib::boolean(unsafe { node(el) }.is_some_and(|el| attrs::has_class(el, name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_name_bloom_bit(name: *const c_char) -> u64 {
    attrs::name_bloom_bit(c_str(name).map_or(&[][..], |n| n.to_bytes()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_attr_bloom(el: *const NsNode) -> u64 {
    unsafe { node(el) }.map_or(0, attrs::bloom)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_find_attr(
    el: *const NsNode,
    name: *const c_char,
) -> *const NsAttr {
    let (Some(el), Some(name)) = (unsafe { node(el) }, c_str(name)) else {
        return ptr::null();
    };
    attr_ptr(attrs::find(el, name))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_attr(
    el: *const NsNode,
    name: *const c_char,
) -> *const c_char {
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
    let found = match (unsafe { node(el) }, c_str(name)) {
        (Some(el), Some(name)) => attrs::find(el, name),
        _ => None,
    };
    if !out_len.is_null() {
        unsafe { *out_len = found.map_or(0, |a| a.value_len() as usize) };
    }
    found.map_or(ptr::null(), Attr::value_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_find_attr_ns(
    el: *const NsNode,
    namespace_uri: *const c_char,
    local_name: *const c_char,
) -> *const NsAttr {
    let (Some(el), Some(local_name)) = (unsafe { node(el) }, c_str(local_name)) else {
        return ptr::null();
    };
    attr_ptr(attrs::find_ns(el, c_str(namespace_uri), local_name))
}
