//! Southstar — import maps: registering the imports of a document's importmap scripts and mapping module specifiers through them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{MAX_DEPTH, Node};

use crate::ffi::{Js, hidden_child, is_named, url_resolve};
use crate::{ALREADY_STARTED, page};

fn add(js: Js, key: &[u8], value: &[u8], base: Option<&CStr>) {
    if key.is_empty() || value.is_empty() {
        return;
    }
    let Ok(href) = CString::new(value) else {
        return;
    };
    let resolved = url_resolve(base.filter(|base| !base.is_empty()), &href)
        .map_or_else(|| value.to_vec(), CString::into_bytes);
    page::with_page(js, |page| {
        match page.import_map.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = resolved,
            None => page.import_map.push((key.to_vec(), resolved)),
        }
    });
}

fn register_json(js: Js, text: &[u8], base: Option<&CStr>) {
    let entries = js
        .with_main_scope(|scope| {
            let mut entries = Vec::new();
            let Ok(root) = scope.parse_json(text, "importmap") else {
                return entries;
            };
            let Ok(imports) = scope.get(&root, "imports") else {
                return entries;
            };
            if !imports.is_object() {
                return entries;
            }
            let Ok(keys) = scope.own_enumerable_keys(&imports) else {
                return entries;
            };
            for key in keys {
                let Ok(name) = scope.to_bytes(&key) else {
                    continue;
                };
                let Ok(value) = scope.get_key(&imports, &key) else {
                    continue;
                };
                if !value.is_string() {
                    continue;
                }
                if let Ok(value) = scope.to_bytes(&value) {
                    entries.push((name, value));
                }
            }
            entries
        })
        .unwrap_or_default();
    for (key, value) in entries {
        add(js, &key, &value, base);
    }
}

pub(crate) fn register_in(js: Js, root: Node) {
    fn walk(js: Js, n: Node, depth: i32) {
        if depth >= MAX_DEPTH || (depth > 0 && hidden_child(n)) {
            return;
        }
        if is_named(n, c"script") {
            let is_import_map = n
                .attr(c"type")
                .is_some_and(|ty| ty.to_bytes().eq_ignore_ascii_case(b"importmap"));
            if is_import_map && n.attr(ALREADY_STARTED).is_none() {
                crate::mark(n, ALREADY_STARTED);
                let base = js.base_url(n);
                for c in southstar_dom::children(n) {
                    if let (true, Some(text)) = (c.is_text(), c.text()) {
                        register_json(js, text.to_bytes(), base.as_deref());
                    }
                }
            }
            return;
        }
        if is_named(n, c"template") {
            return;
        }
        for c in southstar_dom::children(n) {
            walk(js, c, depth + 1);
        }
    }
    walk(js, root, 0);
}

pub(crate) fn resolve(js: Js, name: &[u8]) -> Option<Vec<u8>> {
    page::peek_page(js, |page| {
        let mut best: Option<&(Vec<u8>, Vec<u8>)> = None;
        for entry in &page.import_map {
            let key = &entry.0;
            if key.as_slice() == name {
                return Some(entry.1.clone());
            }
            if key.last() == Some(&b'/')
                && name.starts_with(key)
                && best.is_none_or(|best| key.len() > best.0.len())
            {
                best = Some(entry);
            }
        }
        best.map(|(key, value)| [value.as_slice(), &name[key.len()..]].concat())
    })
    .flatten()
}
