//! Southstar — wrapping row flex containers: breaking items into lines, per-line flexing and alignment, align-content and wrap-reverse.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::PropId;

use crate::Frame;
use crate::ffi::{
    box_read_definite_height, edges_from_style, flex_align_is_baseline, flex_gap_of,
    flex_item_align, flex_item_baseline, gap_px, keyword_or, layout_box, resolve_used_height,
    shift_box_tree, style_is_flex_container, style_of,
};
use crate::items::{
    self, Item, align_content_offsets, align_stretches, cross_size_auto, has_auto, is_keyword,
    is_length, is_rtl, justify_offsets, outer_height, outer_main_extras, outer_width,
    preset_cross_size, relayout_after_cross_resize, resolve_lengths, row_len, stretched_height,
    value, vertical_extras, wrap_clamp_height,
};

struct Line {
    top: f64,
    height: f64,
    start: usize,
    count: usize,
}

fn container_cross_size(b: BoxRef<'_>, cw: f64, has_lines: bool) -> f64 {
    let hv = value(b, PropId::Height);
    let mut eh = -1.0;
    if is_length(hv) && has_lines {
        eh = resolve_used_height(b, hv, cw, -1.0);
        if eh >= 0.0 && is_keyword(value(b, PropId::BoxSizing), c"border-box") {
            eh -= vertical_extras(b);
        }
        if eh >= 0.0 {
            eh = wrap_clamp_height(b, eh, cw);
        }
    }
    let flexed_item = b
        .parent()
        .is_some_and(|p| style_is_flex_container(p.style()) && box_read_definite_height(b) > 0.0);
    if (eh < 0.0 || flexed_item) && b.definite_height() > 0.0 && has_lines {
        eh = b.definite_height();
    }
    eh
}

