//! Southstar — absolutely and fixed positioned boxes: containing blocks, sizing with shrink-to-fit and min/max limits, insets, auto margins and self-alignment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind as NodeKind, Node};
use southstar_layout::{BoxKind, BoxRef, Style};
use southstar_style::{Kind, PropId, StyleRef, StyleTable, ValueRef};

use crate::ffi::{
    self, Area, Entry, box_clips_children, build_abs_box, edges_from_style, estimate_natural_width,
    flat_parent, flex_box_is_border_box, grid_abs_containing_block, grid_static_align_offset,
    grid_static_position, height_keyword_stretches, layout_box, length_resolve,
    measure_natural_width, min_width_of, prop, resolve_height_with_basis, shift_box_tree,
    size_keyword_is_intrinsic, static_run, style_creates_fixed_cb, style_of, value_is_percent,
    viewport_h, viewport_w,
};
use crate::relative::{apply_position_offsets, length_is_auto};
use crate::static_pos::{
    BoxMap, NodeOrder, abs_flex_parent_box, flex_static_position, precompute,
    static_x_from_ancestors, static_y_walk,
};

const MAX_DEPTH: usize = 512;

fn self_alignment(abox: BoxRef<'_>, p: PropId) -> &CStr {
    let mut k = style_of(abox)
        .and_then(|s| s.keyword_of(p))
        .filter(|k| *k != c"auto")
        .unwrap_or(c"normal");
    for prefix in [&b"safe "[..], &b"unsafe "[..]] {
        if k.to_bytes().starts_with(prefix) {
            k = CStr::from_bytes_with_nul(&k.to_bytes_with_nul()[prefix.len()..]).unwrap_or(k);
            break;
        }
    }
    if k == c"normal" {
        let replaced = matches!(abox.kind(), BoxKind::Image | BoxKind::Video | BoxKind::Svg);
        k = if replaced { c"start" } else { c"stretch" };
    }
    k
}

fn style_creates_abs_cb(s: Option<StyleRef<'_>>) -> bool {
    let Some(st) = s else { return false };
    let positioned = st
        .get(PropId::Position)
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| {
            kw == c"relative" || kw == c"absolute" || kw == c"fixed" || kw == c"sticky"
        });
    positioned || style_creates_fixed_cb(s)
}

fn flat_ancestors(n: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(flat_parent(n), |p| flat_parent(*p))
}

fn find_abs_containing_block_dom<'n>(n: Node<'n>, styles: StyleTable) -> Option<Node<'n>> {
    flat_ancestors(n)
        .filter(|p| p.kind() == NodeKind::Element)
        .find(|p| style_creates_abs_cb(styles.get(*p)))
}

fn abs_entry_cb_dom<'n>(e: Entry<'n>, styles: StyleTable) -> Option<Node<'n>> {
    if e.pseudo.is_some() && style_creates_abs_cb(styles.get(e.dom)) {
        return Some(e.dom);
    }
    find_abs_containing_block_dom(e.dom, styles)
}

fn fixed_entry_cb_dom<'n>(e: Entry<'n>, styles: StyleTable) -> Option<Node<'n>> {
    if e.pseudo.is_some() && style_creates_fixed_cb(styles.get(e.dom)) {
        return Some(e.dom);
    }
    flat_ancestors(e.dom)
        .filter(|p| p.kind() == NodeKind::Element)
        .find(|p| style_creates_fixed_cb(styles.get(*p)))
}

fn positioned_entry_cb_dom<'n>(e: Entry<'n>, styles: StyleTable) -> Option<Node<'n>> {
    if e.fixed {
        fixed_entry_cb_dom(e, styles)
    } else {
        abs_entry_cb_dom(e, styles)
    }
}

fn has_transform_style(b: BoxRef<'_>) -> bool {
    let Some(s) = style_of(b) else { return false };
    if s.get(PropId::Transform)
        .and_then(ValueRef::transform)
        .is_some_and(|t| t.n_ops > 0)
    {
        return true;
    }
    s.get(PropId::Translate).is_some()
        || s.get(PropId::Rotate).is_some()
        || s.get(PropId::Scale).is_some()
        || s.get(PropId::Perspective).is_some()
}

