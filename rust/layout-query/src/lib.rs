//! Southstar — box-tree queries ported from layout.c: find-in-page matches, CSS scroll snapping, paint bounds and subtree extents, inline element rectangles and the boxes, videos and frame viewports a laid-out page holds.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod collect;
mod extent;
mod ffi;
mod find;
mod snap;

use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::StyleRef;

pub use collect::{frame_viewport, kind_name, record_frame_viewports};
pub use extent::{Extent, compute_paint_bounds};

pub fn count_matches(root: BoxRef<'_>, needle: &[u8], case_sensitive: bool) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    find::count_matches(root, needle, case_sensitive)
}

pub fn first_match_below<'a>(
    root: BoxRef<'a>,
    needle: &[u8],
    y_threshold: f64,
    case_sensitive: bool,
) -> Option<BoxRef<'a>> {
    if needle.is_empty() {
        return None;
    }
    find::first_match_below(root, needle, y_threshold, case_sensitive)
}

pub fn first_match_above<'a>(
    root: BoxRef<'a>,
    needle: &[u8],
    y_threshold: f64,
    case_sensitive: bool,
) -> Option<BoxRef<'a>> {
    if needle.is_empty() {
        return None;
    }
    find::first_match_above(root, needle, y_threshold, case_sensitive)
}

pub fn match_ordinal(
    root: BoxRef<'_>,
    needle: &[u8],
    target: BoxRef<'_>,
    case_sensitive: bool,
) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    find::match_ordinal(root, needle, target, case_sensitive)
}

pub fn scroll_snap_from(scroller: BoxRef<'_>, prev_x: f64, prev_y: f64) {
    let Some(style) = southstar_style::box_style(scroller) else {
        return;
    };
    let Some(kind) = snap::snap_type(style) else {
        return;
    };
    let port = snap::scroller_port(scroller, style, prev_x, prev_y);
    if let Some((x, y)) = snap::solve(scroller, kind, &port) {
        scroller.set_scroll(x.unwrap_or(port.cur_x), y.unwrap_or(port.cur_y));
    }
}

pub fn scroll_snap(scroller: BoxRef<'_>) {
    scroll_snap_from(scroller, scroller.scroll_x(), scroller.scroll_y());
}

pub struct SnapViewport {
    pub size: (f64, f64),
    pub max: (f64, f64),
    pub prev: (f64, f64),
}

pub fn scroll_snap_viewport(
    root: BoxRef<'_>,
    style: StyleRef<'_>,
    viewport: &SnapViewport,
    pos: (f64, f64),
) -> Option<(f64, f64)> {
    let (w, h) = viewport.size;
    if w.is_nan() || w <= 0.0 || h.is_nan() || h <= 0.0 {
        return None;
    }
    let kind = snap::snap_type(style)?;
    let port = snap::viewport_port(style, w, h, pos, viewport.prev, viewport.max);
    let (sx, sy) = snap::solve(root, kind, &port)?;
    Some((sx.unwrap_or(pos.0), sy.unwrap_or(pos.1)))
}

pub fn subtree_extent_y(b: BoxRef<'_>) -> Extent {
    extent::subtree_extent_y(Some(b))
}

pub fn inline_rect_for_dom(root: BoxRef<'_>, target: Node<'_>) -> Option<(f64, f64, f64, f64)> {
    extent::inline_rect_for_dom(root, target).map(|r| (r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0))
}

pub fn collect_images(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let mut boxes = Vec::new();
    collect::images(root, &mut boxes);
    boxes
}

pub fn collect_videos(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let mut boxes = Vec::new();
    collect::videos(root, &mut boxes);
    boxes
}
