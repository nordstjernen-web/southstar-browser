//! Southstar — single-line row flex containers: main sizes, auto margins, justify-content, the cross size with baselines, and item placement and stretching.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::PropId;

use crate::Frame;
use crate::ffi::{
    box_read_definite_height, edges_from_style, flex_align_is_baseline, flex_gap_of,
    flex_item_align, flex_item_baseline, keyword_or, layout_box, resolve_used_height,
    shift_box_tree, style_is_absolute_or_fixed, style_of,
};
use crate::items::{
    self, Item, align_stretches, cross_size_auto, fit_line_keyword_limits, has_auto, is_keyword,
    is_length, is_rtl, justify_offsets, outer_height, outer_main_extras, preset_cross_size,
    relayout_after_cross_resize, resolve_lengths, row_len, stretched_height,
};

struct CrossLimits {
    explicit: f64,
    definite: bool,
    min: f64,
    max: f64,
}

fn cross_limits(b: BoxRef<'_>, parent_content_width: f64) -> CrossLimits {
    let s = style_of(b);
    let get = |prop| s.and_then(|s| s.get(prop));
    let hv = get(PropId::Height);
    let mut explicit = 0.0;
    let mut definite = false;
    if is_length(hv) {
        explicit = resolve_used_height(b, hv, parent_content_width, 0.0);
        definite = explicit > 0.0;
    }
    let mut min = resolve_used_height(b, get(PropId::MinHeight), parent_content_width, -1.0);
    let mut max = resolve_used_height(b, get(PropId::MaxHeight), parent_content_width, -1.0);
    if is_keyword(get(PropId::BoxSizing), c"border-box") {
        let (bd, pd) = (b.border(), b.padding());
        let vex = bd.top + bd.bottom + pd.top + pd.bottom;
        for v in [&mut explicit, &mut min, &mut max] {
            if *v > 0.0 {
                *v -= vex;
                if *v < 0.0 {
                    *v = 0.0;
                }
            }
        }
    }
    if min > explicit {
        explicit = min;
    }
    if explicit <= 0.0
        && s.is_some()
        && style_is_absolute_or_fixed(b.style())
        && b.content_height() > 0.0
        && (hv.is_some() || (get(PropId::Top).is_some() && get(PropId::Bottom).is_some()))
    {
        explicit = b.content_height();
    }
    if explicit <= 0.0 && box_read_definite_height(b) > 0.0 {
        explicit = b.definite_height();
    }
    if !definite && explicit > 0.0 && b.definite_height() > 0.0 {
        definite = true;
    }
    CrossLimits {
        explicit,
        definite,
        min,
        max,
    }
}

fn layout_width(c: BoxRef<'_>, main: f64) -> f64 {
    let (m, p, bd) = (c.margin(), c.padding(), c.border());
    main + m.left + m.right + bd.left + bd.right + p.left + p.right
}

