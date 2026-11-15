//! Southstar — the scroll offsets of a box tree's scroll containers, kept by element across a relayout.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use crate::{BoxRef, children};

pub type ScrollOffsets = HashMap<usize, (f64, f64)>;

fn collect(b: BoxRef<'_>, map: &mut ScrollOffsets) {
    if !b.dom_ptr().is_null() && (b.scroll_y() != 0.0 || b.scroll_x() != 0.0) {
        map.insert(b.dom_ptr() as usize, (b.scroll_x(), b.scroll_y()));
    }
    for c in children(b) {
        collect(c, map);
    }
}

pub fn scroll_offsets(root: Option<BoxRef<'_>>) -> ScrollOffsets {
    let mut map = ScrollOffsets::new();
    if let Some(root) = root {
        collect(root, &mut map);
    }
    map
}

fn clamped(v: f64, max: f64) -> f64 {
    v.min(max).max(0.0)
}

pub fn restore_scroll_offsets(root: Option<BoxRef<'_>>, map: &ScrollOffsets) {
    let Some(b) = root else {
        return;
    };
    if map.is_empty() {
        return;
    }
    if !b.dom_ptr().is_null()
        && let Some(&(x, y)) = map.get(&(b.dom_ptr() as usize))
    {
        b.set_scroll(clamped(x, b.scroll_max_x()), clamped(y, b.scroll_max_y()));
    }
    for c in children(b) {
        restore_scroll_offsets(Some(c), map);
    }
}
