//! Southstar — DOM change recording: index and past-names upkeep for child list and attribute changes, the recorded setters, the orphan set, the pending change event and the NamedNodeMap cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod api;
mod attribute_maps;
mod attrs;
mod change;
mod ffi;
mod orphans;
mod record;
mod tree;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

use southstar_dom::{Node, WeakNode};
use southstar_js_engine::Value;

use crate::ffi::Js;

pub use api::*;

pub(crate) type Element = Node<'static>;

#[derive(Default)]
pub(crate) struct Page {
    pub orphans: Option<HashSet<WeakNode>>,
    pub attribute_maps: Option<HashMap<WeakNode, Value>>,
    pub change: Option<(WeakNode, CString)>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn key(node: Node<'_>) -> WeakNode {
    node.downgrade()
}

pub(crate) fn with_page<R: Default>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES
        .try_with(|pages| {
            let mut pages = pages.try_borrow_mut().ok()?;
            pages.get_mut(&js).map(f)
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn init(js: Js) {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let page = pages.entry(js).or_default();
        page.orphans = Some(HashSet::new());
        page.attribute_maps = Some(HashMap::new());
    });
}

pub(crate) fn remove_page(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.try_borrow_mut().ok()?.remove(&js))
        .ok()
        .flatten();
    drop(page);
}