fn covers_viewport(b: BoxRef<'_>) -> bool {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    let x = b.x() + m.left;
    let y = b.y() + m.top;
    let w = b.content_width() + p.left + p.right + e.left + e.right;
    let h = b.content_height() + p.top + p.bottom + e.top + e.bottom;
    x <= 0.5 && y <= 0.5 && x + w >= viewport_w() - 0.5 && y + h >= viewport_h() - 0.5
}

fn can_host_fixed(anc: BoxRef<'_>) -> bool {
    let mut b = Some(anc);
    while let Some(bx) = b {
        if bx.parent().is_none() {
            break;
        }
        if has_transform_style(bx) || (box_clips_children(bx) && !covers_viewport(bx)) {
            return false;
        }
        b = bx.parent();
    }
    true
}

fn outer_size(b: BoxRef<'_>) -> (f64, f64) {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    (
        b.content_width() + p.left + p.right + e.left + e.right + m.left + m.right,
        b.content_height() + p.top + p.bottom + e.top + e.bottom + m.top + m.bottom,
    )
}

fn inset_or_zero(v: Option<ValueRef<'_>>, basis: f64) -> Option<f64> {
    (v.is_some() && !length_is_auto(v)).then(|| length_resolve(v, basis, 0.0))
}

fn place_axis(
    start: Option<f64>,
    end: Option<f64>,
    auto_margins: (bool, bool),
    cb_size: f64,
    outer: f64,
    centre_clamps: bool,
    align: impl FnOnce() -> f64,
) -> Option<f64> {
    match (start, end) {
        (Some(s), Some(e)) => {
            let remaining = cb_size - s - e - outer;
            let offset = match auto_margins {
                (true, true) if centre_clamps => {
                    if remaining > 0.0 {
                        remaining / 2.0
                    } else {
                        0.0
                    }
                }
                (true, true) => remaining / 2.0,
                (true, false) => remaining,
                (false, true) => 0.0,
                (false, false) => align(),
            };
            Some(s + offset)
        }
        (Some(s), None) => Some(s),
        (None, Some(e)) => Some(cb_size - e - outer),
        (None, None) => None,
    }
}

fn position_absolute_box_in(abox: BoxRef<'_>, area: Area) {
    let auto = |p: PropId| length_is_auto(prop(abox, p));
    let left = inset_or_zero(prop(abox, PropId::Left), area.w);
    let right = inset_or_zero(prop(abox, PropId::Right), area.w);
    let top = inset_or_zero(prop(abox, PropId::Top), area.h);
    let bottom = inset_or_zero(prop(abox, PropId::Bottom), area.h);
    let (outer_w, outer_h) = outer_size(abox);
    let final_x = place_axis(
        left,
        right,
        (auto(PropId::MarginLeft), auto(PropId::MarginRight)),
        area.w,
        outer_w,
        true,
        || {
            let remaining = area.w - left.unwrap_or(0.0) - right.unwrap_or(0.0) - outer_w;
            grid_static_align_offset(self_alignment(abox, PropId::JustifySelf), remaining)
        },
    )
    .map_or(abox.x(), |x| area.x + x);
    let final_y = place_axis(
        top,
        bottom,
        (auto(PropId::MarginTop), auto(PropId::MarginBottom)),
        area.h,
        outer_h,
        false,
        || {
            let remaining = area.h - top.unwrap_or(0.0) - bottom.unwrap_or(0.0) - outer_h;
            grid_static_align_offset(self_alignment(abox, PropId::AlignSelf), remaining)
        },
    )
    .map_or(abox.y(), |y| area.y + y);
    let dx = final_x - abox.x();
    let dy = final_y - abox.y();
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    shift_box_tree(abox, dx, dy);
}

fn position_absolute_box(abox: BoxRef<'_>, cb: BoxRef<'_>, cb_is_icb: bool) {
    let (m, p, e) = (cb.margin(), cb.padding(), cb.border());
    let w = if cb_is_icb {
        viewport_w()
    } else {
        cb.content_width() + p.left + p.right
    };
    let h = if cb_is_icb {
        viewport_h()
    } else {
        cb.content_height() + p.top + p.bottom
    };
    let area = Area {
        x: cb.x() + m.left + e.left,
        y: cb.y() + m.top + e.top,
        w,
        h,
    };
    position_absolute_box_in(abox, area);
}

