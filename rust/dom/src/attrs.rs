//! Southstar — element attributes: lookup by name and namespace, setting and removal, the name bloom filter and class tokens.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::data::NewAttr;
use crate::{Attr, FLAG_SVG_NS, Node};

pub fn is_ascii_lower(s: &[u8]) -> bool {
    !s.iter().any(u8::is_ascii_uppercase)
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
            Some(_) if a.name_is_lowercase() => {
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

pub fn set_len(el: Node, name: &CStr, value: Option<&[u8]>) {
    if el.has_class_set() && name.to_bytes().eq_ignore_ascii_case(b"class") {
        el.clear_class_set();
    }
    el.attrs_changed();
    let value = value.unwrap_or_default();
    if let Some(attr) = el.attrs().find(|a| caseless_eq(a.name(), name.to_bytes())) {
        attr.set_value(value);
        return;
    }
    el.push_attr(NewAttr {
        name: name.to_bytes(),
        value,
        namespace_uri: None,
        prefix: None,
        local_name: None,
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
        attr.set_value(value);
        return;
    }
    el.push_attr(NewAttr {
        name: qualified.to_bytes(),
        value,
        namespace_uri: ns.map(CStr::to_bytes),
        prefix: prefix.map(CStr::to_bytes),
        local_name: Some(local.to_bytes()),
    });
}

pub fn append(el: Node, name: &[u8], value: &[u8]) {
    if el.has_class_set() {
        el.clear_class_set();
    }
    el.attrs_changed();
    el.push_attr(NewAttr {
        name,
        value,
        namespace_uri: None,
        prefix: None,
        local_name: None,
    });
}

pub fn set_attribute(el: Node, name: &CStr, value: Option<&[u8]>) {
    if el.is_element() {
        set_len(el, name, value);
    }
}

pub fn remove_attribute(el: Node, name: &CStr) {
    if el.is_element() {
        remove(el, name);
    }
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
