//! Southstar — element attributes: lookup by name and namespace, setting and removal, the name bloom filter and class tokens.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};
use core::ptr;

use crate::FLAG_SVG_NS;
use crate::ffi::{
    ATTR_NAME_LOWER, ATTR_OWN_NAME, ATTR_OWN_VALUE, Attr, NewAttr, Node, dup, value_dup,
};

pub fn is_ascii_lower(s: &[u8]) -> bool {
    !s.iter().any(u8::is_ascii_uppercase)
}

fn name_flags(name: &[u8]) -> u8 {
    if is_ascii_lower(name) {
        ATTR_NAME_LOWER
    } else {
        0
    }
}

fn caseless_eq(attr_name: Option<&CStr>, name: &[u8]) -> bool {
    attr_name.is_none_or(|a| a.to_bytes().eq_ignore_ascii_case(name))
}

pub fn is_internal(name: &[u8]) -> bool {
    name.len() >= 8 && name[..8].eq_ignore_ascii_case(b"data-nd-")
}

pub fn local_name(attr: Attr<'_>) -> &CStr {
    attr.local_name().or_else(|| attr.name()).unwrap_or(c"")
}

fn normalize_namespace(ns: Option<&CStr>) -> Option<&CStr> {
    ns.filter(|ns| !ns.is_empty())
}

fn matches_ns(attr: Attr, ns: Option<&CStr>, local: &CStr) -> bool {
    normalize_namespace(attr.namespace_uri()) == ns && local_name(attr) == local
}

pub fn find<'a>(el: Node<'a>, name: &CStr) -> Option<Attr<'a>> {
    if !el.is_element() {
        return None;
    }
    let wanted = name.to_bytes();
    if el.flags() & FLAG_SVG_NS != 0 {
        return el.attrs().find(|a| a.name() == Some(name));
    }
    if is_ascii_lower(wanted) {
        let first = wanted.first().copied().unwrap_or(0);
        return el.attrs().find(|a| match a.name_first_byte() {
            None => false,
            Some(_) if a.flags() & ATTR_NAME_LOWER != 0 => {
                a.name_first_byte() == Some(first) && a.name() == Some(name)
            }
            Some(_) => caseless_eq(a.name(), wanted),
        });
    }
    el.attrs().find(|a| caseless_eq(a.name(), wanted))
}

pub fn get<'a>(el: Node<'a>, name: &CStr) -> Option<&'a CStr> {
    find(el, name).and_then(Attr::value)
}

pub fn find_ns<'a>(el: Node<'a>, namespace_uri: Option<&CStr>, local: &CStr) -> Option<Attr<'a>> {
    if !el.is_element() {
        return None;
    }
    let ns = normalize_namespace(namespace_uri);
    el.attrs().find(|a| matches_ns(*a, ns, local))
}

pub fn set_len(el: Node, name: &CStr, value: Option<&[u8]>, len: usize) {
    if el.has_class_set() && name.to_bytes().eq_ignore_ascii_case(b"class") {
        el.clear_class_set();
    }
    el.attrs_changed();
    let value_ptr = value.map_or(ptr::null(), |v| v.as_ptr().cast());
    if let Some(attr) = el.attrs().find(|a| caseless_eq(a.name(), name.to_bytes())) {
        attr.set_value(value_dup(value_ptr, len), len as c_uint);
        return;
    }
    el.push_attr(NewAttr {
        name: dup(name),
        value: value_dup(value_ptr, len),
        value_len: len as c_uint,
        namespace_uri: ptr::null_mut(),
        prefix: ptr::null_mut(),
        local_name: ptr::null_mut(),
        flags: ATTR_OWN_NAME | ATTR_OWN_VALUE | name_flags(name.to_bytes()),
    });
}

pub struct NsName<'a> {
    pub namespace_uri: Option<&'a CStr>,
    pub prefix: Option<&'a CStr>,
    pub local_name: &'a CStr,
    pub name: Option<&'a CStr>,
}