fn height_limit(
    abox: BoxRef<'_>,
    v: Option<ValueRef<'_>>,
    width_basis: f64,
    cb_h: f64,
    inset_h: f64,
    sizing_extras: f64,
) -> f64 {
    let mut limit = resolve_height_with_basis(v, width_basis, cb_h, -1.0);
    if limit < 0.0 && size_keyword_is_intrinsic(v) && abox.measured_content_height() >= 0.0 {
        limit = abox.measured_content_height() + sizing_extras;
    }
    if limit < 0.0 && height_keyword_stretches(v) && inset_h >= 0.0 {
        limit = inset_h + sizing_extras;
    }
    limit
}

fn height_within_limits(
    abox: BoxRef<'_>,
    h: f64,
    width_basis: f64,
    cb_h: f64,
    inset_h: f64,
) -> f64 {
    let Some(s) = style_of(abox) else { return h };
    let sizing_extras = if flex_box_is_border_box(abox) {
        let (p, e) = (abox.padding(), abox.border());
        p.top + p.bottom + e.top + e.bottom
    } else {
        0.0
    };
    let mut h = h;
    let mx = height_limit(
        abox,
        s.get(PropId::MaxHeight),
        width_basis,
        cb_h,
        inset_h,
        sizing_extras,
    );
    if mx >= 0.0 && h > mx - sizing_extras {
        h = if mx > sizing_extras {
            mx - sizing_extras
        } else {
            0.0
        };
    }
    let mn = height_limit(
        abox,
        s.get(PropId::MinHeight),
        width_basis,
        cb_h,
        inset_h,
        sizing_extras,
    );
    if mn >= 0.0 && h < mn - sizing_extras {
        h = mn - sizing_extras;
    }
    h
}

