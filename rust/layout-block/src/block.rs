//! Southstar — laying out a block container: its used width, auto margins and content box, then flex, grid or block flow of its children and its used height.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::sync::atomic::{AtomicI32, Ordering};

use southstar_layout::{BoxRef, Style};
use southstar_layout_float::{box_establishes_bfc, style_is_grid_container};
use southstar_style::{Display, Kind, PropId, StyleRef, ValueRef, display_of};

use crate::align::length_is_auto;
use crate::ffi::{
    ContentBox, edges_into_box, flex_direction_of, height_keyword_stretches,
    inline_runs_join_splits, intrinsic_keyword_width, keyword_or, layout_flex_column,
    layout_flex_row, layout_grid, length_resolve, measure_natural_width, min_width_of,
    reorder_children_by_order, style_is_absolute_or_fixed, style_is_flex_container, style_of,
    text_input_intrinsic_width,
};
use crate::fieldset::{layout_rendered_legend, rendered_legend, set_content_block_size};
use crate::flex::{grow_of, wraps};
use crate::flow::{FlowStart, block_height_is_auto, layout_flow};
use crate::height::{HeightInput, finish_height};

const GRID_NESTING_MAX: i32 = 64;
static GRID_NESTING: AtomicI32 = AtomicI32::new(0);

const BOX_NORMAL: u8 = 0;
const OUTER_INLINE: u8 = 0;
const INNER_FLOW: u8 = 0;
const INNER_TABLE: u8 = 2;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;

fn display_is_atomic_inline_container(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
        && d.inner != INNER_TABLE
}

fn keyword_bytes(v: Option<ValueRef<'_>>) -> Option<&[u8]> {
    v.filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(|k| k.to_bytes())
}

fn prop<'a>(s: Option<StyleRef<'a>>, p: PropId) -> Option<ValueRef<'a>> {
    s.and_then(|s| s.get(p))
}

fn non_negative(v: f64) -> f64 {
    if v < 0.0 { 0.0 } else { v }
}

fn mi_style(inherited: *const Style, b: BoxRef<'_>) -> *const Style {
    if inherited.is_null() {
        b.style()
    } else {
        inherited
    }
}

struct UsedWidth {
    cw: f64,
    explicit: bool,
    border_box: bool,
}

fn flex_col_item_stretches(b: BoxRef<'_>, parent: BoxRef<'_>) -> bool {
    let style = style_of(b);
    let mut eff = style.and_then(|s| s.keyword_of(PropId::AlignSelf));
    if eff.is_none_or(|e| e.to_bytes() == b"auto") {
        eff = Some(keyword_or(parent.style(), PropId::AlignItems, c"stretch"));
    }
    let eff = eff.map_or(&b""[..], |e| e.to_bytes());
    if eff != b"stretch" && eff != b"normal" {
        return false;
    }
    !(length_is_auto(prop(style, PropId::MarginLeft))
        || length_is_auto(prop(style, PropId::MarginRight)))
}

