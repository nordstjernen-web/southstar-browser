//! Southstar — column flex containers: item heights as flex base sizes, column wrapping, cross placement, justify-content and resizing items to their flexed heights.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::{BoxKind, BoxRef, Style};
use southstar_style::{Kind, PropId};

use crate::Frame;
use crate::ffi::{
    aspect_ratio_number, box_is_scroll_container, box_read_definite_height, edges_from_style,
    flex_gap_of, flex_grow_of, flex_shrink_of, flex_wraps, gap_px, height_keyword_stretches,
    keyword_or, layout_box, length_resolve, measure_natural_width, overflow_scrolls,
    resolve_used_height, shift_box_tree, size_keyword_is_intrinsic, style_is_absolute_or_fixed,
    style_of, value_is_percent,
};
use crate::items::{
    self, FlexLen, Item, align_content_offsets, basis_main_height, clamp_main, has_auto,
    is_keyword, is_length, is_replaced_like, is_rtl, justify_offsets, main_height_outer, max,
    outer_width, resolve_lengths, value, vertical_extras,
};

struct ColLine {
    start: usize,
    count: usize,
    cross: f64,
    x: f64,
}

fn content_plus_vertical(c: BoxRef<'_>, content: f64) -> f64 {
    let (p, bd) = (c.padding(), c.border());
    content + p.top + p.bottom + bd.top + bd.bottom
}

fn stretch_main_height(c: BoxRef<'_>, container_main_size: f64) -> f64 {
    if container_main_size < 0.0 {
        return -1.0;
    }
    let m = c.margin();
    let h = container_main_size - m.top - m.bottom;
    if h > 0.0 { h } else { 0.0 }
}

fn measured_or_content(c: BoxRef<'_>) -> f64 {
    if c.measured_content_height() >= 0.0 {
        c.measured_content_height()
    } else {
        c.content_height()
    }
}

fn min_main_height(c: BoxRef<'_>, cw: f64, pct_basis: f64) -> f64 {
    let vextra = vertical_extras(c);
    let mnh = value(c, PropId::MinHeight);
    if is_length(mnh) {
        if value_is_percent(mnh) && pct_basis < 0.0 {
            return 0.0;
        }
        let mn = main_height_outer(c, mnh, cw, pct_basis);
        return if mn > 0.0 { mn } else { 0.0 };
    }
    let mut content = measured_or_content(c) + vextra;
    if mnh.is_some_and(|v| v.kind() == Kind::Keyword) && !is_keyword(mnh, c"auto") {
        if size_keyword_is_intrinsic(mnh) {
            return if content > 0.0 { content } else { 0.0 };
        }
        let stretch = stretch_main_height(c, pct_basis);
        return if stretch > 0.0 { stretch } else { 0.0 };
    }
    if box_is_scroll_container(c) {
        return 0.0;
    }
    let hv = value(c, PropId::Height);
    if is_length(hv) && !(value_is_percent(hv) && pct_basis < 0.0) {
        let specified = if value_is_percent(hv) && is_replaced_like(c) {
            main_height_outer(c, hv, cw, 0.0)
        } else {
            main_height_outer(c, hv, cw, pct_basis)
        };
        if specified >= 0.0 && specified < content {
            content = specified;
        }
    }
    if content > 0.0 { content } else { 0.0 }
}

fn max_main_height(c: BoxRef<'_>, cw: f64, pct_basis: f64) -> f64 {
    let mxh = value(c, PropId::MaxHeight);
    if size_keyword_is_intrinsic(mxh) {
        return content_plus_vertical(c, measured_or_content(c));
    }
    if height_keyword_stretches(mxh) {
        return stretch_main_height(c, pct_basis);
    }
    if !is_length(mxh) {
        return -1.0;
    }
    if value_is_percent(mxh) && pct_basis < 0.0 {
        return -1.0;
    }
    main_height_outer(c, mxh, cw, pct_basis)
}

fn item_align<'a>(c: BoxRef<'a>, align: &'a CStr) -> &'a CStr {
    style_of(c)
        .and_then(|s| s.keyword_of(PropId::AlignSelf))
        .filter(|a| *a != c"auto")
        .unwrap_or(align)
}

fn shrinks_to_fit(c: BoxRef<'_>, align: &CStr) -> bool {
    !matches!(item_align(c, align).to_bytes(), b"stretch" | b"normal")
}

