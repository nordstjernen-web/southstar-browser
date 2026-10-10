//! Southstar — the static position of an absolutely positioned box: where it would have been laid out in its block, inline, or flex parent.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::collections::{HashMap, HashSet};

use southstar_dom::{Kind as NodeKind, Node};
use southstar_layout::{BoxRef, NsBox};
use southstar_style::{PropId, StyleRef};

use crate::ffi::{
    Entry, dom_of, flex_align_is_baseline, flex_direction_of, flex_item_align, keyword_or,
    style_is_absolute_or_fixed, style_is_flex_container, style_of,
};

pub struct NodeOrder(HashMap<*const c_void, u32>);

impl NodeOrder {
    pub fn build(root: Node<'_>) -> NodeOrder {
        let mut ranks = HashMap::new();
        let mut rank = 0u32;
        let mut stack = vec![root];
        let mut children = Vec::new();
        while let Some(n) = stack.pop() {
            rank += 1;
            ranks.insert(n.as_ptr().cast(), rank);
            children.clear();
            let mut c = n.first_child();
            while let Some(child) = c {
                children.push(child);
                c = child.next_sibling();
            }
            stack.extend(children.iter().rev());
        }
        NodeOrder(ranks)
    }

    pub fn rank(&self, n: *const c_void) -> u32 {
        self.0.get(&n).copied().unwrap_or(0)
    }

    fn precedes(&self, a: Node<'_>, b: Node<'_>) -> bool {
        if a.as_ptr() == b.as_ptr() {
            return false;
        }
        let ra = self.rank(a.as_ptr().cast());
        let rb = self.rank(b.as_ptr().cast());
        if ra != 0 && rb != 0 {
            return ra < rb;
        }
        tree_precedes(a, b)
    }
}

fn depth(n: Node<'_>) -> usize {
    southstar_dom::ancestors_and_self(n).count()
}

fn tree_precedes(a: Node<'_>, b: Node<'_>) -> bool {
    let (mut da, mut db) = (depth(a), depth(b));
    let (mut pa, mut pb) = (a, b);
    while da > db + 1 {
        let Some(p) = pa.parent() else { return false };
        pa = p;
        da -= 1;
    }
    while db > da + 1 {
        let Some(p) = pb.parent() else { return false };
        pb = p;
        db -= 1;
    }
    let same =
        |x: Option<Node<'_>>, y: Option<Node<'_>>| x.map(Node::as_ptr) == y.map(Node::as_ptr);
    if da > db {
        if same(pa.parent(), Some(pb)) {
            return false;
        }
        let Some(p) = pa.parent() else { return false };
        pa = p;
    } else if db > da {
        if same(pb.parent(), Some(pa)) {
            return true;
        }
        let Some(p) = pb.parent() else { return false };
        pb = p;
    }
    while !same(pa.parent(), pb.parent()) {
        let (Some(x), Some(y)) = (pa.parent(), pb.parent()) else {
            return false;
        };
        pa = x;
        pb = y;
    }
    let mut s = pa.next_sibling();
    while let Some(sib) = s {
        if sib.as_ptr() == pb.as_ptr() {
            return true;
        }
        s = sib.next_sibling();
    }
    false
}

#[derive(Default)]
pub struct BoxMap<'a>(HashMap<*const c_void, BoxRef<'a>>);

impl<'a> BoxMap<'a> {
    pub fn add_tree(&mut self, root: BoxRef<'a>) {
        if !root.dom_ptr().is_null() {
            self.0.entry(root.dom_ptr()).or_insert(root);
        }
        let mut c = root.first_child();
        while let Some(child) = c {
            self.add_tree(child);
            c = child.next_sibling();
        }
        for ab in root.inline_atomic_boxes() {
            self.add_tree(ab);
        }
    }

    pub fn get(&self, n: Node<'_>) -> Option<BoxRef<'a>> {
        self.0.get(&n.as_ptr().cast()).copied()
    }
}

fn is_abs_or_fixed(b: BoxRef<'_>) -> bool {
    style_is_absolute_or_fixed(b.style())
}

fn content_top(b: BoxRef<'_>) -> f64 {
    b.y() + b.margin().top + b.border().top + b.padding().top
}

fn content_left(b: BoxRef<'_>) -> f64 {
    b.x() + b.margin().left + b.border().left + b.padding().left
}

fn outer_bottom(b: BoxRef<'_>) -> f64 {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    b.y() + m.top + e.top + p.top + b.content_height() + p.bottom + e.bottom + m.bottom
}

fn is_ancestor_of(a: Node<'_>, n: Node<'_>) -> bool {
    southstar_dom::ancestors(n).any(|p| p.as_ptr() == a.as_ptr())
}

struct Target<'n> {
    node: Node<'n>,
    rank: u32,
    ancestors: HashSet<*const c_void>,
}