fn used_width(b: BoxRef<'_>, pcw: f64, inherited: *const Style) -> UsedWidth {
    let style = style_of(b);
    let wv = prop(style, PropId::Width);
    let mxw = prop(style, PropId::MaxWidth);
    let mnw = prop(style, PropId::MinWidth);
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let horiz_extras = p.left + p.right + bd.left + bd.right;
    let horiz_total = horiz_extras + m.left + m.right;
    let mut explicit_width = false;
    let mut intrinsic_width = false;
    let mut flex_grow_filled = false;
    let parent = b.parent();
    let parent_flex_dir = parent.map_or(&b"row"[..], |p| flex_direction_of(p.style()));
    let parent_is_flex = parent.is_some_and(|p| style_is_flex_container(p.style()));
    let flex_row_item = parent_is_flex && matches!(parent_flex_dir, b"row" | b"row-reverse");
    let mut flex_col_item_stretch =
        parent_is_flex && matches!(parent_flex_dir, b"column" | b"column-reverse");
    if flex_col_item_stretch && let Some(parent) = parent {
        flex_col_item_stretch = flex_col_item_stretches(b, parent);
    }
    let mut pct_width_base = pcw;
    if flex_row_item && let Some(parent) = parent.filter(|p| p.content_width() > 0.0) {
        pct_width_base = parent.content_width();
    }
    let wkw = keyword_bytes(wv);
    let mut cw;
    if let Some(size) = b.take_flex_main() {
        cw = non_negative(size);
        explicit_width = true;
        flex_grow_filled = true;
    } else if flex_row_item && grow_of(b) > 0.0 {
        cw = non_negative(pcw - horiz_total);
        explicit_width = true;
        flex_grow_filled = true;
    } else if wv.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc)) {
        cw = length_resolve(wv, pct_width_base, 0.0);
        explicit_width = true;
    } else if matches!(
        wkw,
        Some(b"stretch" | b"-webkit-fill-available" | b"-moz-available")
    ) {
        cw = non_negative(pcw - horiz_total);
        explicit_width = true;
        intrinsic_width = true;
    } else if let Some(kw @ (b"max-content" | b"min-content" | b"fit-content")) = wkw {
        let mi = mi_style(inherited, b);
        let avail = non_negative(pcw - horiz_total);
        cw = match kw {
            b"min-content" => min_width_of(b, mi),
            b"max-content" => measure_natural_width(b, mi),
            _ => {
                let mn = min_width_of(b, mi);
                let mx = measure_natural_width(b, mi);
                let w = if mx < avail { mx } else { avail };
                if w < mn { mn } else { w }
            }
        };
        cw = non_negative(cw);
        explicit_width = true;
        intrinsic_width = true;
    } else if !flex_col_item_stretch && display_is_atomic_inline_container(display_of(style)) {
        let avail = non_negative(pcw - horiz_total);
        if height_keyword_stretches(wv) {
            cw = avail;
            explicit_width = true;
            intrinsic_width = true;
        } else {
            let mut natural = measure_natural_width(b, mi_style(inherited, b));
            let input_width = text_input_intrinsic_width(b);
            if natural < input_width {
                natural = input_width;
            }
            cw = if natural < avail { natural } else { avail };
        }
        cw = non_negative(cw);
    } else {
        cw = non_negative(pcw - horiz_total);
    }
    let border_box = keyword_bytes(prop(style, PropId::BoxSizing)) == Some(b"border-box");
    if border_box && explicit_width && !intrinsic_width && !flex_grow_filled {
        cw = non_negative(cw - horiz_extras);
    }
    let keyword_avail = pcw - horiz_total;
    let mut max_cw = length_resolve(mxw, pct_width_base, -1.0);
    if max_cw < 0.0
        && let Some(v) = mxw.filter(|v| v.kind() == Kind::Keyword)
    {
        max_cw =
            intrinsic_keyword_width(b, v.keyword_text(), mi_style(inherited, b), keyword_avail);
        if max_cw >= 0.0 && border_box {
            max_cw += horiz_extras;
        }
    }
    if max_cw >= 0.0 {
        if border_box {
            max_cw -= horiz_extras;
        }
        if max_cw >= 0.0 && cw > max_cw {
            cw = max_cw;
            explicit_width = true;
        }
    }
    let mut min_cw = length_resolve(mnw, pct_width_base, -1.0);
    if min_cw < 0.0
        && let Some(v) = mnw.filter(|v| v.kind() == Kind::Keyword)
    {
        min_cw =
            intrinsic_keyword_width(b, v.keyword_text(), mi_style(inherited, b), keyword_avail);
        if min_cw >= 0.0 && border_box {
            min_cw += horiz_extras;
        }
    }
    if min_cw >= 0.0 {
        if border_box {
            min_cw -= horiz_extras;
        }
        if min_cw >= 0.0 && cw < min_cw {
            cw = min_cw;
            explicit_width = true;
        }
    }
    UsedWidth {
        cw,
        explicit: explicit_width,
        border_box,
    }
}