fn stretch_replaced_width(c: BoxRef<'_>, line_w: f64) {
    if !matches!(c.kind(), BoxKind::Image | BoxKind::Video | BoxKind::Svg) {
        return;
    }
    let Some(s) = style_of(c) else { return };
    if is_keyword(s.get(PropId::MarginLeft), c"auto")
        || is_keyword(s.get(PropId::MarginRight), c"auto")
    {
        return;
    }
    let (m, p, bd) = (c.margin(), c.padding(), c.border());
    let hextra = p.left + p.right + bd.left + bd.right;
    let mut w = line_w - m.left - m.right - hextra;
    let border_box = is_keyword(s.get(PropId::BoxSizing), c"border-box");
    let mut max_w = length_resolve(s.get(PropId::MaxWidth), line_w, -1.0);
    let mut min_w = length_resolve(s.get(PropId::MinWidth), line_w, -1.0);
    if border_box {
        if max_w >= 0.0 {
            max_w = max(max_w - hextra, 0.0);
        }
        if min_w >= 0.0 {
            min_w = max(min_w - hextra, 0.0);
        }
    }
    if max_w >= 0.0 && w > max_w {
        w = max_w;
    }
    if min_w >= 0.0 && w < min_w {
        w = min_w;
    }
    if w < 0.0 {
        w = 0.0;
    }
    let height_auto = !is_length(s.get(PropId::Height));
    if height_auto && c.content_width() > 0.0 && c.content_height() > 0.0 {
        c.set_content_height(w * c.content_height() / c.content_width());
    }
    c.set_content_width(w);
}

fn layout_item(c: BoxRef<'_>, line_w: f64, fit: bool, child_inherited: *const Style) {
    let width_explicit = is_length(value(c, PropId::Width));
    let mut parent_w = line_w;
    if fit && !width_explicit {
        let mut nat = measure_natural_width(c, child_inherited);
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let avail = line_w - m.left - m.right - p.left - p.right - bd.left - bd.right;
        if nat > avail {
            nat = avail;
        }
        if nat < 0.0 {
            nat = 0.0;
        }
        parent_w = nat + m.left + m.right + p.left + p.right + bd.left + bd.right;
    }
    c.set_definite_height(0.0);
    c.set_measured_content_height(-1.0);
    layout_box(c, parent_w, child_inherited);
    if !fit && !width_explicit {
        stretch_replaced_width(c, line_w);
    }
}

struct MainLimits {
    explicit: f64,
    percentage_basis: f64,
    min: f64,
    max: f64,
    row_gap: f64,
}

fn main_limits(b: BoxRef<'_>, cw: f64, parent_content_height: f64) -> MainLimits {
    let s = style_of(b);
    let get = |prop| s.and_then(|s| s.get(prop));
    let hv = get(PropId::Height);
    let mut explicit = -1.0;
    if is_length(hv) {
        explicit = resolve_used_height(b, hv, cw, -1.0);
    }
    if explicit < 0.0
        && s.is_some()
        && style_is_absolute_or_fixed(b.style())
        && b.content_height() > 0.0
        && (hv.is_some() || (get(PropId::Top).is_some() && get(PropId::Bottom).is_some()))
    {
        explicit = b.content_height();
    }
    if explicit < 0.0 && s.is_some() {
        let ratio = aspect_ratio_number(get(PropId::AspectRatio));
        if ratio > 0.0 && cw > 0.0 {
            explicit = cw / ratio;
        }
    }
    if explicit < 0.0 && box_read_definite_height(b) > 0.0 {
        explicit = b.definite_height();
    }
    if explicit > 0.0 {
        b.set_definite_height(explicit);
    }
    let row_gap = s.map_or(0.0, |s| {
        gap_px(
            s.get(PropId::RowGap),
            s.get(PropId::Gap),
            if explicit > 0.0 { explicit } else { 0.0 },
        )
    });
    let mut min = resolve_used_height(b, get(PropId::MinHeight), parent_content_height, -1.0);
    let mut max_h = resolve_used_height(b, get(PropId::MaxHeight), parent_content_height, -1.0);
    if is_keyword(get(PropId::BoxSizing), c"border-box") {
        let (bd, pd) = (b.border(), b.padding());
        let vex = bd.top + bd.bottom + pd.top + pd.bottom;
        if explicit > 0.0 {
            explicit = max(explicit - vex, 0.0);
        }
        if min > 0.0 {
            min = max(min - vex, 0.0);
        }
        if max_h >= 0.0 {
            max_h = max(max_h - vex, 0.0);
        }
    }
    let percentage_basis = explicit;
    if explicit >= 0.0 && min > explicit {
        explicit = min;
    }
    if max_h >= 0.0 && explicit > max_h {
        explicit = max_h;
    }
    MainLimits {
        explicit,
        percentage_basis,
        min,
        max: max_h,
        row_gap,
    }
}

