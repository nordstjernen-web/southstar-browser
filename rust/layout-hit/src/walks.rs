//! Southstar — the narrower hit tests: the form control, the inline element and the link under a point, the scroll container a wheel or scrollbar drag reaches, and the node a click targets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef, LinkRange, Style, children, inline_kind};

use crate::ffi;
use crate::geometry::{
    atomic_point, border_contains, clips_out, enter, outside_paint_span, padding_contains,
    scrolled, untransform,
};
use crate::stack::{box_blocks_hit_testing, children_stacked, node_is_form_hit_target};

pub fn form_hit<'a>(b: BoxRef<'a>, x: f64, y: f64, inherited: *const Style) -> Option<Node<'a>> {
    let (x, y) = enter(b, x, y);
    let (x, y) = untransform(b, x, y)?;
    let child_inherited = if b.style().is_null() {
        inherited
    } else {
        b.style()
    };
    let dom = ffi::dom_of(b);
    let self_hit =
        (node_is_form_hit_target(dom) && border_contains(b, x, y) && !box_blocks_hit_testing(b))
            .then_some(dom)
            .flatten();
    if b.kind() == BoxKind::Inline
        && let Some(m) = ffi::inline_box_form_hit(b, x - b.x(), y - b.y(), child_inherited)
    {
        return Some(m);
    }
    if clips_out(b, x, y) {
        return None;
    }
    if ffi::paint_3d_registered(b) {
        return self_hit;
    }
    let (cx, cy) = scrolled(b, x, y);
    let mut best = None;
    for c in children_stacked(b) {
        if let Some(m) = form_hit(c, cx, cy, child_inherited) {
            best = Some(m);
        }
    }
    for atomic in b.inline_atomics().unwrap_or(&[]) {
        let Some((ab, ax, ay)) = atomic_point(b, atomic, cx, cy) else {
            continue;
        };
        if let Some(m) = form_hit(ab, ax, ay, child_inherited) {
            best = Some(m);
        }
    }
    best.or(self_hit)
}

pub fn scrollable(root: BoxRef<'_>, x: f64, y: f64) -> Option<(BoxRef<'_>, f64, f64)> {
    let (x, y) = enter(root, x, y);
    let (x, y) = untransform(root, x, y)?;
    if outside_paint_span(root, y) || clips_out(root, x, y) {
        return None;
    }
    let (cx, cy) = scrolled(root, x, y);
    if let Some(m) = children(root).find_map(|c| scrollable(c, cx, cy)) {
        return Some(m);
    }
    for atomic in root.inline_atomics().unwrap_or(&[]) {
        let Some((ab, ax, ay)) = atomic_point(root, atomic, cx, cy) else {
            continue;
        };
        if let Some(m) = scrollable(ab, ax, ay) {
            return Some(m);
        }
    }
    (root.scrolls()
        && (root.scroll_max_x() > 0.0 || root.scroll_max_y() > 0.0)
        && padding_contains(root, x, y))
    .then_some((root, x, y))
}

fn inline_content_contains(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    x >= b.x() && x <= b.x() + b.content_width() && y >= b.y() && y <= b.y() + b.content_height()
}

pub fn link_range(root: BoxRef<'_>, x: f64, y: f64) -> Option<&LinkRange> {
    let (x, y) = untransform(root, x, y)?;
    for atomic in root.inline_atomics().unwrap_or(&[]) {
        let (sx, sy) = scrolled(root, x, y);
        let Some((ab, ax, ay)) = atomic_point(root, atomic, sx, sy) else {
            continue;
        };
        if let Some(r) = link_range(ab, ax, ay) {
            return Some(r);
        }
    }
    let links = root.links();
    if !box_blocks_hit_testing(root)
        && root.kind() == BoxKind::Inline
        && !links.is_empty()
        && inline_content_contains(root, x, y)
    {
        let byte = ffi::inline_xy_to_byte(root, x - root.x(), y - root.y())?;
        return links
            .iter()
            .find(|r| byte >= r.start && byte < r.start + r.len);
    }
    if clips_out(root, x, y) {
        return None;
    }
    let (cx, cy) = scrolled(root, x, y);
    let mut best = None;
    for c in children_stacked(root) {
        if let Some(r) = link_range(c, cx, cy) {
            best = Some(r);
        }
    }
    best
}

pub fn inline_dom(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    let (x, y) = enter(root, x, y);
    let (x, y) = untransform(root, x, y)?;
    if outside_paint_span(root, y) {
        return None;
    }
    for atomic in root.inline_atomics().unwrap_or(&[]) {
        let (sx, sy) = scrolled(root, x, y);
        let Some((ab, ax, ay)) = atomic_point(root, atomic, sx, sy) else {
            continue;
        };
        if let Some(m) = inline_dom(ab, ax, ay) {
            return Some(m);
        }
    }
    let attrs = root.attrs();
    if !box_blocks_hit_testing(root)
        && root.kind() == BoxKind::Inline
        && !attrs.is_empty()
        && root.text().is_some_and(|t| !t.is_empty())
        && inline_content_contains(root, x, y)
    {
        let byte = ffi::inline_xy_to_byte(root, x - root.x(), y - root.y())?;
        let mut best: Option<(Node<'_>, usize)> = None;
        for r in attrs {
            if r.kind != inline_kind::ELEMENT {
                continue;
            }
            let Some(dom) = ffi::node(r.dom_ptr()) else {
                continue;
            };
            if byte < r.start || byte >= r.start + r.len {
                continue;
            }
            if best.is_none_or(|(_, len)| r.len < len) {
                best = Some((dom, r.len));
            }
        }
        return best.map(|(dom, _)| dom);
    }
    if clips_out(root, x, y) || ffi::paint_3d_registered(root) {
        return None;
    }
    let (cx, cy) = scrolled(root, x, y);
    let mut best = None;
    for c in children_stacked(root) {
        if let Some(m) = inline_dom(c, cx, cy) {
            best = Some(m);
        }
    }
    best
}

pub fn hit_node(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    let hit = crate::tree::hit_test(root, x, y);
    let (local_x, local_y) = crate::tree::last_local();
    let mut target = hit.and_then(ffi::dom_of);
    if let Some(m) = inline_dom(root, x, y) {
        target = Some(m);
    }
    if let Some(m) = form_hit(root, x, y, core::ptr::null()) {
        target = Some(m);
    }
    if let Some(hit) = hit
        && target == ffi::dom_of(hit)
        && let Some(area) = ffi::image_map_area(hit, local_x, local_y)
    {
        target = Some(area);
    }
    target
}
