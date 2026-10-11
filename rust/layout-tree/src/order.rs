//! Southstar — reordering a flex or grid container's children by their CSS order values, stably.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, children};
use southstar_style::{PropId, ValueRef};

use crate::ffi::style_of;

fn css_order(b: BoxRef<'_>) -> i32 {
    let Some(s) = style_of(b) else {
        return 0;
    };
    s.get(PropId::Order)
        .and_then(ValueRef::length)
        .map_or(0, |(v, _)| v as i32)
}

pub fn reorder_children(b: BoxRef<'_>) {
    let mut arr: Vec<BoxRef<'_>> = children(b).collect();
    if arr.len() < 2 || arr.iter().all(|&c| css_order(c) == 0) {
        return;
    }
    for a in 1..arr.len() {
        let key = arr[a];
        let ko = css_order(key);
        let mut j = a;
        while j > 0 && css_order(arr[j - 1]) > ko {
            arr[j] = arr[j - 1];
            j -= 1;
        }
        arr[j] = key;
    }
    for (k, c) in arr.iter().enumerate() {
        c.set_next_sibling(arr.get(k + 1).copied());
    }
    b.set_first_child(arr.first().copied());
    b.set_last_child(arr.last().copied());
}