fn break_lines(
    list: &[Item<'_>],
    lines: &mut Vec<ColLine>,
    inner_x: f64,
    row_gap: f64,
    wraps: bool,
    line_limit: f64,
) {
    let n = list.len();
    let mut i = 0;
    while i < n {
        let mut ln = ColLine {
            start: i,
            count: 0,
            cross: 0.0,
            x: inner_x,
        };
        let mut used = 0.0;
        while i < n {
            let c = list[i].b;
            let m = c.margin();
            let outer = list[i].len.hypothetical() + m.top + m.bottom;
            let lead_gap = if ln.count > 0 { row_gap } else { 0.0 };
            let try_used = used + lead_gap + outer;
            if wraps && try_used > line_limit + 0.5 && ln.count > 0 {
                break;
            }
            used = try_used;
            ln.count += 1;
            let w = outer_width(c);
            if w > ln.cross {
                ln.cross = w;
            }
            i += 1;
        }
        let empty = ln.count == 0;
        lines.push(ln);
        if empty {
            break;
        }
    }
}

fn line_avail(line: &[Item<'_>], explicit: f64, margins: f64, gaps: f64, sum_hyp: f64) -> f64 {
    let mut avail = if explicit > 0.0 {
        explicit
    } else {
        sum_hyp + margins + gaps
    };
    if explicit <= 0.0 {
        let mut fraction = 0.0;
        for it in line {
            let l = &it.len;
            let diff = it.extra - l.basis;
            let f = if diff > 0.0 {
                diff / max(l.grow, 1.0)
            } else if l.shrink * l.basis > 0.0 {
                diff / (l.shrink * l.basis)
            } else {
                0.0
            };
            if f > fraction {
                fraction = f;
            }
        }
        let mut sum = margins + gaps;
        for it in line {
            let l = &it.len;
            let size = l.basis + fraction * l.grow;
            sum += clamp_main(size, l.min, l.max);
        }
        if sum > avail {
            avail = sum;
        }
    }
    avail
}

pub fn layout(b: BoxRef<'_>, at: &Frame, parent_content_height: f64) -> f64 {
    let Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse,
    } = *at;
    let mut list: Vec<Item<'_>> = Vec::new();
    items::collect(b, &mut list);

    let align = keyword_or(b.style(), PropId::AlignItems, c"stretch");
    let justify = keyword_or(b.style(), PropId::JustifyContent, c"flex-start");
    let limits = main_limits(b, cw, parent_content_height);
    let explicit_h = limits.explicit;
    let row_gap = limits.row_gap;
    let col_gap = flex_gap_of(b.style(), cw);
    let line_limit = if explicit_h > 0.0 {
        explicit_h
    } else {
        limits.max
    };
    let multi_line = flex_wraps(b.style());
    let wraps = multi_line && line_limit > 0.0;
    let wrap_reverse = is_keyword(value(b, PropId::FlexWrap), c"wrap-reverse");
    let rtl = is_rtl(b);
    let cross_start_right = wrap_reverse != rtl;

    for it in list.iter_mut() {
        let c = it.b;
        edges_from_style(c, cw);
        c.set_x(inner_x);
        c.set_y(inner_y);
        layout_item(
            c,
            cw,
            multi_line || shrinks_to_fit(c, align),
            child_inherited,
        );
        let explicit_basis = basis_main_height(c, cw, limits.percentage_basis);
        let basis = explicit_basis.unwrap_or_else(|| content_plus_vertical(c, c.content_height()));
        it.len = FlexLen {
            basis,
            min: min_main_height(c, cw, limits.percentage_basis),
            max: max_main_height(c, cw, limits.percentage_basis),
            grow: flex_grow_of(c),
            shrink: flex_shrink_of(c),
            ..FlexLen::default()
        };
        let mut contrib = content_plus_vertical(c, c.content_height());
        if contrib < basis && explicit_basis.is_some() {
            contrib = basis;
        }
        it.extra = contrib;
    }

    let mut lines: Vec<ColLine> = Vec::new();
    break_lines(&list, &mut lines, inner_x, row_gap, wraps, line_limit);

    let mut lines_cross = 0.0;
    for ln in &lines {
        lines_cross += ln.cross;
    }
    if lines.len() > 1 {
        lines_cross += col_gap * (lines.len() - 1) as f64;
    }
    if lines.is_empty() {
        lines.push(ColLine {
            start: 0,
            count: 0,
            cross: cw,
            x: inner_x,
        });
    }
    if !multi_line {
        lines[0].cross = cw;
    } else {
        let offsets = align_content_offsets(b, cw - lines_cross, lines.len());
        let mut x = inner_x + offsets.lead;
        for ln in lines.iter_mut() {
            ln.cross += offsets.per_line;
            ln.x = x;
            x += ln.cross + col_gap + offsets.between;
        }
        if cross_start_right {
            for ln in lines.iter_mut() {
                ln.x = inner_x + cw - (ln.x - inner_x) - ln.cross;
            }
        }
    }

    let mut main_extent = 0.0;
    for ln in &lines {
        let line = &mut list[ln.start..ln.start + ln.count];
        let gaps = if ln.count > 1 {
            row_gap * (ln.count - 1) as f64
        } else {
            0.0
        };
        let mut margins = 0.0;
        let mut sum_hyp = 0.0;
        let mut auto_margins = 0;
        for it in line.iter() {
            let m = it.b.margin();
            margins += m.top + m.bottom;
            sum_hyp += it.len.hypothetical();
            auto_margins += i32::from(has_auto(it.b, PropId::MarginTop));
            auto_margins += i32::from(has_auto(it.b, PropId::MarginBottom));
        }
        let mut avail = line_avail(line, explicit_h, margins, gaps, sum_hyp);
        if limits.max >= 0.0 && avail > limits.max {
            avail = limits.max;
        }
        if avail < limits.min {
            avail = limits.min;
        }
        resolve_lengths(line, avail - margins - gaps);
        let mut free_main = avail - margins - gaps;
        for it in line.iter() {
            free_main -= it.len.target;
        }
        let (leading, between) = if auto_margins > 0 && free_main > 0.0 {
            let share = free_main / f64::from(auto_margins);
            for it in line.iter() {
                if it.b.style().is_null() {
                    continue;
                }
                let mut m = it.b.margin();
                if has_auto(it.b, PropId::MarginTop) {
                    m.top += share;
                }
                if has_auto(it.b, PropId::MarginBottom) {
                    m.bottom += share;
                }
                it.b.set_margin(m);
            }
            (0.0, 0.0)
        } else {
            justify_offsets(b, justify, free_main, ln.count, reverse)
        };
        if avail > main_extent {
            main_extent = avail;
        }

        let mut cursor_y = if reverse {
            inner_y + avail - leading
        } else {
            inner_y + leading
        };
        for it in line.iter() {
            let c = it.b;
            let main_size = it.len.target;
            let vextra = vertical_extras(c);
            let ml_auto = has_auto(c, PropId::MarginLeft);
            let mr_auto = has_auto(c, PropId::MarginRight);
            let width_explicit = is_length(value(c, PropId::Width));
            let eff_align = item_align(c, align);
            let eff = eff_align.to_bytes();
            let stretches =
                !ml_auto && !mr_auto && !width_explicit && matches!(eff, b"stretch" | b"normal");
            let centered = eff == b"center";
            let at_right = match eff {
                b"flex-end" => !cross_start_right,
                b"end" | b"self-end" => !rtl,
                b"start" | b"self-start" => rtl,
                b"right" => true,
                b"left" => false,
                _ => !centered && cross_start_right,
            };
            if (stretches || ml_auto || mr_auto) && (outer_width(c) - ln.cross).abs() > 0.01 {
                c.set_x(inner_x);
                c.set_y(inner_y);
                layout_item(c, ln.cross, false, child_inherited);
            }
            let item_outer_w = outer_width(c);
            let cx = if ml_auto || mr_auto {
                ln.x
            } else if centered {
                ln.x + (ln.cross - item_outer_w) / 2.0
            } else if at_right {
                ln.x + ln.cross - item_outer_w
            } else {
                ln.x
            };
            let m = c.margin();
            let outer_main = main_size + m.top + m.bottom;
            if reverse {
                cursor_y -= outer_main;
            }
            let dx = cx - c.x();
            let dy = cursor_y - c.y();
            if dx != 0.0 || dy != 0.0 {
                shift_box_tree(c, dx, dy);
            }
            c.set_x(cx);
            c.set_y(cursor_y);

            let mut target_h = main_size - vextra;
            if target_h < 0.0 {
                target_h = 0.0;
            }
            if (target_h - c.content_height()).abs() > 0.01 {
                let natural_h = c.content_height();
                let shrank = target_h < natural_h;
                c.set_content_height(target_h);
                if shrank && !c.style().is_null() && overflow_scrolls(c.style(), PropId::OverflowY)
                {
                    let mut max_y = natural_h - target_h;
                    if max_y < 0.0 {
                        max_y = 0.0;
                    }
                    c.set_scroll_overflow_y(max_y);
                }
                if c.first_child().is_some() && c.definite_height() != target_h {
                    let relayout_w = if stretches { ln.cross } else { item_outer_w };
                    let reuse = !c.definite_height_read() && c.last_layout_width() == relayout_w;
                    c.set_definite_height(target_h);
                    if !reuse {
                        let (sx, sy) = (c.x(), c.y());
                        layout_box(c, relayout_w, child_inherited);
                        if c.x() != sx || c.y() != sy {
                            shift_box_tree(c, sx - c.x(), sy - c.y());
                        }
                    }
                    c.set_content_height(target_h);
                }
            }
            if reverse {
                cursor_y -= row_gap + between;
            } else {
                cursor_y += outer_main + row_gap + between;
            }
        }
    }
    inner_y + main_extent
}
