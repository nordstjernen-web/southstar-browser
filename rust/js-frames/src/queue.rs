//! Southstar — the frame load queues: frames waiting to load and lazy frames deferred until they come near the viewport.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

use southstar_dom::Node;
use southstar_dom::index::next_in_subtree;

use crate::ffi::{self, Js};
use crate::page::{self, Frame};
use crate::realm::frame_of;

pub(crate) fn pending_count(js: Js) -> usize {
    page::peek(js, |page| page.pending_loads.len()).unwrap_or(0)
}

pub(crate) fn add_pending(js: Js, frame: Frame) -> bool {
    page::with(js, |page| {
        if page.pending_loads.contains(&frame) {
            return false;
        }
        page.pending_loads.push(frame);
        true
    })
}

pub(crate) fn first_pending(js: Js) -> Option<Frame> {
    page::peek(js, |page| page.pending_loads.first().copied()).flatten()
}

pub(crate) fn remove_first_pending(js: Js) {
    page::peek_mut(js, |page| {
        if !page.pending_loads.is_empty() {
            page.pending_loads.remove(0);
        }
    });
}

pub(crate) fn add_deferred(js: Js, frame: Frame) {
    page::with(js, |page| {
        if !page.deferred_loads.contains(&frame) {
            page.deferred_loads.push(frame);
        }
    });
}

pub(crate) fn remove_deferred(js: Js, frame: Frame) {
    page::peek_mut(js, |page| {
        if let Some(index) = page.deferred_loads.iter().position(|f| *f == frame) {
            page.deferred_loads.remove(index);
        }
    });
}

pub(crate) fn promote_deferred(js: Js) {
    let mut index = 0;
    loop {
        let Some(frame) = page::peek(js, |page| page.deferred_loads.get(index).copied()).flatten()
        else {
            return;
        };
        if ffi::beyond_load_range(js, frame) {
            index += 1;
            continue;
        }
        page::peek_mut(js, |page| {
            if page.deferred_loads.get(index) == Some(&frame) {
                page.deferred_loads.remove(index);
            }
        });
        ffi::schedule_iframe_load(js, frame);
    }
}

pub(crate) fn purge_subtree(js: Js, root: Node<'_>) {
    let has_queued = page::peek(js, |page| {
        !page.pending_loads.is_empty() || !page.deferred_loads.is_empty()
    })
    .unwrap_or(false);
    if !has_queued {
        return;
    }
    let mut subtree = HashSet::new();
    let mut node = Some(root);
    while let Some(n) = node {
        subtree.insert(frame_of(n));
        node = next_in_subtree(n, Some(root), true);
    }
    page::peek_mut(js, |page| {
        page.pending_loads.retain(|frame| !subtree.contains(frame));
        page.deferred_loads.retain(|frame| !subtree.contains(frame));
    });
}
