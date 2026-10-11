//! Southstar — the per-page cache that gives each element one NamedNodeMap for its attributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::Value;

use crate::ffi::{self, Js};
use crate::{Element, key, with_page};

pub(crate) fn lookup(js: Js, owner: Element) -> Option<Value> {
    with_page(js, |page| {
        page.attribute_maps
            .as_ref()
            .and_then(|maps| maps.get(&key(owner)).cloned())
    })
}

pub(crate) fn remember(js: Js, owner: Element, map: Value) {
    let replaced = with_page(js, |page| {
        page.attribute_maps
            .as_mut()
            .and_then(|maps| maps.insert(key(owner), map))
    });
    drop(replaced);
}

pub(crate) fn release_owner(js: Js, owner: Element) {
    if !ffi::has_main_context(js) {
        return;
    }
    let released = with_page(js, |page| {
        page.attribute_maps
            .as_mut()
            .and_then(|maps| maps.remove(&key(owner)))
    });
    drop(released);
}

pub(crate) fn release_all(js: Js) {
    if !ffi::has_main_context(js) {
        return;
    }
    let released: Vec<Value> = with_page(js, |page| {
        page.attribute_maps
            .as_mut()
            .map(|maps| maps.drain().map(|(_, map)| map).collect())
            .unwrap_or_default()
    });
    drop(released);
}

pub(crate) fn teardown(js: Js) {
    release_all(js);
    let leftover = with_page(js, |page| page.attribute_maps.take());
    if ffi::has_main_context(js) {
        drop(leftover);
    } else {
        core::mem::forget(leftover);
    }
}