pub fn layout(b: BoxRef<'_>, at: &Frame, parent_content_width: f64) -> f64 {
    let Frame {
        cw,
        inner_x,
        inner_y,
        child_inherited,
        reverse,
    } = *at;
    let limits = cross_limits(b, parent_content_width);

    let mut list: Vec<Item<'_>> = Vec::new();
    items::collect(b, &mut list);
    let n = list.len();

    let gap = flex_gap_of(b.style(), cw);
    let mut total_extras = 0.0;
    for it in list.iter_mut() {
        edges_from_style(it.b, cw);
        total_extras += outer_main_extras(it.b);
        it.len = row_len(it.b, cw, child_inherited);
    }
    if n > 1 {
        total_extras += gap * (n - 1) as f64;
    }
    resolve_lengths(&mut list, cw - total_extras);

    let mut used_main = total_extras;
    for it in &list {
        used_main += it.len.target;
    }
    let mut free_main = cw - used_main;
    let mut auto_margins = 0;
    for it in &list {
        auto_margins += i32::from(has_auto(it.b, PropId::MarginLeft));
        auto_margins += i32::from(has_auto(it.b, PropId::MarginRight));
    }
    if auto_margins > 0 && free_main > 0.0 {
        let share = free_main / f64::from(auto_margins);
        for it in &list {
            let mut m = it.b.margin();
            if has_auto(it.b, PropId::MarginLeft) {
                m.left += share;
            }
            if has_auto(it.b, PropId::MarginRight) {
                m.right += share;
            }
            it.b.set_margin(m);
        }
        free_main = 0.0;
    }
    let justify = keyword_or(b.style(), PropId::JustifyContent, c"flex-start");
    let (leading, between) = if auto_margins == 0 || free_main < 0.0 {
        justify_offsets(b, justify, free_main, n, reverse)
    } else {
        (0.0, 0.0)
    };

    let mut max_cross = 0.0;
    for it in list.iter_mut() {
        let c = it.b;
        let a = it.len.target;
        c.set_x(inner_x);
        c.set_y(inner_y);
        c.set_flex_main(a);
        c.set_definite_height_before_flex(c.definite_height());
        layout_box(c, layout_width(c, a), child_inherited);
        c.set_flex_pass(c.x(), c.y());
        let item_h = outer_height(c);
        it.extra = item_h;
        if item_h > max_cross {
            max_cross = item_h;
        }
    }

    let main_reversed = reverse != is_rtl(b);
    let mut cursor_x = if main_reversed {
        inner_x + cw - leading
    } else {
        inner_x + leading
    };
    let align = keyword_or(b.style(), PropId::AlignItems, c"stretch");
    let mut cross_size = if limits.definite || max_cross < limits.explicit {
        limits.explicit
    } else {
        max_cross
    };
    if limits.min > cross_size {
        cross_size = limits.min;
    }
    if limits.max >= 0.0 && cross_size > limits.max {
        cross_size = limits.max;
        if cross_size < limits.min {
            cross_size = limits.min;
        }
    }

    let mut cross_baseline = 0.0;
    let mut cross_below_baseline = 0.0;
    for it in &list {
        if !flex_align_is_baseline(flex_item_align(it.b, align)) {
            continue;
        }
        let item_h_full = it.extra;
        let bl = flex_item_baseline(it.b, item_h_full);
        if bl > cross_baseline {
            cross_baseline = bl;
        }
        if item_h_full - bl > cross_below_baseline {
            cross_below_baseline = item_h_full - bl;
        }
    }
    if !limits.definite && cross_baseline + cross_below_baseline > cross_size {
        cross_size = cross_baseline + cross_below_baseline;
    }

    for it in &list {
        let c = it.b;
        let eff_align = flex_item_align(c, align);
        let item_h_full = it.extra;
        let mt_auto = has_auto(c, PropId::MarginTop);
        let mb_auto = has_auto(c, PropId::MarginBottom);
        let mut cy = inner_y;
        if mt_auto || mb_auto {
            let mut free_cross = cross_size - item_h_full;
            if free_cross < 0.0 {
                free_cross = 0.0;
            }
            if mt_auto && mb_auto {
                cy = inner_y + free_cross / 2.0;
            } else if mt_auto {
                cy = inner_y + free_cross;
            }
        } else if eff_align == c"center" {
            cy = inner_y + (cross_size - item_h_full) / 2.0;
        } else if eff_align == c"flex-end" || eff_align == c"end" {
            cy = inner_y + cross_size - item_h_full;
        } else if flex_align_is_baseline(eff_align) {
            cy = inner_y + cross_baseline - flex_item_baseline(c, item_h_full);
        }
        let a = it.len.target;
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let outer_main = a + m.left + m.right + p.left + p.right + bd.left + bd.right;
        if main_reversed {
            cursor_x -= outer_main;
        }
        c.set_x(cursor_x);
        c.set_y(cy);
        c.set_flex_main(a);
        let item_layout_width = layout_width(c, a);
        let stretches = !mt_auto && !mb_auto && align_stretches(eff_align) && cross_size_auto(c);
        let cross_preset = stretches && preset_cross_size(c, cross_size, cw);
        let same_input = c.last_layout_width() == item_layout_width
            && (c.definite_height() == c.definite_height_before_flex()
                || !c.definite_height_read());
        if same_input && (!stretches || cross_preset || c.first_child().is_none()) {
            let (px, py) = c.flex_pass();
            c.set_x(px);
            c.set_y(py);
            shift_box_tree(c, cursor_x - inner_x, cy - inner_y);
        } else {
            layout_box(c, item_layout_width, child_inherited);
        }
        if !stretches {
            fit_line_keyword_limits(c, cross_size, cw);
        }
        if stretches {
            let pre_h = c.content_height();
            c.set_content_height(stretched_height(c, cross_size, cw));
            if c.definite_height() <= 0.0 {
                c.set_definite_height(c.content_height());
            }
            if !cross_preset {
                relayout_after_cross_resize(c, item_layout_width, a, pre_h, child_inherited);
            }
        }
        if main_reversed {
            cursor_x -= gap + between;
        } else {
            cursor_x += outer_main + gap + between;
        }
    }
    inner_y + cross_size
}
