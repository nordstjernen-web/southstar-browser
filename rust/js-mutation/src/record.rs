//! Southstar — the document index, past-names and restyle upkeep behind recorded child list, attribute and character data changes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind, controls, index};
use southstar_js_engine::Attributes;

use crate::Element;
use crate::ffi::{self, Js};

const FLAG_FRAGMENT: u32 = 1 << 2;
const PAST_NAMES: &str = "_ns_past_names";
const PAST_NAME: Attributes = Attributes::METHOD;
const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) fn scope_document(node: Option<Element>) -> Option<Element> {
    let mut cursor = node;
    while let Some(p) = cursor {
        if p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0 {
            return Some(p);
        }
        cursor = p.parent();
    }
    None
}

fn in_shadow_tree(node: Element) -> bool {
    let mut cursor = Some(node);
    while let Some(p) = cursor {
        if ffi::is_shadow_root(p) {
            return true;
        }
        if p.kind() == Kind::Document {
            return false;
        }
        cursor = p.parent();
    }
    false
}

fn indexed_document(js: Option<Js>, node: Element) -> Option<Element> {
    js?;
    scope_document(Some(node)).filter(|_| !in_shadow_tree(node))
}

pub(crate) fn index_removed(doc: Element, root: Element) {
    index::id_subtree_removed(doc, root);
    index::class_subtree_removed(doc, root);
    index::tag_subtree_removed(doc, root);
}

fn index_added(doc: Element, root: Element) {
    index::id_subtree_added(doc, root);
    index::class_subtree_added(doc, root);
    index::tag_subtree_added(doc, root);
}

fn non_empty(value: Option<&CStr>) -> Option<&CStr> {
    value.filter(|v| !v.is_empty())
}

fn remember_past_names(js: Js, control: Element, names: &[&CStr]) {
    let Some(form) = controls::form_owner(control).filter(|f| f.has_js_wrapper()) else {
        return;
    };
    ffi::with_page_context(js, |scope| {
        let Some(form_value) = ffi::element_value(scope, form) else {
            return;
        };
        let past = match scope.get(&form_value, PAST_NAMES) {
            Ok(past) if past.is_object() => past,
            _ => {
                let past = scope.new_object();
                let _ = scope.define(&form_value, PAST_NAMES, past.clone(), HIDDEN);
                past
            }
        };
        for name in names {
            let Some(element) = ffi::element_value(scope, control) else {
                continue;
            };
            let key = String::from_utf8_lossy(name.to_bytes());
            let _ = scope.define(&past, &key, element, PAST_NAME);
        }
    });
}

fn remember_added_names(js: Js, added: Element) {
    let mut names = Vec::new();
    names.extend(non_empty(added.attr(c"name")));
    names.extend(non_empty(added.attr(c"id")));
    remember_past_names(js, added, &names);
}

pub(crate) fn index_child_changes(
    js: Option<Js>,
    parent: Element,
    added: &[Element],
    removed: &[Element],
) {
    ffi::qcache_invalidate(js);
    let (Some(js), Some(doc)) = (js, indexed_document(js, parent)) else {
        return;
    };
    for &node in removed {
        index_removed(doc, node);
    }
    for &node in added {
        index_added(doc, node);
        remember_added_names(js, node);
    }
}

fn invalidates_qcache(name: &[u8]) -> bool {
    [b"id".as_slice(), b"class", b"form", b"name"]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
}

fn reindex_attribute(
    doc: Element,
    target: Element,
    old: Option<&CStr>,
    attr: &CStr,
    unregister: fn(Element, &CStr, Element),
    register: fn(Element, &CStr, Element),
) {
    if let Some(old) = non_empty(old) {
        unregister(doc, old, target);
    }
    if let Some(new) = non_empty(target.attr(attr)) {
        register(doc, new, target);
    }
}

fn id_unregister(doc: Element, id: &CStr, node: Element) {
    index::id_unregister(doc, id, Some(node));
}

pub(crate) fn attribute_changed(
    js: Option<Js>,
    target: Option<Element>,
    name: Option<&CStr>,
    namespace_uri: Option<&CStr>,
    old: Option<&CStr>,
) {
    let name_bytes = name.map(CStr::to_bytes);
    if js.is_some() && name_bytes.is_some_and(invalidates_qcache) {
        ffi::qcache_invalidate(js);
    }
    let doc = target.and_then(|t| scope_document(Some(t)).filter(|_| !in_shadow_tree(t)));
    if let (Some(_), Some(doc), Some(target), Some(name)) = (js, doc, target, name_bytes) {
        if name.eq_ignore_ascii_case(b"id") {
            reindex_attribute(doc, target, old, c"id", id_unregister, index::id_register);
        }
        if name.eq_ignore_ascii_case(b"class") {
            reindex_attribute(
                doc,
                target,
                old,
                c"class",
                index::class_unregister,
                index::class_register,
            );
        }
    }
    if let (Some(js), Some(target), Some(name)) = (js, target, name)
        && (name.to_bytes().eq_ignore_ascii_case(b"id")
            || name.to_bytes().eq_ignore_ascii_case(b"name"))
    {
        let mut names = Vec::new();
        names.extend(non_empty(old));
        names.extend(non_empty(target.attr(name)));
        remember_past_names(js, target, &names);
    }
    if let (Some(js), Some(target), Some(name), None) = (js, target, name, namespace_uri)
        && is_link_resource_attr(target, name)
        && target.attr(name) != old
    {
        ffi::link_resource_changed(js, target);
    }
    ffi::mark_attr_dirty(target, name, old);
    ffi::emit_attributes(js, target, name, namespace_uri, old);
}

fn is_link_resource_attr(target: Element, name: &CStr) -> bool {
    let name = name.to_bytes();
    target.is_element()
        && target
            .name()
            .is_some_and(|tag| tag.to_bytes().eq_ignore_ascii_case(b"link"))
        && [b"rel".as_slice(), b"href", b"as"]
            .iter()
            .any(|attr| name.eq_ignore_ascii_case(attr))
}

pub(crate) fn character_data_changed(js: Option<Js>, target: Option<Element>, old: Option<&CStr>) {
    ffi::mark_restyle_dirty(target.map(|t| t.parent().unwrap_or(t)));
    ffi::emit_character_data(js, target, old);
}
