//! Southstar — absolutely and fixed positioned boxes: containing blocks, sizing with shrink-to-fit and min/max limits, insets, auto margins and self-alignment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind as NodeKind, Node};
use southstar_layout::{BoxKind, BoxRef};
use southstar_layout_geom::{
    edges_from_style, height_keyword_stretches, length_resolve, resolve_height_with_basis,
    style_creates_fixed_cb, value_is_percent,
};
use southstar_style::{Kind, PropId, StyleRef, StyleTable, ValueRef};

use crate::anchor;
use crate::ffi::{
    self, Area, Entry, box_clips_children, build_abs_box, estimate_natural_width, flat_parent,
    flex_box_is_border_box, grid_abs_containing_block, grid_static_align_offset,
    grid_static_position, inline_containing_block, layout_box, measure_natural_width, min_width_of,
    prop, shift_box_tree, size_keyword_is_intrinsic, static_run, style_of, viewport_h, viewport_w,
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

fn is_frame_owner(n: Node<'_>) -> bool {
    match n.element_name() {
        Some(b"iframe" | b"frame") => true,
        Some(b"object") => core::iter::successors(n.first_child(), |c| c.next_sibling())
            .any(|c| c.kind() == NodeKind::Document),
        _ => false,
    }
}

fn find_abs_containing_block_dom<'n>(n: Node<'n>, styles: StyleTable) -> Option<Node<'n>> {
    flat_ancestors(n)
        .filter(|p| p.kind() == NodeKind::Element)
        .find(|p| is_frame_owner(*p) || style_creates_abs_cb(styles.get(*p)))
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
        .find(|p| is_frame_owner(*p) || style_creates_fixed_cb(styles.get(*p)))
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

struct Placing<'s> {
    area: Area,
    inset: [Option<f64>; 4],
    justify: &'s CStr,
    align: &'s CStr,
    centre: [Option<f64>; 2],
}

fn centred_offset(centre: f64, start: f64, outer: f64, remaining: f64) -> f64 {
    (centre - start - outer / 2.0).min(remaining).max(0.0)
}