fn y_visit_other(
    b: BoxRef<'_>,
    dom: Node<'_>,
    t: &Target<'_>,
    order: &NodeOrder,
    out: &mut f64,
) -> bool {
    let rb = order.rank(b.dom_ptr());
    let ranked = rb != 0 && t.rank != 0;
    let after = if ranked {
        t.rank < rb
    } else {
        order.precedes(t.node, dom)
    };
    if after || is_abs_or_fixed(b) {
        return false;
    }
    let before = if ranked {
        rb < t.rank
    } else {
        order.precedes(dom, t.node)
    };
    if before {
        let bottom = outer_bottom(b);
        if bottom > *out {
            *out = bottom;
        }
    }
    true
}

fn y_visit(b: BoxRef<'_>, t: &Target<'_>, order: &NodeOrder, out: &mut f64) -> bool {
    let Some(dom) = dom_of(b) else { return true };
    if dom.as_ptr() == t.node.as_ptr() {
        return true;
    }
    if !t.ancestors.contains(&b.dom_ptr()) {
        return y_visit_other(b, dom, t, order, out);
    }
    if !is_abs_or_fixed(b) {
        let edge = content_top(b);
        if edge > *out {
            *out = edge;
        }
    }
    true
}

fn y_walk_from(b: BoxRef<'_>, t: &Target<'_>, order: &NodeOrder, out: &mut f64) {
    if !y_visit(b, t, order, out) {
        return;
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        y_walk_from(child, t, order, out);
        c = child.next_sibling();
    }
}

pub fn static_y_walk(cb: BoxRef<'_>, target: Node<'_>, order: &NodeOrder, out: &mut f64) {
    let t = Target {
        node: target,
        rank: order.rank(target.as_ptr().cast()),
        ancestors: southstar_dom::ancestors(target)
            .map(|p| p.as_ptr().cast())
            .collect(),
    };
    y_walk_from(cb, &t, order, out);
}

pub struct StaticX {
    pub x: f64,
    pub rtl: bool,
    pub right: f64,
}

fn is_rtl(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::Direction))
        .is_some_and(|v| v.is_keyword(c"rtl"))
}

pub fn static_x_from_ancestors(
    cb: BoxRef<'_>,
    target: Node<'_>,
    map: &BoxMap<'_>,
    fallback: f64,
) -> StaticX {
    let mut out = StaticX {
        x: fallback,
        rtl: is_rtl(style_of(cb)),
        right: cb.x()
            + cb.margin().left
            + cb.border().left
            + cb.padding().left
            + cb.content_width(),
    };
    let mut best_depth = 0usize;
    for p in southstar_dom::ancestors(target) {
        if p.kind() != NodeKind::Element {
            continue;
        }
        let pb = map.get(p);
        let Some(pb) = pb.filter(|pb| !is_abs_or_fixed(*pb)) else {
            if pb.is_some_and(|pb| pb.same(cb)) {
                break;
            }
            continue;
        };
        let mut inside_cb = false;
        let mut a = Some(pb);
        while let Some(ab) = a {
            if ab.same(cb) {
                inside_cb = true;
                break;
            }
            a = ab.parent();
        }
        if !inside_cb {
            break;
        }
        let depth = southstar_dom::ancestors_and_self(target)
            .take_while(|q| q.as_ptr() != p.as_ptr())
            .count();
        if best_depth == 0 || depth < best_depth {
            out.x = content_left(pb);
            out.right = out.x + pb.content_width();
            out.rtl = is_rtl(style_of(pb));
            best_depth = depth;
        }
        if pb.same(cb) {
            break;
        }
    }
    out
}

struct Calc<'n> {
    entry_index: usize,
    dom: Node<'n>,
    rank: u32,
    y: Option<f64>,
}

fn batch_walk(
    b: BoxRef<'_>,
    calcs: &mut [Calc<'_>],
    next: &mut usize,
    cur_max: &mut f64,
    order: &NodeOrder,
) {
    if *next >= calcs.len() {
        return;
    }
    let dom = dom_of(b);
    if dom.is_some() {
        let rb = order.rank(b.dom_ptr());
        if rb != 0 {
            while *next < calcs.len() && calcs[*next].rank <= rb {
                calcs[*next].y = Some(*cur_max);
                *next += 1;
            }
        }
    }
    let in_flow = dom.is_some() && !is_abs_or_fixed(b);
    if in_flow {
        let top = content_top(b);
        if top > *cur_max {
            *cur_max = top;
        }
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        batch_walk(child, calcs, next, cur_max, order);
        c = child.next_sibling();
    }
    if let Some(dom) = dom {
        while *next < calcs.len() && is_ancestor_of(dom, calcs[*next].dom) {
            calcs[*next].y = Some(*cur_max);
            *next += 1;
        }
    }
    if in_flow {
        let bottom = outer_bottom(b);
        if bottom > *cur_max {
            *cur_max = bottom;
        }
    }
}

pub fn precompute<'a>(
    root: BoxRef<'a>,
    entries: &[(usize, Entry<'_>, Option<BoxRef<'a>>)],
    order: &NodeOrder,
    len: usize,
) -> Vec<Option<f64>> {
    let mut by_cb: HashMap<*const NsBox, (BoxRef<'a>, Vec<Calc<'_>>)> = HashMap::new();
    for &(i, e, cb) in entries {
        let rank = order.rank(e.dom.as_ptr().cast());
        if rank == 0 {
            continue;
        }
        let cb = cb.unwrap_or(root);
        by_cb
            .entry(cb.as_ptr())
            .or_insert_with(|| (cb, Vec::new()))
            .1
            .push(Calc {
                entry_index: i,
                dom: e.dom,
                rank,
                y: None,
            });
    }
    let mut out = vec![None; len];
    for (cb, calcs) in by_cb.values_mut() {
        calcs.sort_by_key(|c| c.rank);
        let mut cur_max = content_top(*cb);
        let mut next = 0;
        batch_walk(*cb, calcs, &mut next, &mut cur_max, order);
        for c in calcs.iter() {
            if let Some(y) = c.y {
                out[c.entry_index] = Some(y);
            }
        }
    }
    out
}

