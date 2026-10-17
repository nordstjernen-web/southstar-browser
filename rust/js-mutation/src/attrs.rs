//! Southstar — the recorded attribute setters and removers that keep mutation records, restyle flags and custom element callbacks in step.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::attrs;

use crate::Element;
use crate::ffi::{self, Js};
use crate::record;

pub(crate) fn c_copy(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn notify(js: Js, n: Element, name: &CStr, old: Option<&CStr>, new: Option<&CStr>, changed: bool) {
    if changed && ffi::attr_may_affect_style(n, name) {
        ffi::mark_mutated(js);
    }
    record::attribute_changed(Some(js), Some(n), Some(name), None, old);
    ffi::ce_attr_changed(js, n, name, old, new);
}

pub(crate) fn set_recorded(js: Option<Js>, n: Element, name: &CStr, value: &[u8]) {
    let old = attrs::find(n, name).and_then(|a| a.value_bytes());
    let changed = old != Some(value);
    let old_copy = old.map(c_copy);
    ffi::set_attr_len(n, name, value);
    if let Some(js) = js {
        let new_value = c_copy(value);
        notify(js, n, name, old_copy.as_deref(), Some(&new_value), changed);
    }
}

pub(crate) fn set_recorded_paint_only(js: Option<Js>, n: Element, name: &CStr, value: &CStr) {
    let old = n.attr(name);
    if old == Some(value) {
        return;
    }
    let old_copy = old.map(CStr::to_owned);
    ffi::set_attr(n, name, value);
    if let Some(js) = js {
        record::attribute_changed(Some(js), Some(n), Some(name), None, old_copy.as_deref());
        ffi::ce_attr_changed(js, n, name, old_copy.as_deref(), Some(value));
        ffi::request_repaint(js);
    }
}

pub struct QualifiedName<'a> {
    pub namespace_uri: Option<&'a CStr>,
    pub prefix: Option<&'a CStr>,
    pub local_name: &'a CStr,
    pub name: &'a CStr,
}

pub(crate) fn set_ns_recorded(js: Option<Js>, n: Element, qname: &QualifiedName, value: &CStr) {
    let old_attr = attrs::find_ns(n, qname.namespace_uri, qname.local_name);
    let record_name = old_attr
        .and_then(|a| a.name())
        .unwrap_or(qname.name)
        .to_owned();
    let old = old_attr.and_then(|a| a.value());
    let changed = old != Some(value);
    let old_copy = old.map(CStr::to_owned);
    ffi::set_attr_ns(n, qname, value);
    if let Some(js) = js {
        if changed && ffi::attr_may_affect_style(n, &record_name) {
            ffi::mark_mutated(js);
        }
        record::attribute_changed(
            Some(js),
            Some(n),
            Some(qname.local_name),
            qname.namespace_uri,
            old_copy.as_deref(),
        );
        ffi::ce_attr_changed(js, n, &record_name, old_copy.as_deref(), Some(value));
    }
}

pub(crate) fn remove_recorded(js: Option<Js>, n: Element, name: &CStr) {
    let Some(old_attr) = attrs::find(n, name) else {
        return;
    };
    let old_copy = old_attr.value().map(CStr::to_owned);
    ffi::attr_detach_matching(js, n, old_attr.namespace_uri(), attrs::local_name(old_attr));
    ffi::remove_attr(n, name);
    if let Some(js) = js {
        if ffi::attr_may_affect_style(n, name) {
            ffi::mark_mutated(js);
        }
        record::attribute_changed(Some(js), Some(n), Some(name), None, old_copy.as_deref());
        ffi::ce_attr_changed(js, n, name, old_copy.as_deref(), None);
    }
}

pub(crate) fn remove_ns_recorded(
    js: Option<Js>,
    n: Element,
    namespace_uri: Option<&CStr>,
    local_name: &CStr,
) {
    let Some(old_attr) = attrs::find_ns(n, namespace_uri, local_name) else {
        return;
    };
    let old_copy = old_attr.value().unwrap_or_default().to_owned();
    let record_name = old_attr.name().unwrap_or(local_name).to_owned();
    ffi::attr_detach_matching(js, n, namespace_uri, local_name);
    ffi::remove_attr_ns(n, namespace_uri, local_name);
    if let Some(js) = js {
        if ffi::attr_may_affect_style(n, &record_name) {
            ffi::mark_mutated(js);
        }
        record::attribute_changed(
            Some(js),
            Some(n),
            Some(local_name),
            namespace_uri,
            Some(&old_copy),
        );
        ffi::ce_attr_changed(js, n, &record_name, Some(&old_copy), None);
    }
}