fn is_set(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| !length_is_auto(Some(v)) && matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

struct Ctx<'a, 'o> {
    root: BoxRef<'a>,
    styles: StyleTable,
    viewport_width: f64,
    map: BoxMap<'a>,
    order: &'o NodeOrder,
    batch: Vec<Option<f64>>,
}

fn depth_exceeds(b: BoxRef<'_>) -> bool {
    let mut depth = 0;
    let mut p = Some(b);
    while let Some(pb) = p {
        depth += 1;
        if depth >= MAX_DEPTH {
            return true;
        }
        p = pb.parent();
    }
    false
}

fn place_entry<'a>(ctx: &mut Ctx<'a, '_>, i: usize, e: Entry<'_>) {
    let styles = ctx.styles;
    let mut cb_dom = positioned_entry_cb_dom(e, styles);
    let cb = match cb_dom.map(|d| ctx.map.get(d)) {
        Some(Some(b)) => b,
        Some(None) => {
            if e.fixed {
                cb_dom = None;
            }
            ctx.root
        }
        None => ctx.root,
    };
    let mut paint_parent = cb;
    if e.fixed
        && cb_dom.is_none()
        && let Some(anc) = abs_entry_cb_dom(e, styles).and_then(|d| ctx.map.get(d))
        && can_host_fixed(anc)
    {
        paint_parent = anc;
    }
    if depth_exceeds(paint_parent) {
        return;
    }
    let Some(abox) = build_abs_box(e, styles) else {
        return;
    };
    ffi::append_child(paint_parent, abox);
    ctx.map.add_tree(abox);
    let cb_is_icb = cb_dom.is_none();
    let (cp, cbd, cm) = (cb.padding(), cb.border(), cb.margin());
    let cb_pad_w = cb.content_width() + cp.left + cp.right;
    let cb_pad_h = cb.content_height() + cp.top + cp.bottom;
    let mut avail = if cb_is_icb || cb_pad_w.is_nan() || cb_pad_w <= 0.0 {
        ctx.viewport_width
    } else {
        cb_pad_w
    };
    let mut cb_h = if cb_is_icb { viewport_h() } else { cb_pad_h };
    let cs: *const Style = cb.style();
    let grid = if !cb_is_icb && e.pseudo.is_none() {
        grid_abs_containing_block(cb, abox.style())
    } else {
        None
    };
    if let Some(g) = grid {
        avail = g.w;
        cb_h = g.h;
    }
    let st = static_run(e);
    let flex_parent = if e.pseudo.is_some() {
        None
    } else {
        abs_flex_parent_box(e.dom, &ctx.map)
    };
    let mut static_rtl = false;
    let mut static_right = 0.0;
    if let Some((run, rel_x, rel_y)) = st {
        abox.set_x(run.x() + rel_x);
        abox.set_y(run.y() + rel_y);
    } else {
        let mut static_y = cb.y() + cm.top + cbd.top + cp.top;
        match ctx.batch.get(i).copied().flatten() {
            Some(y) => static_y = y,
            None => static_y_walk(cb, e.dom, ctx.order, &mut static_y),
        }
        let base_x = cb.x() + cm.left + cbd.left + cp.left;
        let sx = static_x_from_ancestors(cb, e.dom, &ctx.map, base_x);
        static_rtl = sx.rtl;
        static_right = sx.right;
        abox.set_x(sx.x);
        abox.set_y(static_y);
    }
    let awv = prop(abox, PropId::Width);
    let has_explicit_width = is_length_or_calc(awv);
    let alv = prop(abox, PropId::Left);
    let arv = prop(abox, PropId::Right);
    let atv = prop(abox, PropId::Top);
    let abv = prop(abox, PropId::Bottom);
    let l_set = is_set(alv);
    let r_set = is_set(arv);
    let js_stretch = self_alignment(abox, PropId::JustifySelf) == c"stretch";
    let as_stretch = self_alignment(abox, PropId::AlignSelf) == c"stretch";
    let stretch_w =
        !has_explicit_width && ((l_set && r_set && js_stretch) || height_keyword_stretches(awv));
    let mut layout_w = avail;
    let mut inset_w = avail;
    if l_set && r_set {
        inset_w = avail - length_resolve(alv, avail, 0.0) - length_resolve(arv, avail, 0.0);
        if inset_w < 0.0 {
            inset_w = 0.0;
        }
    }
    if stretch_w {
        let mut l = if l_set {
            length_resolve(alv, avail, 0.0)
        } else {
            0.0
        };
        let r = if r_set {
            length_resolve(arv, avail, 0.0)
        } else {
            0.0
        };
        if !l_set && !r_set && grid.is_none() {
            let origin = cb.x() + cm.left + cbd.left;
            if abox.x() > origin {
                l = abox.x() - origin;
            }
        }
        let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
        layout_w = avail - l - r - m.left - m.right - b.left - b.right - p.left - p.right;
        if layout_w < 0.0 {
            layout_w = 0.0;
        }
    }
    let ahv = prop(abox, PropId::Height);
    let has_explicit_height = is_length_or_calc(ahv);
    if has_explicit_height && value_is_percent(ahv) && cb_h > 0.0 {
        let pre_h = resolve_height_with_basis(ahv, avail, cb_h, -1.0);
        if pre_h > 0.0 {
            abox.set_content_height(pre_h);
            abox.set_definite_height(pre_h);
        }
    }
    layout_box(abox, layout_w, cs);
    if !stretch_w && !has_explicit_width && abox.kind() == BoxKind::Block {
        let (fm, fp, fb) = edges_from_style(abox.style(), avail);
        let box_extras = fp.left + fp.right + fb.left + fb.right;
        let outer_extras = box_extras + fm.left + fm.right;
        let mut fit = measure_natural_width(abox, cs);
        if fit.is_nan() || fit <= 0.0 {
            fit = estimate_natural_width(abox, inset_w) - box_extras;
        }
        let mut floor_w = min_width_of(abox, cs);
        if fit < floor_w {
            fit = floor_w;
        }
        fit += outer_extras;
        floor_w += outer_extras;
        let fit_w = if fit < inset_w {
            fit
        } else if floor_w > inset_w {
            floor_w
        } else {
            inset_w
        };
        if fit_w != layout_w {
            layout_w = fit_w;
            layout_box(abox, layout_w, cs);
        }
    }
    let t_set = is_set(atv);
    let b_set = is_set(abv);
    let mut inset_h = -1.0;
    if cb_h > 0.0 {
        let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
        inset_h =
            cb_h - if t_set {
                length_resolve(atv, cb_h, 0.0)
            } else {
                0.0
            } - if b_set {
                length_resolve(abv, cb_h, 0.0)
            } else {
                0.0
            } - m.top
                - m.bottom
                - b.top
                - b.bottom
                - p.top
                - p.bottom;
        if inset_h < 0.0 {
            inset_h = 0.0;
        }
    }
    if has_explicit_height {
        let mut explicit_h = resolve_height_with_basis(ahv, avail, cb_h, -1.0);
        if explicit_h >= 0.0 {
            if flex_box_is_border_box(abox) {
                let (p, b) = (abox.padding(), abox.border());
                explicit_h -= p.top + p.bottom + b.top + b.bottom;
                if explicit_h < 0.0 {
                    explicit_h = 0.0;
                }
            }
            abox.set_content_height(height_within_limits(abox, explicit_h, avail, cb_h, inset_h));
        }
    }
    let intrinsic_height = ahv
        .and_then(ValueRef::keyword_text)
        .is_some_and(|k| k == c"fit-content" || k == c"min-content" || k == c"max-content");
    if !has_explicit_height && !intrinsic_height && t_set && b_set && cb_h > 0.0 && as_stretch {
        let h = height_within_limits(abox, inset_h, avail, cb_h, inset_h);
        abox.set_content_height(h);
        if h >= 0.0 {
            layout_box(abox, layout_w, cs);
            abox.set_content_height(h);
        }
    }
    let static_x = (alv.is_none() || length_is_auto(alv)) && (arv.is_none() || length_is_auto(arv));
    let static_y = (atv.is_none() || length_is_auto(atv)) && (abv.is_none() || length_is_auto(abv));
    if static_x && static_rtl && st.is_none() {
        let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
        let outer_w = m.left + b.left + p.left + abox.content_width() + p.right + b.right + m.right;
        shift_box_tree(abox, static_right - outer_w - abox.x(), 0.0);
    }
    if let Some(fp) = flex_parent
        && (static_x || static_y)
        && let Some((flex_x, flex_y)) = flex_static_position(abox, fp)
    {
        shift_box_tree(
            abox,
            if static_x { flex_x - abox.x() } else { 0.0 },
            if static_y { flex_y - abox.y() } else { 0.0 },
        );
    }
    if let Some(g) = grid
        && flex_parent.is_some_and(|fp| fp.same(cb))
        && (static_x || static_y)
    {
        grid_static_position(abox, cb, g, static_x, static_y);
    }
    apply_position_offsets(abox, avail, cb_h);
    match grid {
        Some(g) => position_absolute_box_in(abox, g),
        None => position_absolute_box(abox, cb, cb_is_icb),
    }
}