pub fn layout(b: BoxRef<'_>, at: &Frame) -> f64 {
    let Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse,
    } = *at;
    let mut list: Vec<Item<'_>> = Vec::new();
    items::collect(b, &mut list);
    let n = list.len();
    let style = style_of(b);
    let gap = flex_gap_of(b.style(), cw);
    let row_gap_basis = if box_read_definite_height(b) > 0.0 {
        b.definite_height()
    } else {
        0.0
    };
    let row_gap = style.map_or(0.0, |s| {
        gap_px(s.get(PropId::RowGap), s.get(PropId::Gap), row_gap_basis)
    });
    let align = keyword_or(b.style(), PropId::AlignItems, c"stretch");
    let justify = keyword_or(b.style(), PropId::JustifyContent, c"flex-start");
    let main_reversed = reverse != is_rtl(b);
    let mut lines: Vec<Line> = Vec::new();

    for it in list.iter_mut() {
        edges_from_style(it.b, cw);
        it.extra = outer_main_extras(it.b);
        it.len = row_len(it.b, cw, child_inherited);
    }

    let mut line_y = inner_y;
    let mut i = 0;
    while i < n {
        let line_start = i;
        let mut used = 0.0;
        let mut line_max_h = 0.0;
        let mut line_count = 0;
        while i < n {
            let item_outer = list[i].len.hypothetical() + list[i].extra;
            let lead_gap = if line_count > 0 { gap } else { 0.0 };
            let try_used = used + lead_gap + item_outer;
            if try_used > cw + 0.5 && line_count > 0 {
                break;
            }
            used = try_used;
            line_count += 1;
            i += 1;
        }
        let line = &mut list[line_start..line_start + line_count];

        let mut line_extras = if line_count > 1 {
            gap * (line_count - 1) as f64
        } else {
            0.0
        };
        for it in line.iter() {
            line_extras += it.extra;
        }
        resolve_lengths(line, cw - line_extras);
        let mut remaining = cw - line_extras;
        for it in line.iter() {
            remaining -= it.len.target;
        }

        let mut line_auto_margins = 0;
        for it in line.iter() {
            line_auto_margins += i32::from(has_auto(it.b, PropId::MarginLeft));
            line_auto_margins += i32::from(has_auto(it.b, PropId::MarginRight));
        }
        if line_auto_margins > 0 && remaining > 0.0 {
            let share = remaining / f64::from(line_auto_margins);
            for it in line.iter_mut() {
                let mut m = it.b.margin();
                if has_auto(it.b, PropId::MarginLeft) {
                    m.left += share;
                    it.extra += share;
                }
                if has_auto(it.b, PropId::MarginRight) {
                    m.right += share;
                    it.extra += share;
                }
                it.b.set_margin(m);
            }
            remaining = 0.0;
        }

        let (leading, between) = if line_auto_margins == 0 || remaining < 0.0 {
            justify_offsets(b, justify, remaining, line_count, reverse)
        } else {
            (0.0, 0.0)
        };

        for it in line.iter_mut() {
            let c = it.b;
            let a = it.len.target;
            it.main = a;
            c.set_x(inner_x);
            c.set_y(line_y);
            layout_box(c, a + it.extra, child_inherited);
            let item_h = outer_height(c);
            if item_h > line_max_h {
                line_max_h = item_h;
            }
        }

        let mut line_baseline = 0.0;
        let mut line_below_baseline = 0.0;
        for it in line.iter() {
            if !flex_align_is_baseline(flex_item_align(it.b, align)) {
                continue;
            }
            let item_h_full = outer_height(it.b);
            let bl = flex_item_baseline(it.b, item_h_full);
            if bl > line_baseline {
                line_baseline = bl;
            }
            if item_h_full - bl > line_below_baseline {
                line_below_baseline = item_h_full - bl;
            }
        }
        if line_baseline + line_below_baseline > line_max_h {
            line_max_h = line_baseline + line_below_baseline;
        }

        let mut cursor_x = inner_x + leading;
        for it in line.iter() {
            let c = it.b;
            let eff_align = flex_item_align(c, align);
            let item_h_full = outer_height(c);
            let mt_auto = has_auto(c, PropId::MarginTop);
            let mb_auto = has_auto(c, PropId::MarginBottom);
            let mut cy = line_y;
            if mt_auto || mb_auto {
                let mut free_line = line_max_h - item_h_full;
                if free_line < 0.0 {
                    free_line = 0.0;
                }
                if mt_auto && mb_auto {
                    cy = line_y + free_line / 2.0;
                } else if mt_auto {
                    cy = line_y + free_line;
                }
            } else if eff_align == c"center" {
                cy = line_y + (line_max_h - item_h_full) / 2.0;
            } else if eff_align == c"flex-end" || eff_align == c"end" {
                cy = line_y + line_max_h - item_h_full;
            } else if flex_align_is_baseline(eff_align) {
                cy = line_y + line_baseline - flex_item_baseline(c, item_h_full);
            }
            c.set_x(cursor_x);
            c.set_y(cy);
            c.set_flex_main(it.main);
            let item_layout_width = it.main + it.extra;
            let stretches =
                !mt_auto && !mb_auto && cross_size_auto(c) && align_stretches(eff_align);
            let cross_preset = stretches && preset_cross_size(c, line_max_h, cw);
            layout_box(c, item_layout_width, child_inherited);
            let (p, bd) = (c.padding(), c.border());
            let outer = c.content_width() + p.left + p.right + bd.left + bd.right;
            if stretches {
                let pre_h = c.content_height();
                let stretched = stretched_height(c, line_max_h, cw);
                if stretched > c.content_height() {
                    c.set_content_height(stretched);
                }
                if !cross_preset {
                    relayout_after_cross_resize(
                        c,
                        item_layout_width,
                        it.main,
                        pre_h,
                        child_inherited,
                    );
                }
            }
            let m = c.margin();
            cursor_x += outer + m.left + m.right + gap + between;
        }
        if main_reversed {
            for it in line.iter() {
                let c = it.b;
                let w = outer_width(c);
                let nx = inner_x + cw - (c.x() - inner_x) - w;
                if nx != c.x() {
                    shift_box_tree(c, nx - c.x(), 0.0);
                }
            }
        }
        lines.push(Line {
            top: line_y,
            height: line_max_h,
            start: line_start,
            count: line_count,
        });
        line_y += line_max_h + row_gap;
    }

    let trailing_gap = if n > 0 { row_gap } else { 0.0 };
    let measured = (line_y - trailing_gap) - inner_y;
    let mut container_cross = -1.0;
    let eh = container_cross_size(b, cw, !lines.is_empty());
    let cross_definite = eh >= 0.0;
    let mut free_cross = 0.0;
    if cross_definite {
        container_cross = eh;
        free_cross = eh - measured;
    }
    if cross_definite && free_cross.abs() > 0.01 {
        let offsets = align_content_offsets(b, free_cross, lines.len());
        for (li, fl) in lines.iter_mut().enumerate() {
            let dy = offsets.lead + (offsets.between + offsets.per_line) * li as f64;
            let line_h = fl.height + offsets.per_line;
            fl.top += dy;
            fl.height = line_h;
            for it in &list[fl.start..fl.start + fl.count] {
                let c = it.b;
                if dy != 0.0 {
                    shift_box_tree(c, 0.0, dy);
                }
                if offsets.per_line > 0.5 {
                    let eff_align = style_of(c)
                        .and_then(|s| s.keyword_of(PropId::AlignSelf))
                        .filter(|a| *a != c"auto")
                        .unwrap_or(align);
                    if align_stretches(eff_align) && cross_size_auto(c) {
                        let pre_h = c.content_height();
                        let stretched = stretched_height(c, line_h, cw);
                        if stretched > c.content_height() {
                            c.set_content_height(stretched);
                        }
                        relayout_after_cross_resize(
                            c,
                            it.main + it.extra,
                            it.main,
                            pre_h,
                            child_inherited,
                        );
                    } else if eff_align == c"center" {
                        shift_box_tree(c, 0.0, offsets.per_line / 2.0);
                    } else if eff_align == c"flex-end" || eff_align == c"end" {
                        shift_box_tree(c, 0.0, offsets.per_line);
                    }
                }
            }
        }
        line_y += if free_cross > 0.0 { free_cross } else { 0.0 };
    }

    let cursor_y = line_y - trailing_gap;

    if is_keyword(value(b, PropId::FlexWrap), c"wrap-reverse") {
        let cross_total = if container_cross >= 0.0 {
            container_cross
        } else {
            cursor_y - inner_y
        };
        for fl in &lines {
            let mirrored_top = inner_y + cross_total - (fl.top - inner_y) - fl.height;
            for it in &list[fl.start..fl.start + fl.count] {
                let c = it.b;
                let (m, p, bd) = (c.margin(), c.padding(), c.border());
                let outer_h =
                    c.content_height() + m.top + m.bottom + p.top + p.bottom + bd.top + bd.bottom;
                let within = c.y() - fl.top;
                let target = mirrored_top + fl.height - within - outer_h;
                if target != c.y() {
                    shift_box_tree(c, 0.0, target - c.y());
                }
            }
        }
    }
    cursor_y
}
