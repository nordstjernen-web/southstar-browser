//! Southstar — fragmenting a lone inline run across multicol columns by its wrapped lines, and finding the line a run should split at below a float band.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxPool, BoxRef, Style, children, inline_kind};
use southstar_layout_tree::{append_child, clone_range};
use southstar_paint::ffi::pango::{self, AttrList, Layout};
use southstar_style::{PropId, ValueRef};

use crate::{attrs, ffi};

fn attr_can_fragment(kind: u32) -> bool {
    !matches!(
        kind,
        inline_kind::INPUT_FIELD
            | inline_kind::INPUT_FIELD_FOCUSED
            | inline_kind::BUTTON
            | inline_kind::CHECKBOX
            | inline_kind::CHECKBOX_CHECKED
            | inline_kind::RADIO
            | inline_kind::RADIO_CHECKED
            | inline_kind::PROGRESS
            | inline_kind::METER
            | inline_kind::CARET
            | inline_kind::SELECTION
    )
}

fn keyword_is(v: Option<ValueRef<'_>>, kw: &core::ffi::CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

pub fn can_fragment(b: BoxRef<'_>, style: *const Style, basis: f64) -> bool {
    if b.kind() != BoxKind::Inline || b.text().is_none_or(|t| t.is_empty()) {
        return false;
    }
    if b.inline_atomics().is_some_and(|a| !a.is_empty()) {
        return false;
    }
    let s = ffi::style(style);
    let value = |p: PropId| s.and_then(|s| s.get(p));
    if keyword_is(value(PropId::WhiteSpace), c"nowrap")
        || keyword_is(value(PropId::WhiteSpace), c"pre")
    {
        return false;
    }
    if keyword_is(value(PropId::TextOverflow), c"ellipsis") {
        return false;
    }
    if value(PropId::LineClamp).is_some() {
        return false;
    }
    if ffi::style_text_indent(style, basis).abs() > 0.01 {
        return false;
    }
    b.attrs().iter().all(|a| attr_can_fragment(a.kind))
}

fn layout_for_multicol(b: BoxRef<'_>, content_width: f64, ps: *const Style) -> Layout {
    let layout = ffi::new_layout(ps);
    layout.set_width((content_width * pango::SCALE_F) as i32);
    layout.set_wrap(ffi::wrap_mode_for(ps));
    if b.inline_atomics().is_none_or(|a| a.is_empty()) {
        ffi::apply_css_line_spacing(&layout, ps);
    }
    let text = b.text().unwrap_or(c"");
    layout.set_text(text);
    let i18n = AttrList::new();
    ffi::apply_i18n(&layout, &i18n, b);
    ffi::apply_font_features(&i18n, ps);
    attrs::apply_spacing(&i18n, ffi::style(ps), text.to_bytes());
    attrs::apply_layout_attrs(&i18n, b);
    attrs::set_attrs(&layout, Some(&i18n), Some(b));
    layout
}

pub struct Columns {
    pub inner_x: f64,
    pub inner_y: f64,
    pub col_w: f64,
    pub col_gap: f64,
    pub n_cols: i32,
}

pub fn layout_single_inline(b: BoxRef<'_>, cols: &Columns, ps: *const Style) -> Option<f64> {
    let src = b.first_child()?;
    if src.next_sibling().is_some() || cols.n_cols < 2 {
        return None;
    }
    if !can_fragment(src, ps, cols.col_w) {
        return None;
    }
    let layout = layout_for_multicol(src, cols.col_w, ps);
    let line_count = layout.line_count();
    if line_count <= 1 {
        return None;
    }
    let per_col = line_count / cols.n_cols + i32::from(line_count % cols.n_cols != 0);
    let text_len = src.text().map_or(0, |t| t.to_bytes().len());
    let mut fragments = Vec::new();
    for col in 0..cols.n_cols {
        let first_line = col * per_col;
        let mut after_line = first_line + per_col;
        if first_line >= line_count {
            break;
        }
        if after_line > line_count {
            after_line = line_count;
        }
        let Some(first) = layout.line(first_line) else {
            continue;
        };
        let start = first.start_index() as usize;
        let end = if after_line < line_count {
            layout
                .line(after_line)
                .map_or(text_len, |l| l.start_index() as usize)
        } else {
            text_len
        };
        let Some(frag) = clone_range(src, start, end) else {
            continue;
        };
        frag.set_x(cols.inner_x + f64::from(col) * (cols.col_w + cols.col_gap));
        frag.set_y(cols.inner_y);
        ffi::layout_box(frag, cols.col_w, ps);
        fragments.push(frag);
    }
    drop(layout);
    if fragments.len() < 2 {
        for frag in fragments {
            BoxPool::free_tree(Some(frag));
        }
        return None;
    }
    b.set_first_child(None);
    b.set_last_child(None);
    for frag in fragments {
        frag.set_parent(None);
        frag.set_next_sibling(None);
        append_child(b, frag);
    }
    src.set_parent(None);
    src.set_next_sibling(None);
    BoxPool::free_tree(Some(src));
    let mut max_h = 0.0;
    for c in children(b) {
        if c.content_height() > max_h {
            max_h = c.content_height();
        }
    }
    b.set_content_width(
        cols.col_w * f64::from(cols.n_cols) + cols.col_gap * f64::from(cols.n_cols - 1),
    );
    Some(cols.inner_y + max_h)
}

pub fn run_break_before(run: BoxRef<'_>, band_height: f64) -> usize {
    let Some(layout) = ffi::paint_inline_layout(run) else {
        return 0;
    };
    let mut iter = layout.iter();
    loop {
        let logical = iter.line_logical_extents();
        if f64::from(logical.y) / pango::SCALE_F >= band_height - 0.5 {
            return iter.index() as usize;
        }
        if !iter.next_line() {
            return 0;
        }
    }
}
