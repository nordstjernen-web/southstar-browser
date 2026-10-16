//! Southstar — a fieldset's rendered legend: sizing and placing it over the top border, the content height left below it, and the border gap the painter leaves for it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, Style, children};
use southstar_layout_geom::{edges_into_box, resolve_used_height};
use southstar_style::{Kind, PropId, ValueRef};

use crate::align::length_is_auto;
use crate::dispatch::{layout_box, shift_box_tree};
use crate::ffi::{measure_natural_width, min_width_of, style_of};

pub fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub fn rendered_legend(b: BoxRef<'_>) -> Option<BoxRef<'_>> {
    children(b).find(|c| c.is_rendered_legend())
}

fn is_rtl(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| v.is_keyword(c"rtl"))
}

fn legend_inline_offset(fieldset: BoxRef<'_>, legend: BoxRef<'_>, free_w: f64) -> f64 {
    let ls = style_of(legend);
    let ml_auto = ls.is_some_and(|s| length_is_auto(s.get(PropId::MarginLeft)));
    let mr_auto = ls.is_some_and(|s| length_is_auto(s.get(PropId::MarginRight)));
    if ml_auto && mr_auto {
        return free_w / 2.0;
    }
    if ml_auto {
        return free_w;
    }
    if mr_auto {
        return 0.0;
    }
    let mut js = ls
        .and_then(|s| s.keyword_of(PropId::JustifySelf))
        .map(|k| k.to_bytes());
    if let Some(j) = js {
        if let Some(rest) = j.strip_prefix(b"safe ") {
            js = Some(rest);
        } else if let Some(rest) = j.strip_prefix(b"unsafe ") {
            js = Some(rest);
        }
    }
    let fieldset_rtl = style_of(fieldset).is_some_and(|s| is_rtl(s.get(PropId::Direction)));
    let legend_rtl = ls.is_some_and(|s| is_rtl(s.get(PropId::Direction)));
    let mut at_end = fieldset_rtl;
    let Some(js) = js else {
        return if at_end { free_w } else { 0.0 };
    };
    match js {
        b"center" => return free_w / 2.0,
        b"left" => at_end = false,
        b"right" => at_end = true,
        b"end" | b"flex-end" => at_end = !fieldset_rtl,
        b"self-start" => at_end = legend_rtl,
        b"self-end" => at_end = !legend_rtl,
        _ => {}
    }
    if at_end { free_w } else { 0.0 }
}

pub fn layout_rendered_legend(
    fieldset: BoxRef<'_>,
    legend: BoxRef<'_>,
    cw: f64,
    inner_x: f64,
    inherited: *const Style,
) -> f64 {
    edges_into_box(legend, cw);
    let (m, p, bd) = (legend.margin(), legend.padding(), legend.border());
    let outer_extras = p.left + p.right + bd.left + bd.right + m.left + m.right;
    let wv = style_of(legend).and_then(|s| s.get(PropId::Width));
    let mut layout_w = cw;
    if wv.is_none() || length_is_auto(wv) {
        let avail = c_max(cw - outer_extras, 0.0);
        let mut fit = measure_natural_width(legend, inherited);
        if fit > avail {
            fit = avail;
        }
        let floor_w = min_width_of(legend, inherited);
        if fit < floor_w {
            fit = floor_w;
        }
        layout_w = fit + outer_extras;
    }
    let top_edge = fieldset.y() + fieldset.margin().top;
    legend.set_x(inner_x);
    legend.set_y(top_edge);
    layout_box(legend, layout_w, inherited);
    let (m, p, bd) = (legend.margin(), legend.padding(), legend.border());
    let outer_w = legend.content_width() + p.left + p.right + bd.left + bd.right + m.left + m.right;
    let border_box_h = legend.content_height() + p.top + p.bottom + bd.top + bd.bottom;
    let border_top = fieldset.border().top;
    let legend_top = m.top + c_max((border_top - border_box_h) / 2.0, 0.0);
    let target_x = inner_x + legend_inline_offset(fieldset, legend, cw - outer_w);
    let target_y = top_edge + legend_top - m.top;
    shift_box_tree(legend, target_x - legend.x(), target_y - legend.y());
    let painted_border_top = c_max(legend_top + border_box_h / 2.0 - border_top / 2.0, 0.0);
    let legend_bottom = legend_top + border_box_h + m.bottom;
    let extra = c_max(painted_border_top, legend_bottom - border_top);
    c_max(extra, 0.0)
}

pub fn set_content_block_size(
    b: BoxRef<'_>,
    width_basis: f64,
    sizing_extras: f64,
    legend_extra: f64,
) {
    let hv = style_of(b).and_then(|s| s.get(PropId::Height));
    if !hv.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc)) {
        return;
    }
    let h = resolve_used_height(b, hv, width_basis, -1.0);
    if h < 0.0 {
        return;
    }
    let h = c_max(h - sizing_extras - legend_extra, 0.0);
    for c in children(b) {
        c.set_cb_height_override(h);
    }
}

pub struct LegendGap {
    pub inset: f64,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

pub fn legend_gap(fieldset: BoxRef<'_>) -> Option<LegendGap> {
    let legend = rendered_legend(fieldset)?;
    let top_edge = fieldset.y() + fieldset.margin().top;
    let (m, p, bd) = (legend.margin(), legend.padding(), legend.border());
    let legend_border_top = legend.y() + m.top;
    let border_box_w = legend.content_width() + p.left + p.right + bd.left + bd.right;
    let border_box_h = legend.content_height() + p.top + p.bottom + bd.top + bd.bottom;
    let fs_border_top = fieldset.border().top;
    let inset = c_max(
        legend_border_top - top_edge + border_box_h / 2.0 - fs_border_top / 2.0,
        0.0,
    );
    let x0 = legend.x() + m.left;
    Some(LegendGap {
        inset,
        x0,
        x1: x0 + border_box_w,
        y0: c_min(legend.y(), top_edge + inset),
        y1: c_max(
            legend_border_top + border_box_h + m.bottom,
            top_edge + inset + fs_border_top,
        ),
    })
}