fn position_absolute_box_in(abox: BoxRef<'_>, p: &Placing<'_>) {
    let auto = |prop: PropId| length_is_auto(crate::ffi::prop(abox, prop));
    let area = p.area;
    let [top, right, bottom, left] = p.inset;
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
            match p.centre[0] {
                Some(c) => centred_offset(c, area.x + left.unwrap_or(0.0), outer_w, remaining),
                None => grid_static_align_offset(p.justify, remaining),
            }
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
            match p.centre[1] {
                Some(c) => centred_offset(c, area.y + top.unwrap_or(0.0), outer_h, remaining),
                None => grid_static_align_offset(p.align, remaining),
            }
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

fn containing_area(cb: BoxRef<'_>, cb_is_icb: bool) -> Area {
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
    Area {
        x: cb.x() + m.left + e.left,
        y: cb.y() + m.top + e.top,
        w,
        h,
    }
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
    order_root: Node<'a>,
    anchors: anchor::Anchors<'a>,
    styles: StyleTable<'o>,
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
    let mut inline_cb = None;
    let cb = match cb_dom.map(|d| ctx.map.get(d)) {
        Some(Some(b)) => b,
        Some(None) => {
            if e.fixed {
                cb_dom = None;
            } else {
                inline_cb = cb_dom.and_then(|d| inline_containing_block(ctx.root, d));
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
    let cs: Option<StyleRef<'_>> = southstar_style::box_style(cb);
    let grid = if inline_cb.is_some() {
        inline_cb
    } else if !cb_is_icb && e.pseudo.is_none() {
        grid_abs_containing_block(cb, southstar_style::box_style(abox))
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
    let frame = Frame {
        abox,
        cb,
        grid,
        avail,
        cb_h,
        cs,
        static_run: st.is_some(),
        flex_parent,
        static_rtl,
        static_right,
        area: grid.unwrap_or_else(|| containing_area(cb, cb_is_icb)),
    };
    let abox_style = style_of(abox);
    let target = e.dom.downgrade().get();
    let plans = match (abox_style, target) {
        (Some(style), Some(target_node)) if anchor::uses_anchoring(Some(style)) => {
            let doc = Node::ptr_or_null(Some(ctx.order_root)) as usize;
            let mut env = anchor::Env {
                root: ctx.order_root,
                target: target_node,
                pseudo: e.pseudo.is_some(),
                cb_node: cb_dom.and_then(|d| d.downgrade().get()),
                styles,
                map: &ctx.map,
                order: ctx.order,
                anchors: &mut ctx.anchors,
                cb: frame.area,
                font_px: anchor::font_px(abox_style),
            };
            anchor::candidates(&mut env, style, doc)
        }
        _ => Vec::new(),
    };
    if plans.is_empty() {
        southstar_css::anchor::set_placement_flags(e.dom, 0);
        size_and_place(&frame, None);
        return;
    }
    let (x0, y0) = (abox.x(), abox.y());
    let restore = || shift_box_tree(abox, x0 - abox.x(), y0 - abox.y());
    for (k, plan) in plans.iter().enumerate() {
        if k > 0 {
            restore();
        }
        size_and_place(&frame, Some(plan));
        if plans.len() == 1 || fits(abox, plan) {
            southstar_css::anchor::set_placement_flags(e.dom, plan.flags(k > 0));
            return;
        }
    }
    restore();
    southstar_css::anchor::set_placement_flags(e.dom, plans[0].flags(false));
    size_and_place(&frame, Some(&plans[0]));
}

fn fits(abox: BoxRef<'_>, plan: &anchor::Plan) -> bool {
    const EPSILON: f64 = 0.5;
    let imcb = plan.imcb();
    let (w, h) = outer_size(abox);
    abox.x() >= imcb.x - EPSILON
        && abox.y() >= imcb.y - EPSILON
        && abox.x() + w <= imcb.x + imcb.w + EPSILON
        && abox.y() + h <= imcb.y + imcb.h + EPSILON
}

struct Frame<'b> {
    abox: BoxRef<'b>,
    cb: BoxRef<'b>,
    grid: Option<Area>,
    avail: f64,
    cb_h: f64,
    cs: Option<StyleRef<'b>>,
    static_run: bool,
    flex_parent: Option<BoxRef<'b>>,
    static_rtl: bool,
    static_right: f64,
    area: Area,
}

fn size_and_place(f: &Frame<'_>, plan: Option<&anchor::Plan>) {
    let Frame { abox, cb, cs, .. } = *f;
    let grid = plan.map(|p| p.area).or(f.grid);
    let (avail, cb_h) = match (plan, grid) {
        (Some(p), _) => (p.area.w, p.area.h),
        (None, Some(g)) => (g.w, g.h),
        (None, None) => (f.avail, f.cb_h),
    };
    let awv = prop(abox, PropId::Width);
    let alv = prop(abox, PropId::Left);
    let arv = prop(abox, PropId::Right);
    let atv = prop(abox, PropId::Top);
    let abv = prop(abox, PropId::Bottom);
    let placed = |k: usize, v: Option<ValueRef<'_>>, basis: f64| match plan {
        Some(p) => p.inset[k],
        None => inset_or_zero(v, basis),
    };
    let sized = |k: usize, v: Option<ValueRef<'_>>| match plan {
        Some(p) => p.inset[k].is_some(),
        None => is_set(v),
    };
    let left = placed(3, alv, avail);
    let right = placed(1, arv, avail);
    let top = placed(0, atv, cb_h);
    let bottom = placed(2, abv, cb_h);
    let l_set = sized(3, alv);
    let r_set = sized(1, arv);
    let pick = |chosen: Option<&'static CStr>, p: PropId| -> &CStr {
        match chosen.filter(|k| *k != c"normal") {
            Some(k) => k,
            None => self_alignment(abox, p),
        }
    };
    let justify = pick(plan.and_then(|p| p.justify), PropId::JustifySelf);
    let align = pick(plan.and_then(|p| p.align), PropId::AlignSelf);
    let js_stretch = justify == c"stretch";
    let as_stretch = align == c"stretch";
    let planned_w = plan.and_then(|p| p.width);
    let planned_h = plan.and_then(|p| p.height);
    let has_explicit_width = planned_w.is_some() || is_length_or_calc(awv);
    let stretch_w =
        !has_explicit_width && ((l_set && r_set && js_stretch) || height_keyword_stretches(awv));
    let mut layout_w = avail;
    let mut inset_w = avail;
    if l_set && r_set {
        inset_w = avail - left.unwrap_or(0.0) - right.unwrap_or(0.0);
        if inset_w < 0.0 {
            inset_w = 0.0;
        }
    }
    if let Some(w) = planned_w {
        let (m, p, b) = edges_from_style(style_of(abox), avail);
        let extras = if flex_box_is_border_box(abox) {
            0.0
        } else {
            p.left + p.right + b.left + b.right
        };
        layout_w = (w.max(0.0) + extras + m.left + m.right).max(0.0);
    }
    if stretch_w {
        let mut l = if l_set { left.unwrap_or(0.0) } else { 0.0 };
        let r = if r_set { right.unwrap_or(0.0) } else { 0.0 };
        if !l_set && !r_set && f.grid.is_none() {
            let (cm, cbd) = (cb.margin(), cb.border());
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
    let has_explicit_height = planned_h.is_some() || is_length_or_calc(ahv);
    if planned_h.is_none() && has_explicit_height && value_is_percent(ahv) && cb_h > 0.0 {
        let pre_h = resolve_height_with_basis(ahv, avail, cb_h, -1.0);
        if pre_h > 0.0 {
            abox.set_content_height(pre_h);
            abox.set_definite_height(pre_h);
        }
    }
    layout_box(abox, layout_w, cs);
    if let Some(w) = planned_w {
        let content = if flex_box_is_border_box(abox) {
            let (p, b) = (abox.padding(), abox.border());
            w - p.left - p.right - b.left - b.right
        } else {
            w
        };
        abox.set_content_width(content.max(0.0));
    }
    if !stretch_w && !has_explicit_width && abox.kind() == BoxKind::Block {
        let (fm, fp, fb) = edges_from_style(style_of(abox), avail);
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
    let t_set = sized(0, atv);
    let b_set = sized(2, abv);
    let mut inset_h = -1.0;
    if cb_h > 0.0 {
        let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
        inset_h = cb_h
            - if t_set { top.unwrap_or(0.0) } else { 0.0 }
            - if b_set { bottom.unwrap_or(0.0) } else { 0.0 }
            - m.top
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
        let mut explicit_h =
            planned_h.unwrap_or_else(|| resolve_height_with_basis(ahv, avail, cb_h, -1.0));
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
    if let Some(p) = plan
        && p.margin.iter().any(Option::is_some)
    {
        let mut m = abox.margin();
        m.top = p.margin[0].unwrap_or(m.top);
        m.right = p.margin[1].unwrap_or(m.right);
        m.bottom = p.margin[2].unwrap_or(m.bottom);
        m.left = p.margin[3].unwrap_or(m.left);
        abox.set_margin(m);
    }
    let static_x = left.is_none() && right.is_none();
    let static_y = top.is_none() && bottom.is_none();
    if static_x && f.static_rtl && !f.static_run {
        let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
        let outer_w = m.left + b.left + p.left + abox.content_width() + p.right + b.right + m.right;
        shift_box_tree(abox, f.static_right - outer_w - abox.x(), 0.0);
    }
    if let Some(fp) = f.flex_parent
        && (static_x || static_y)
        && let Some((flex_x, flex_y)) = flex_static_position(abox, fp)
    {
        shift_box_tree(
            abox,
            if static_x { flex_x - abox.x() } else { 0.0 },
            if static_y { flex_y - abox.y() } else { 0.0 },
        );
    }
    if let Some(g) = f.grid
        && f.flex_parent.is_some_and(|fp| fp.same(cb))
        && (static_x || static_y)
    {
        grid_static_position(abox, cb, g, static_x, static_y);
    }
    apply_position_offsets(abox, avail, cb_h);
    let area = grid.unwrap_or(f.area);
    position_absolute_box_in(
        abox,
        &Placing {
            area,
            inset: [top, right, bottom, left],
            justify,
            align,
            centre: plan.map_or([None; 2], |p| p.centre),
        },
    );
}

fn precompute_round(ctx: &mut Ctx<'_, '_>, from: usize, to: usize) {
    let mut entries = Vec::new();
    for i in from..to {
        let Some(Some(e)) = ffi::pending_entry(i, ctx.styles) else {
            continue;
        };
        if static_run(e).is_some() {
            continue;
        }
        let cb = match positioned_entry_cb_dom(e, ctx.styles) {
            Some(d) => match ctx.map.get(d) {
                Some(b) => Some(b),
                None => continue,
            },
            None => None,
        };
        entries.push((i, e, cb));
    }
    let mut round = precompute(ctx.root, &entries, ctx.order, to);
    ctx.batch.resize(to, None);
    ctx.batch[from..to].swap_with_slice(&mut round[from..to]);
}

pub fn process_absolute_boxes(root: BoxRef<'_>, styles: StyleTable, viewport_width: f64) {
    let batch_len = ffi::pending_len();
    if batch_len == 0 {
        return;
    }
    let Some(Some(first)) = ffi::pending_entry(0, styles) else {
        ffi::pending_clear();
        return;
    };
    let order_root = southstar_dom::ancestors_and_self(first.dom)
        .last()
        .unwrap_or(first.dom);
    let order = NodeOrder::build(order_root);
    let mut map = BoxMap::default();
    map.add_tree(root);
    let mut ctx = Ctx {
        root,
        order_root,
        anchors: anchor::Anchors::new(),
        styles,
        viewport_width,
        map,
        order: &order,
        batch: Vec::new(),
    };
    precompute_round(&mut ctx, 0, batch_len);
    let mut i = 0;
    while let Some(entry) = ffi::pending_entry(i, styles) {
        if i == ctx.batch.len() {
            precompute_round(&mut ctx, i, ffi::pending_len());
        }
        if let Some(e) = entry {
            place_entry(&mut ctx, i, e);
        }
        i += 1;
    }
    ffi::pending_clear();
}
