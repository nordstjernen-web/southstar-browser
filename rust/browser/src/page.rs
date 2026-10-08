//! Southstar — relayout of a page: saved scroll offsets, the oscillation damper, mutation-driven relayout, the viewport and the page size.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::collections::HashMap;
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_layout::{BoxRef, children};

use crate::ffi::{self, NsBrowser};
use crate::images;

const LAYOUT_OSC_THRESHOLD: c_int = 6;
const LAYOUT_RAPID_US: i64 = 250 * 1000;
const LAYOUT_DAMP_US: i64 = 700 * 1000;
const LAYOUT_BACKGROUND_MIN_US: i64 = 16 * 1000;
const LAYOUT_BACKGROUND_MAX_US: i64 = 120 * 1000;
const FNV_PRIME: u64 = 0x100000001b3;
const FNV_OFFSET: u64 = 0xcbf29ce484222325;

fn doc_has_node(root: Node<'_>, target: Node<'_>) -> bool {
    let mut n = Some(root);
    while let Some(cur) = n {
        if core::ptr::eq(cur.as_ptr(), target.as_ptr()) {
            return true;
        }
        n = ffi::next_in_subtree(cur, root);
    }
    false
}

fn node_alive(b: &NsBrowser, node: Node<'_>) -> bool {
    b.doc().is_some_and(|doc| doc_has_node(doc, node))
}

pub fn prune_cached_nodes(b: &NsBrowser) {
    if b.doc().is_none() {
        return;
    }
    for slot in [&b.press_node, &b.hover_node, &b.open_select] {
        if slot.get().is_some_and(|n| !node_alive(b, n)) {
            slot.set(None);
        }
    }
}

fn signature_walk(b: Option<BoxRef<'_>>, mut h: u64) -> u64 {
    let Some(b) = b else {
        return h;
    };
    let q = [
        (b.x() * 4.0) as i32,
        (b.y() * 4.0) as i32,
        (b.content_width() * 4.0) as i32,
        (b.content_height() * 4.0) as i32,
    ];
    h ^= u64::from(b.kind_raw());
    h = h.wrapping_mul(FNV_PRIME);
    for v in q {
        for byte in v.to_ne_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    for c in children(b) {
        h = signature_walk(Some(c), h);
    }
    for atomic in b.inline_atomic_boxes() {
        h = signature_walk(Some(atomic), h);
    }
    h
}

fn layout_signature(root: Option<BoxRef<'_>>) -> u64 {
    signature_walk(root, FNV_OFFSET)
}

pub fn damp_reset(b: &NsBrowser) {
    b.layout_osc.set(0);
    b.damp_until_us.set(0);
    b.damp_logged.set(false);
}

fn collect_scroll(b: Option<BoxRef<'_>>, map: &mut HashMap<usize, (f64, f64)>) {
    let Some(b) = b else {
        return;
    };
    if !b.dom_ptr().is_null() && (b.scroll_y() != 0.0 || b.scroll_x() != 0.0) {
        map.insert(b.dom_ptr() as usize, (b.scroll_x(), b.scroll_y()));
    }
    for c in children(b) {
        collect_scroll(Some(c), map);
    }
}

fn clamp_scroll(v: f64, max: f64) -> f64 {
    let max = if max > 0.0 { max } else { 0.0 };
    if v < 0.0 {
        0.0
    } else if v > max {
        max
    } else {
        v
    }
}

fn restore_scroll(b: Option<BoxRef<'_>>, map: &HashMap<usize, (f64, f64)>) {
    let Some(b) = b else {
        return;
    };
    if !b.dom_ptr().is_null() {
        if let Some(&(x, y)) = map.get(&(b.dom_ptr() as usize)) {
            b.set_scroll(
                clamp_scroll(x, b.scroll_max_x()),
                clamp_scroll(y, b.scroll_max_y()),
            );
        }
    }
    for c in children(b) {
        restore_scroll(Some(c), map);
    }
}

fn datalist_open(b: &NsBrowser, focused: Option<Node<'_>>) -> bool {
    !b.datalist_suppressed.get()
        && focused.is_some_and(|f| ffi::is_named(f, c"input") && f.attr(c"list").is_some())
}

pub fn relayout(b: &NsBrowser) {
    if b.relaying.get() {
        b.dirty.set(true);
        return;
    }
    b.relaying.set(true);
    b.cascade_dirty.set(false);
    if let Some(js) = b.js() {
        js.consume_mutated();
    }
    b.image_arrivals_since_layout.set(0);
    let mut scroll_save = HashMap::new();
    collect_scroll(b.layout(), &mut scroll_save);
    ffi::css_set_viewport(f64::from(b.vw.get()), b.vh.get());
    ffi::css_set_doc_language(b.doc_language.get());
    if let Some(js) = b.js() {
        js.sync_window_metrics();
    }
    b.search_active.set(None);
    b.selection_clear();
    if let (Some(js), Some(_)) = (b.js(), b.layout()) {
        js.set_layout_root(None);
    }
    if b.layout().is_some() {
        b.take_layout();
    }
    if let Some(js) = b.js().filter(|_| !b.styles_raw().is_null()) {
        js.set_style_table(None);
    }
    b.drop_styles();
    prune_cached_nodes(b);
    b.set_open_select_for_layout();
    ffi::layout_set_datalist_open(datalist_open(b, b.js().and_then(|js| js.focused_node())));
    let t0 = ffi::monotonic_us();
    b.engine_relayout(b.js().and_then(|js| js.focused_node()));
    b.relayout_cost_us.set(ffi::monotonic_us() - t0);
    b.relaying.set(false);
    if !scroll_save.is_empty() {
        restore_scroll(b.layout(), &scroll_save);
    }
    b.images_fetched.set(false);
    b.has_deferred_lazy.set(false);
    b.images_arrived_since_layout.set(false);
    if let Some(js) = b.js() {
        js.set_style_table(Some(b.styles_raw()));
        js.set_layout_root(b.layout());
        images::ensure_images(b);
        ffi::schedule_media_events(b);
    }
    note_layout_signature(b);
}

fn note_layout_signature(b: &NsBrowser) {
    let now = ffi::monotonic_us();
    let rapid = now - b.last_layout_us.get() < LAYOUT_RAPID_US;
    b.last_layout_us.set(now);
    let sig = layout_signature(b.layout());
    let [last, before] = b.layout_sig.get();
    if sig == last || sig == before {
        if rapid && b.layout_osc.get() < c_int::MAX {
            b.layout_osc.set(b.layout_osc.get() + 1);
        }
    } else {
        damp_reset(b);
    }
    b.layout_sig.set([sig, last]);
}

pub fn mutation_relayout_due(b: &NsBrowser) -> bool {
    if b.layout().is_none() || b.last_layout_us.get() <= 0 {
        return true;
    }
    let min_gap = b
        .relayout_cost_us
        .get()
        .clamp(LAYOUT_BACKGROUND_MIN_US, LAYOUT_BACKGROUND_MAX_US);
    ffi::monotonic_us() - b.last_layout_us.get() >= min_gap
}

pub fn relayout_from_mutation(b: &NsBrowser) -> bool {
    if b.layout().is_some() && b.layout_osc.get() >= LAYOUT_OSC_THRESHOLD {
        let now = ffi::monotonic_us();
        if now < b.damp_until_us.get() {
            b.dirty.set(true);
            return false;
        }
        b.damp_until_us.set(now + LAYOUT_DAMP_US);
        if !b.damp_logged.get() {
            b.damp_logged.set(true);
            ffi::log_message(
                c"southstar: layout dampener engaged (script reflow loop with no user input)",
            );
        }
    }
    relayout(b);
    true
}

pub fn flush(b: &NsBrowser) {
    let Some(js) = b.js() else {
        return;
    };
    if js.consume_mutated() {
        b.dirty.set(true);
    }
    if b.layout().is_none() || b.dirty.get() || b.cascade_dirty.get() {
        relayout(b);
        b.dirty.set(false);
    }
}

fn overflow_props() -> &'static [c_int; 3] {
    static PROPS: OnceLock<[c_int; 3]> = OnceLock::new();
    PROPS.get_or_init(|| [c"overflow-x", c"overflow-y", c"overflow"].map(ffi::css_prop_id))
}

