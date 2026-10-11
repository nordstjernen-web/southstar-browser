//! Southstar — finding the layout box generated for a DOM node, with a per-thread map built once the page's layout root is searched repeatedly.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::c_void;
use std::collections::HashMap;

use southstar_layout::BoxRef;

use crate::ffi::{self, Js};

const SEARCHES_BEFORE_MAP: i32 = 8;

#[derive(Default)]
struct BoxLookup {
    js: usize,
    map_root: usize,
    boxes: HashMap<usize, BoxRef<'static>>,
    pending_root: usize,
    pending_count: i32,
}

impl BoxLookup {
    fn forget(&mut self) {
        self.boxes.clear();
        self.map_root = 0;
        self.pending_root = 0;
        self.pending_count = 0;
    }

    fn adopt(&mut self, js: usize) {
        if self.js != js {
            self.forget();
            self.js = js;
        }
    }
}

thread_local! {
    static LOOKUP: RefCell<BoxLookup> = RefCell::new(BoxLookup::default());
}

fn record(boxes: &mut HashMap<usize, BoxRef<'static>>, b: BoxRef<'static>) {
    let dom = b.dom_ptr();
    if !dom.is_null() {
        boxes.insert(dom as usize, b);
    }
    for child in southstar_layout::children(b) {
        record(boxes, child);
    }
    for atomic in b.inline_atomic_boxes() {
        record(boxes, atomic);
    }
}

fn mapped(js: Js, root: BoxRef<'static>, target: *const c_void) -> Option<Option<BoxRef<'static>>> {
    let root_key = root.as_ptr() as usize;
    LOOKUP.with_borrow_mut(|lookup| {
        lookup.adopt(js.addr());
        let target = target as usize;
        if lookup.map_root == root_key {
            return Some(lookup.boxes.get(&target).copied());
        }
        if lookup.pending_root != root_key {
            lookup.pending_root = root_key;
            lookup.pending_count = 0;
        }
        lookup.pending_count += 1;
        if lookup.pending_count < SEARCHES_BEFORE_MAP {
            return None;
        }
        lookup.boxes.clear();
        record(&mut lookup.boxes, root);
        lookup.map_root = root_key;
        Some(lookup.boxes.get(&target).copied())
    })
}

fn below<'a>(root: BoxRef<'a>, target: *const c_void) -> Option<BoxRef<'a>> {
    southstar_layout::children(root)
        .find_map(|child| search(child, target))
        .or_else(|| {
            root.inline_atomic_boxes()
                .into_iter()
                .find_map(|atomic| search(atomic, target))
        })
}

fn search<'a>(root: BoxRef<'a>, target: *const c_void) -> Option<BoxRef<'a>> {
    if root.dom_ptr() == target {
        return Some(root);
    }
    below(root, target)
}

pub(crate) fn find_by_dom<'a>(root: BoxRef<'a>, target: *const c_void) -> Option<BoxRef<'a>> {
    if root.dom_ptr() == target {
        return Some(root);
    }
    if let Some(js) = ffi::active_js()
        && let Some(layout) = ffi::layout_root(js).filter(|layout| layout.same(root))
        && let Some(found) = mapped(js, layout, target)
    {
        return found;
    }
    below(root, target)
}

pub(crate) fn forget(js: Js) {
    LOOKUP.with_borrow_mut(|lookup| {
        if lookup.js == js.addr() {
            lookup.forget();
        }
    });
}
