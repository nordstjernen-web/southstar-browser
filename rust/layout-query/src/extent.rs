//! Southstar — vertical extents of laid-out subtrees: the paint bounds layout records on every box, a subtree's border-box extent for the layer planner, and the union of an inline element's fragments.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef};
use southstar_style::{PropId, StyleRef, ValueRef};

use crate::ffi;

fn is_fixed_or_sticky(s: StyleRef<'_>) -> bool {
    s.get(PropId::Position)
        .is_some_and(|v| v.is_keyword(c"fixed") || v.is_keyword(c"sticky"))
}

fn paint_unbounded(s: StyleRef<'_>) -> bool {
    if s.get(PropId::Transform)
        .and_then(ValueRef::transform)
        .is_some_and(|t| t.n_ops > 0)
    {
        return true;
    }
    if s.get(PropId::Animation).is_some() || s.get(PropId::Transition).is_some() {
        return true;
    }
    is_fixed_or_sticky(s)
}

fn border_box_bottom(b: BoxRef<'_>, top: f64) -> f64 {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    top + m.top + bd.top + p.top + b.content_height() + p.bottom + bd.bottom + m.bottom
}

pub fn compute_paint_bounds(b: BoxRef<'_>) {
    let (mut top, mut bottom) = (b.y(), border_box_bottom(b, b.y()));
    if ffi::style_of(b).is_some_and(paint_unbounded) {
        top = -f64::MAX;
        bottom = f64::MAX;
    }
    for c in southstar_layout::children(b) {
        compute_paint_bounds(c);
        if c.paint_top() < top {
            top = c.paint_top();
        }
        if c.paint_bottom() > bottom {
            bottom = c.paint_bottom();
        }
    }
    for atomic in b.inline_atomics().unwrap_or(&[]) {
        let Some(ab) = atomic.box_ref() else {
            continue;
        };
        compute_paint_bounds(ab);
        let dy = b.y() + atomic.owner_offset().1 + ab.rel_dy() - ab.y();
        if ab.paint_top() + dy < top {
            top = ab.paint_top() + dy;
        }
        if ab.paint_bottom() + dy > bottom {
            bottom = ab.paint_bottom() + dy;
        }
    }
    b.set_paint_bounds(top, bottom);
}

fn moves_when_painted(b: BoxRef<'_>) -> bool {
    ffi::style_of(b).is_some_and(|s| paint_unbounded(s) && !is_fixed_or_sticky(s))
}

pub struct Extent {
    pub top: f64,
    pub bottom: f64,
    pub exact: bool,
}

fn extent_walk(b: BoxRef<'_>, off: f64, ext: &mut Extent) {
    if moves_when_painted(b) {
        ext.exact = false;
    }
    let t = b.y() + off;
    let bt = border_box_bottom(b, t);
    if t.is_finite() && t < ext.top {
        ext.top = t;
    }
    if bt.is_finite() && bt > ext.bottom {
        ext.bottom = bt;
    }
    for c in southstar_layout::children(b) {
        extent_walk(c, off, ext);
    }
    for atomic in b.inline_atomics().unwrap_or(&[]) {
        if let Some(ab) = atomic.box_ref() {
            extent_walk(
                ab,
                off + b.y() + atomic.owner_offset().1 + ab.rel_dy() - ab.y(),
                ext,
            );
        }
    }
}

pub fn subtree_extent_y(b: Option<BoxRef<'_>>) -> Extent {
    let mut ext = Extent {
        top: f64::MAX,
        bottom: -f64::MAX,
        exact: true,
    };
    if let Some(b) = b {
        extent_walk(b, 0.0, &mut ext);
    }
    if ext.top > ext.bottom {
        ext.top = 0.0;
        ext.bottom = 0.0;
    }
    ext
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

fn union_walk(root: BoxRef<'_>, target: Node<'_>, dx: f64, dy: f64, acc: &mut Option<Rect>) {
    if root.kind() == BoxKind::Inline && root.has_attrs() && root.text().is_some() {
        for r in root.attrs() {
            if r.dom_ptr() != target.as_ptr().cast() || r.len == 0 {
                continue;
            }
            let Some((ex, ey, ew, eh)) = ffi::inline_range_extents(root, r) else {
                continue;
            };
            let x0 = root.x() + dx + ex;
            let y0 = root.y() + dy + ey;
            let rect = Rect {
                x0,
                y0,
                x1: x0 + ew,
                y1: y0 + eh,
            };
            *acc = Some(match *acc {
                None => rect,
                Some(u) => Rect {
                    x0: if rect.x0 < u.x0 { rect.x0 } else { u.x0 },
                    y0: if rect.y0 < u.y0 { rect.y0 } else { u.y0 },
                    x1: if rect.x1 > u.x1 { rect.x1 } else { u.x1 },
                    y1: if rect.y1 > u.y1 { rect.y1 } else { u.y1 },
                },
            });
        }
    }
    let cdx = dx - root.scroll_x();
    let cdy = dy - root.scroll_y();
    for c in southstar_layout::children(root) {
        union_walk(c, target, cdx, cdy, acc);
    }
    for atomic in root.inline_atomics().unwrap_or(&[]) {
        let Some(ab) = atomic.box_ref() else {
            continue;
        };
        let (ox, oy) = atomic.owner_offset();
        let adx = cdx + root.x() + ox + ab.rel_dx() - ab.x();
        let ady = cdy + root.y() + oy + ab.rel_dy() - ab.y();
        union_walk(ab, target, adx, ady, acc);
    }
}

pub fn inline_rect_for_dom(root: BoxRef<'_>, target: Node<'_>) -> Option<Rect> {
    let mut acc = None;
    union_walk(root, target, 0.0, 0.0, &mut acc);
    acc
}