fn overflow_keyword_hidden(kw: Option<&CStr>) -> bool {
    kw.is_some_and(|kw| {
        let kw = kw.to_bytes();
        kw.eq_ignore_ascii_case(b"hidden") || kw.eq_ignore_ascii_case(b"clip")
    })
}

fn box_axis_overflow_hidden(b: BoxRef<'_>, axis: usize) -> bool {
    if b.style().is_null() {
        return false;
    }
    let props = overflow_props();
    let kw = ffi::style_keyword(b, props[axis]).or_else(|| ffi::style_keyword(b, props[2]));
    overflow_keyword_hidden(kw)
}

fn root_axis_overflow_hidden(b: BoxRef<'_>, axis: usize) -> bool {
    let is_root_element = ffi::box_dom(b)
        .and_then(Node::name)
        .is_some_and(|name| matches!(name.to_bytes(), b"html" | b"body"));
    if is_root_element && box_axis_overflow_hidden(b, axis) {
        return true;
    }
    children(b).any(|c| root_axis_overflow_hidden(c, axis))
}

pub fn page_size(b: &NsBrowser) -> Option<(c_int, c_int)> {
    let layout = b.layout()?;
    let vw = f64::from(b.vw.get());
    let vh = b.vh.get();
    let hide_x = root_axis_overflow_hidden(layout, 0);
    let hide_y = root_axis_overflow_hidden(layout, 1);
    let mut w = if hide_x { vw } else { layout.content_width() };
    if w.is_nan() || w <= 0.0 {
        w = vw;
    }
    let mut bottom = if hide_y { vh } else { layout.content_height() };
    if !hide_y {
        bottom = layout.max_bottom(bottom);
    }
    if bottom.is_nan() || bottom <= 0.0 {
        bottom = 0.0;
    }
    let ypad = if !hide_y && bottom > vh + 0.5 { 32 } else { 0 };
    Some((w as c_int, bottom as c_int + ypad))
}

pub fn set_viewport(b: &NsBrowser, css_width: c_int, css_height: f64) -> c_int {
    if b.doc().is_none() || css_width <= 0 {
        return -1;
    }
    let css_height = if css_height <= 0.0 {
        f64::from(css_width) * 0.75
    } else {
        css_height
    };
    if css_width == b.vw.get() && css_height == b.vh.get() {
        return 0;
    }
    b.vw.set(css_width);
    b.vh.set(css_height);
    ffi::css_set_viewport(f64::from(css_width), css_height);
    if let Some(js) = b.js() {
        js.sync_window_metrics();
        js.dispatch_resize();
    }
    damp_reset(b);
    relayout(b);
    0
}

pub fn set_device_pixel_ratio(b: Option<&NsBrowser>, dppx: f64) -> c_int {
    if dppx.is_nan() || dppx <= 0.0 {
        return -1;
    }
    ffi::css_set_device_pixel_ratio(dppx);
    let Some(b) = b.filter(|b| b.doc().is_some() && b.dppx.get() != dppx) else {
        return 0;
    };
    b.dppx.set(dppx);
    if let Some(js) = b.js() {
        js.sync_window_metrics();
        js.reeval_media_queries();
    }
    relayout(b);
    1
}
