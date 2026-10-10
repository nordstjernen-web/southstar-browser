//! Southstar — the topmost box under a point: a painting-order walk that defers positioned layers to the end of their stacking scope, then tests each box's own area, honouring an open modal dialog.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;

use southstar_layout::{BoxKind, BoxRef, children};

use crate::ffi;
use crate::geometry::{atomic_point, enter, has_transform, outside_paint_span, padding_contains};
use crate::geometry::{scrolled, untransform};
use crate::stack::{
    box_blocks_hit_testing, children_stacked, merge_sort, positioned_z_index, svg_yields_hit,
    tree_order_cmp,
};

thread_local! {
    static LOCAL: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
}

pub fn last_local() -> (f64, f64) {
    LOCAL.get()
}

#[derive(Clone, Copy)]
struct Deferred<'a> {
    b: BoxRef<'a>,
    x: f64,
    y: f64,
    order: usize,
    z: i32,
}

struct Walk<'a> {
    deferred: Vec<Deferred<'a>>,
    depth: i32,
    flush: Option<BoxRef<'a>>,
}

impl<'a> Walk<'a> {
    fn is_flush(&self, b: BoxRef<'_>) -> bool {
        self.flush.is_some_and(|f| f.same(b))
    }

    fn flush_deferred(&mut self, mut list: Vec<Deferred<'a>>) -> Option<BoxRef<'a>> {
        merge_sort(&mut list, &|a: &Deferred<'_>, b: &Deferred<'_>| {
            a.z.cmp(&b.z)
                .then_with(|| tree_order_cmp(a.b, b.b, a.order, b.order))
        });
        let saved = self.flush;
        let mut best = None;
        for d in list {
            self.flush = Some(d.b);
            if let Some(m) = self.visit(d.b, d.x, d.y) {
                best = Some(m);
            }
        }
        self.flush = saved;
        best
    }

    fn visit(&mut self, root: BoxRef<'a>, x: f64, y: f64) -> Option<BoxRef<'a>> {
        if self.depth > 0
            && !self.is_flush(root)
            && let Some(z) = positioned_z_index(root).filter(|&z| z >= 0)
        {
            let order = self.deferred.len();
            self.deferred.push(Deferred {
                b: root,
                x,
                y,
                order,
                z,
            });
            return None;
        }
        let (x, y) = enter(root, x, y);
        let (x, y) = untransform(root, x, y)?;
        if outside_paint_span(root, y) {
            return None;
        }
        let clipped = ffi::clips_children(root);
        if !(clipped && !padding_contains(root, x, y)) {
            if ffi::paint_3d_registered(root) {
                if let Some(m) = ffi::paint_3d_pick(root, x, y) {
                    return Some(m);
                }
            } else if let Some(best) = self.visit_children(root, x, y, clipped) {
                return Some(best);
            }
        }
        self_test(root, x, y)
    }

    fn visit_children(
        &mut self,
        root: BoxRef<'a>,
        x: f64,
        y: f64,
        clipped: bool,
    ) -> Option<BoxRef<'a>> {
        let mut best = None;
        let (cx, cy) = scrolled(root, x, y);
        let own_scope = root.parent().is_none()
            || self.is_flush(root)
            || clipped
            || ffi::style_of(root).is_some_and(has_transform);
        let mut saved = Vec::new();
        if own_scope {
            saved = core::mem::take(&mut self.deferred);
            self.depth += 1;
        }
        for c in children_stacked(root) {
            if let Some(m) = self.visit(c, cx, cy) {
                best = Some(m);
            }
        }
        for atomic in root.inline_atomics().unwrap_or(&[]) {
            let Some((ab, ax, ay)) = atomic_point(root, atomic, cx, cy) else {
                continue;
            };
            if let Some(m) = self.visit(ab, ax, ay) {
                best = Some(m);
            }
        }
        if own_scope {
            let mine = core::mem::replace(&mut self.deferred, saved);
            self.depth -= 1;
            if let Some(m) = self.flush_deferred(mine) {
                best = Some(m);
            }
        }
        best
    }
}

fn self_test(root: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    let (x0, y0) = (root.x(), root.y());
    let block_edges = matches!(root.kind(), BoxKind::Block | BoxKind::TableCaption);
    let (m, p, br) = (root.margin(), root.padding(), root.border());
    let x1 = x0
        + root.content_width()
        + if block_edges {
            p.left + p.right + br.left + br.right + m.left + m.right
        } else {
            0.0
        };
    let y1 = y0
        + root.content_height()
        + if block_edges {
            p.top + p.bottom + br.top + br.bottom + m.top + m.bottom
        } else {
            0.0
        };
    if !box_blocks_hit_testing(root)
        && !svg_yields_hit(root)
        && x >= x0
        && x <= x1
        && y >= y0
        && y <= y1
        && !root.dom_ptr().is_null()
    {
        LOCAL.set((x, y));
        return Some(root);
    }
    None
}

fn hit_test_root(root: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    Walk {
        deferred: Vec::new(),
        depth: 0,
        flush: Some(root),
    }
    .visit(root, x, y)
}

fn box_for_dom<'a>(root: BoxRef<'a>, node: *const core::ffi::c_void) -> Option<BoxRef<'a>> {
    if core::ptr::eq(root.dom_ptr(), node) {
        return Some(root);
    }
    children(root).find_map(|c| box_for_dom(c, node))
}

pub fn hit_test(root: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    if let Some(modal) = ffi::active_modal()
        && let Some(top) = box_for_dom(root, modal.as_ptr().cast())
        && !top.same(root)
    {
        let any = hit_test_root(root, x, y);
        let mut n = any.and_then(ffi::dom_of);
        while let Some(node) = n {
            if node == modal {
                return any;
            }
            n = node.parent();
        }
        if let Some(m) = hit_test_root(top, x, y) {
            return Some(m);
        }
    }
    hit_test_root(root, x, y)
}