pub fn process_absolute_boxes(root: BoxRef<'_>, styles: StyleTable, viewport_width: f64) {
    let batch_len = ffi::pending_len();
    if batch_len == 0 {
        return;
    }
    let Some(Some(first)) = ffi::pending_entry(0) else {
        ffi::pending_clear();
        return;
    };
    let order_root = southstar_dom::ancestors_and_self(first.dom)
        .last()
        .unwrap_or(first.dom);
    let order = NodeOrder::build(order_root);
    let mut map = BoxMap::default();
    map.add_tree(root);
    let mut entries = Vec::new();
    for i in 0..batch_len {
        let Some(Some(e)) = ffi::pending_entry(i) else {
            continue;
        };
        if static_run(e).is_some() {
            continue;
        }
        let cb = match positioned_entry_cb_dom(e, styles) {
            Some(d) => match map.get(d) {
                Some(b) => Some(b),
                None => continue,
            },
            None => None,
        };
        entries.push((i, e, cb));
    }
    let batch = precompute(root, &entries, &order, batch_len);
    let mut ctx = Ctx {
        root,
        styles,
        viewport_width,
        map,
        order: &order,
        batch,
    };
    let mut i = 0;
    while let Some(entry) = ffi::pending_entry(i) {
        if let Some(e) = entry {
            place_entry(&mut ctx, i, e);
        }
        i += 1;
    }
    ffi::pending_clear();
}
