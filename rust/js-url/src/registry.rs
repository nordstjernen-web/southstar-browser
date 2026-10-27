//! Southstar — the blob: URL registry: each page's object URLs and the pages the network resolver searches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crate::ffi::{Js, cstring};

pub type Blob = (Arc<[u8]>, Option<Vec<u8>>);

struct Entry {
    bytes: Arc<[u8]>,
    kind: Option<Vec<u8>>,
}

impl Entry {
    fn blob(&self) -> Blob {
        (self.bytes.clone(), self.kind.clone())
    }
}

#[derive(Default)]
struct Registry {
    installed: bool,
    pages: Vec<usize>,
    urls: HashMap<usize, HashMap<Vec<u8>, Entry>>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

fn key(url: &[u8]) -> Vec<u8> {
    cstring(url).into_bytes()
}

pub(crate) fn add_page(js: Js) -> bool {
    let js = js.key();
    let mut registry = registry();
    let install = !registry.installed;
    registry.installed = true;
    if !registry.pages.contains(&js) {
        registry.pages.push(js);
    }
    install
}

pub(crate) fn remove_page(js: Js) {
    let js = js.key();
    let mut registry = registry();
    if let Some(index) = registry.pages.iter().position(|&p| p == js) {
        registry.pages.swap_remove(index);
    }
}

pub(crate) fn put(js: Js, url: &[u8], bytes: &[u8], kind: Option<&[u8]>) {
    let js = js.key();
    let entry = Entry {
        bytes: bytes.into(),
        kind: kind
            .map(|k| cstring(k).into_bytes())
            .filter(|k| !k.is_empty()),
    };
    registry()
        .urls
        .entry(js)
        .or_default()
        .insert(key(url), entry);
}

pub(crate) fn remove(js: Js, url: &[u8]) {
    let js = js.key();
    if let Some(urls) = registry().urls.get_mut(&js) {
        urls.remove(&key(url));
    }
}

pub(crate) fn free_page(js: Js) {
    let js = js.key();
    let urls = registry().urls.remove(&js);
    drop(urls);
}

pub(crate) fn lookup(js: Js, url: &[u8]) -> Option<Blob> {
    let js = js.key();
    registry().urls.get(&js)?.get(url).map(Entry::blob)
}

pub(crate) fn resolve(url: &[u8]) -> Option<Blob> {
    let registry = registry();
    registry
        .pages
        .iter()
        .find_map(|js| registry.urls.get(js)?.get(url))
        .map(Entry::blob)
}