fn eq(a: &CStr, b: &CStr) -> bool {
    a == b
}

fn flex_main_offset(justify: &CStr, free: f64, row: bool, main_reverse: bool, rtl: bool) -> f64 {
    let flipped = main_reverse != (rtl && row);
    let start_is_end = row && rtl;
    let far = |end: bool| if end { free } else { 0.0 };
    if eq(justify, c"start") {
        return far(start_is_end);
    }
    if eq(justify, c"end") {
        return far(!start_is_end);
    }
    if eq(justify, c"left") {
        return if row { 0.0 } else { far(start_is_end) };
    }
    if eq(justify, c"right") {
        return if row { free } else { far(start_is_end) };
    }
    let main = if eq(justify, c"flex-end") {
        free
    } else if eq(justify, c"center") || eq(justify, c"space-around") || eq(justify, c"space-evenly")
    {
        free / 2.0
    } else {
        0.0
    };
    if flipped { free - main } else { main }
}

fn flex_cross_offset(
    align: &CStr,
    free: f64,
    row: bool,
    wrap_reverse: bool,
    rtl: bool,
    self_rtl: bool,
) -> f64 {
    let flipped = wrap_reverse != (rtl && !row);
    let start_is_end = !row && rtl;
    let self_start_is_end = !row && self_rtl;
    let far = |end: bool| if end { free } else { 0.0 };
    if eq(align, c"start") || flex_align_is_baseline(align) {
        return far(start_is_end);
    }
    if eq(align, c"end") || eq(align, c"last baseline") {
        return far(!start_is_end);
    }
    if eq(align, c"self-start") {
        return far(self_start_is_end);
    }
    if eq(align, c"self-end") {
        return far(!self_start_is_end);
    }
    if eq(align, c"left") {
        return if row { far(start_is_end) } else { 0.0 };
    }
    if eq(align, c"right") {
        return if row { far(start_is_end) } else { free };
    }
    let cross = if eq(align, c"flex-end") {
        free
    } else if eq(align, c"center") || eq(align, c"self-center") {
        free / 2.0
    } else {
        0.0
    };
    if flipped { free - cross } else { cross }
}

pub fn abs_flex_parent_box<'a>(dom: Node<'_>, map: &BoxMap<'a>) -> Option<BoxRef<'a>> {
    let p = southstar_dom::ancestors(dom).find(|p| p.kind() == NodeKind::Element)?;
    map.get(p)
}

fn outer_size(b: BoxRef<'_>) -> (f64, f64) {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    (
        b.content_width() + p.left + p.right + e.left + e.right + m.left + m.right,
        b.content_height() + p.top + p.bottom + e.top + e.bottom + m.top + m.bottom,
    )
}

pub fn flex_static_position(abox: BoxRef<'_>, fc: BoxRef<'_>) -> Option<(f64, f64)> {
    let fs = style_of(fc).filter(|s| style_is_flex_container(*s))?;
    let origin_x = content_left(fc);
    let origin_y = content_top(fc);
    let (outer_w, outer_h) = outer_size(abox);
    let dir = flex_direction_of(fs).to_bytes();
    let row = dir.starts_with(b"row");
    let main_reverse = dir.windows(8).any(|w| w == b"-reverse");
    let wrap_reverse = fs
        .get(PropId::FlexWrap)
        .is_some_and(|v| v.is_keyword(c"wrap-reverse"));
    let rtl = is_rtl(Some(fs));
    let (main_size, cross_size) = if row {
        (fc.content_width(), fc.content_height())
    } else {
        (fc.content_height(), fc.content_width())
    };
    let main_free = main_size - if row { outer_w } else { outer_h };
    let cross_free = cross_size - if row { outer_h } else { outer_w };
    let main = flex_main_offset(
        keyword_or(fs, PropId::JustifyContent, c"flex-start"),
        main_free,
        row,
        main_reverse,
        rtl,
    );
    let self_rtl = is_rtl(style_of(abox));
    let cross = flex_cross_offset(
        flex_item_align(abox, keyword_or(fs, PropId::AlignItems, c"stretch")),
        cross_free,
        row,
        wrap_reverse,
        rtl,
        self_rtl,
    );
    Some((
        origin_x + if row { main } else { cross },
        origin_y + if row { cross } else { main },
    ))
}