pub fn set_ns(el: Node, qname: NsName, value: Option<&CStr>) {
    let ns = normalize_namespace(qname.namespace_uri);
    let prefix = qname.prefix.filter(|p| !p.is_empty());
    let local = qname.local_name;
    let qualified = qname.name.filter(|n| !n.is_empty()).unwrap_or(local);
    let is_class = |s: &CStr| s.to_bytes().eq_ignore_ascii_case(b"class");
    if el.has_class_set() && (is_class(local) || is_class(qualified)) {
        el.clear_class_set();
    }
    el.attrs_changed();
    let value = value.map_or(&[][..], CStr::to_bytes);
    if let Some(attr) = el.attrs().find(|a| matches_ns(*a, ns, local)) {
        attr.set_value(
            value_dup(value.as_ptr().cast(), value.len()),
            value.len() as c_uint,
        );
        return;
    }
    el.push_attr(NewAttr {
        name: dup(qualified),
        value: value_dup(value.as_ptr().cast(), value.len()),
        value_len: value.len() as c_uint,
        namespace_uri: ns.map_or(ptr::null_mut(), dup),
        prefix: prefix.map_or(ptr::null_mut(), dup),
        local_name: dup(local),
        flags: ATTR_OWN_NAME | ATTR_OWN_VALUE | name_flags(qualified.to_bytes()),
    });
}

pub fn append_borrowed(el: Node, name: &CStr, value: Option<&CStr>) {
    if el.has_class_set() {
        el.clear_class_set();
    }
    el.attrs_changed();
    el.push_attr(NewAttr {
        name: name.as_ptr().cast_mut(),
        value: value.unwrap_or(c"").as_ptr().cast_mut(),
        value_len: value.map_or(0, |v| v.to_bytes().len() as c_uint),
        namespace_uri: ptr::null_mut(),
        prefix: ptr::null_mut(),
        local_name: ptr::null_mut(),
        flags: name_flags(name.to_bytes()),
    });
}

pub fn remove(el: Node, name: &CStr) {
    if el.has_class_set() && name.to_bytes().eq_ignore_ascii_case(b"class") {
        el.clear_class_set();
    }
    el.attrs_changed();
    el.remove_first_attr(|a| caseless_eq(a.name(), name.to_bytes()));
}

pub fn remove_ns(el: Node, namespace_uri: Option<&CStr>, local: &CStr) {
    let ns = normalize_namespace(namespace_uri);
    if el.has_class_set() && local.to_bytes().eq_ignore_ascii_case(b"class") {
        el.clear_class_set();
    }
    el.attrs_changed();
    el.remove_first_attr(|a| matches_ns(a, ns, local));
}

pub fn name_bloom_bit(name: &[u8]) -> u64 {
    let mut h: u32 = 2_166_136_261;
    for &b in name {
        h ^= u32::from(b);
        h = h.wrapping_mul(16_777_619);
    }
    1u64 << (h & 63)
}

pub fn bloom(el: Node) -> u64 {
    let cached = el.attr_bloom();
    if cached != 0 {
        return cached;
    }
    let bloom = el
        .attrs()
        .filter_map(Attr::name)
        .fold(0, |b, name| b | name_bloom_bit(name.to_bytes()));
    el.set_attr_bloom(bloom);
    bloom
}

fn class_space(c: &u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

pub fn has_class(el: Node, name: &[u8]) -> bool {
    if !el.is_element() {
        return false;
    }
    if let Some(found) = el.class_set_contains(name) {
        return found;
    }
    let tokens: Vec<&[u8]> = match get(el, c"class").map(CStr::to_bytes) {
        Some(class) if !class.is_empty() => {
            class.split(class_space).filter(|t| !t.is_empty()).collect()
        }
        _ => Vec::new(),
    };
    el.set_class_set(&tokens);
    tokens.contains(&name)
}