fn resolve_auto_margins(b: BoxRef<'_>, pcw: f64, cw: f64) {
    let style = style_of(b);
    let ml_auto = length_is_auto(prop(style, PropId::MarginLeft));
    let mr_auto = length_is_auto(prop(style, PropId::MarginRight));
    let (mut m, p, bd) = (b.margin(), b.padding(), b.border());
    let horiz_extras = p.left + p.right + bd.left + bd.right;
    let available = non_negative(pcw - cw - horiz_extras);
    if ml_auto && mr_auto {
        m.left = available / 2.0;
        m.right = available / 2.0;
    } else if ml_auto {
        m.left = non_negative(available - m.right);
    } else if mr_auto {
        m.right = non_negative(available - m.left);
    } else {
        return;
    }
    b.set_margin(m);
}

fn layout_flex_or_grid(b: BoxRef<'_>, content: &ContentBox, pcw: f64, cursor_y: &mut f64) -> bool {
    if style_is_flex_container(b.style()) {
        let dir = flex_direction_of(b.style());
        match dir {
            b"row" | b"row-reverse" => {
                let wrap = wraps(style_of(b));
                layout_flex_row(b, content, dir == b"row-reverse", wrap, pcw, cursor_y);
                return true;
            }
            b"column" | b"column-reverse" => {
                layout_flex_column(b, content, dir == b"column-reverse", pcw, cursor_y);
                return true;
            }
            _ => {}
        }
    }
    if style_is_grid_container(style_of(b))
        && GRID_NESTING.load(Ordering::Relaxed) < GRID_NESTING_MAX
    {
        GRID_NESTING.fetch_add(1, Ordering::Relaxed);
        layout_grid(b, content, cursor_y);
        GRID_NESTING.fetch_sub(1, Ordering::Relaxed);
        return true;
    }
    false
}

pub fn layout_block(b: BoxRef<'_>, pcw: f64, inherited: *const Style) {
    inline_runs_join_splits(b);
    edges_into_box(b, pcw);
    b.set_margin_top_through(0.0);
    let (p, bd) = (b.padding(), b.border());
    let vert_extras = p.top + p.bottom + bd.top + bd.bottom;
    let width = used_width(b, pcw, inherited);
    let cw = width.cw;
    b.set_content_width(cw);
    if width.explicit && !style_is_absolute_or_fixed(b.style()) {
        resolve_auto_margins(b, pcw, cw);
    }

    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let inner_x = b.x() + m.left + bd.left + p.left;
    let mut inner_y = b.y() + m.top + bd.top + p.top;
    let child_inherited = if b.style().is_null() {
        inherited
    } else {
        b.style()
    };
    let legend = rendered_legend(b);
    let mut legend_extra = 0.0;
    if let Some(legend) = legend {
        legend_extra = layout_rendered_legend(b, legend, cw, inner_x, child_inherited);
        inner_y += legend_extra;
        b.detach_child(legend);
        let sizing_extras = if width.border_box { vert_extras } else { 0.0 };
        set_content_block_size(b, pcw, sizing_extras, legend_extra);
    }
    let mut cursor_y = inner_y;
    let start = FlowStart {
        collapse_top_with_parent: b.padding().top == 0.0
            && b.border().top == 0.0
            && !box_establishes_bfc(b),
        collapse_bottom_with_parent: b.padding().bottom == 0.0
            && b.border().bottom == 0.0
            && b.parent().is_some()
            && !box_establishes_bfc(b)
            && block_height_is_auto(b, pcw),
    };

    if style_is_flex_container(b.style()) || style_is_grid_container(style_of(b)) {
        reorder_children_by_order(b);
    }

    let content = ContentBox {
        cw,
        inner_x,
        inner_y,
        child_inherited,
    };
    if !layout_flex_or_grid(b, &content, pcw, &mut cursor_y) {
        inner_y = layout_flow(b, &content, start, &mut cursor_y);
    }

    if let Some(legend) = legend {
        b.prepend_child(legend);
    }
    finish_height(
        b,
        &HeightInput {
            pcw,
            inner_x,
            inner_y,
            cursor_y,
            legend_extra,
            border_box: width.border_box,
            vert_extras,
        },
    );
}
