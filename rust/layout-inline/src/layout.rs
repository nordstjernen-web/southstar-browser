//! Southstar — laying out an inline run: line boxes and their heights, struts, atomic inlines placed on their lines, text-align, text-indent, line clamping and vertical text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, InlineAtomic, Style, inline_kind as k};
use southstar_paint::ffi::pango::{self, AttrList, Layout};
use southstar_style::{Kind, PropId, StyleRef};

use crate::attrs::{self, font_size};
use crate::ffi;
use crate::measure;

const SCALE: f64 = pango::SCALE_F;

pub(crate) fn atomic_at(b: BoxRef<'_>, i: usize) -> Option<&InlineAtomic> {
    b.inline_atomics()?.get(i)
}

pub(crate) fn has_atomics(b: BoxRef<'_>) -> bool {
    b.inline_atomics().is_some_and(|a| !a.is_empty())
}

fn keyword_is(s: Option<StyleRef<'_>>, prop: PropId, kw: &core::ffi::CStr) -> bool {
    s.and_then(|s| s.get(prop))
        .is_some_and(|v| v.is_keyword(kw))
}

pub(crate) fn white_space_nowrap(s: Option<StyleRef<'_>>) -> bool {
    keyword_is(s, PropId::WhiteSpace, c"nowrap") || keyword_is(s, PropId::WhiteSpace, c"pre")
}

pub(crate) fn line_height(parent_style: *const Style) -> f64 {
    let fs = font_size(ffi::style(parent_style));
    let used = ffi::css_line_height_px(parent_style);
    if used > 0.0 { used } else { fs * 1.2 }
}

fn control_line_height(b: BoxRef<'_>, line_height: f64) -> f64 {
    let all = b.attrs();
    let mut out = line_height;
    for r in all {
        if r.kind != k::INPUT_FIELD && r.kind != k::INPUT_FIELD_FOCUSED && r.kind != k::BUTTON {
            continue;
        }
        if attrs::is_textarea(r) || r.native_chrome == 0 {
            continue;
        }
        let cfs = all
            .iter()
            .find(|f| {
                f.kind == k::FONT_SIZE && f.start <= r.start && f.start + f.len >= r.start + r.len
            })
            .map_or(0.0, |f| f.font_size_px);
        let font_box = if cfs > 0.0 { cfs * 1.3 + 12.0 } else { 0.0 };
        let mut h = if r.box_h > 0.0 {
            r.box_h + 8.0
        } else {
            line_height + 18.0
        };
        if font_box > h {
            h = font_box;
        }
        if h > out {
            out = h;
        }
    }
    out
}

fn style_sets_block_height(s: StyleRef<'_>) -> bool {
    s.get(PropId::Height)
        .is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn textarea_total_height(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    let mut out = 0.0;
    for r in b.attrs() {
        if (r.kind == k::INPUT_FIELD || r.kind == k::INPUT_FIELD_FOCUSED)
            && attrs::is_textarea(r)
            && r.box_h > 0.0
            && !(r.style() == parent_style
                && ffi::style(r.style()).is_some_and(style_sets_block_height))
        {
            let h = r.box_h + if r.native_chrome != 0 { 8.0 } else { 0.0 };
            if h > out {
                out = h;
            }
        }
    }
    out
}

fn attr_kind_cacheable(kind: u32) -> bool {
    !matches!(
        kind,
        k::INPUT_FIELD
            | k::INPUT_FIELD_FOCUSED
            | k::BUTTON
            | k::CHECKBOX
            | k::CHECKBOX_CHECKED
            | k::RADIO
            | k::RADIO_CHECKED
            | k::PROGRESS
            | k::METER
            | k::CARET
            | k::SELECTION
    )
}

pub(crate) fn measure_cacheable(b: BoxRef<'_>) -> bool {
    !has_atomics(b) && b.attrs().iter().all(|a| attr_kind_cacheable(a.kind))
}

fn apply_text_align(layout: &Layout, s: Option<StyleRef<'_>>) {
    let ta = s.and_then(|s| s.get(PropId::TextAlign));
    let is = |kw: &core::ffi::CStr| ta.is_some_and(|v| v.is_keyword(kw));
    let rtl = layout.context().base_dir() == pango::DIRECTION_RTL;
    if is(c"center") {
        layout.set_alignment(pango::ALIGN_CENTER);
    } else if is(c"right") || (is(c"end") && !rtl) || (is(c"start") && rtl) || (ta.is_none() && rtl)
    {
        layout.set_alignment(pango::ALIGN_RIGHT);
    } else if is(c"justify") {
        layout.set_justify(true);
    } else {
        layout.set_alignment(pango::ALIGN_LEFT);
    }
}

pub(crate) fn shaping_attrs(layout: &Layout, b: BoxRef<'_>, ps: *const Style, text: &[u8]) {
    let i18n = AttrList::new();
    ffi::apply_i18n(layout, &i18n, b);
    ffi::apply_font_features(&i18n, ps);
    attrs::apply_atomic_shapes(&i18n, b);
    attrs::apply_spacing(&i18n, ffi::style(ps), text);
    attrs::apply_layout_attrs(&i18n, b);
    attrs::set_attrs(layout, Some(&i18n), Some(b));
}

pub(crate) fn vertical_measure(b: BoxRef<'_>, ps: *const Style) -> (f64, f64) {
    let Some(text) = b.text() else {
        return (0.0, 0.0);
    };
    let layout = ffi::new_layout(ps);
    if ffi::text_orientation(ps) == 1 {
        layout.set_width(-1);
        layout.set_alignment(pango::ALIGN_CENTER);
        match ffi::vertical_stack_text(text) {
            Some(stacked) => layout.set_text(&stacked),
            None => layout.set_text(c""),
        }
        let (pw, ph) = layout.pixel_size();
        (f64::from(pw), f64::from(ph))
    } else {
        layout.set_width(-1);
        ffi::apply_css_line_spacing(&layout, ps);
        let i18n = AttrList::new();
        ffi::apply_i18n(&layout, &i18n, b);
        ffi::apply_font_features(&i18n, ps);
        attrs::apply_spacing(&i18n, ffi::style(ps), text.to_bytes());
        attrs::set_attrs(&layout, Some(&i18n), Some(b));
        drop(i18n);
        layout.set_text(text);
        let (pw, ph) = layout.pixel_size();
        (f64::from(ph), f64::from(pw))
    }
}

fn line_top(layout: &Layout, line_index: i32) -> f64 {
    let mut iter = layout.iter();
    let mut top;
    let mut j = 0;
    loop {
        top = f64::from(iter.line_logical_extents().y) / SCALE;
        if j >= line_index || !iter.next_line() {
            break;
        }
        j += 1;
    }
    top
}

fn record_abs_statics(layout: &Layout, b: BoxRef<'_>, ps: *const Style, content_width: f64) {
    let Some(atomics) = b.inline_atomics() else {
        return;
    };
    let indent = ffi::text_indent(b, ps, content_width);
    for a in atomics {
        let Some(ab) = a.box_ref() else {
            continue;
        };
        let dom = ffi::abs_static_target(ab);
        if dom.is_null() {
            continue;
        }
        let off = a.byte_off() as i32;
        let pos = layout.index_to_pos(off);
        let line_index = layout.index_to_line(off);
        ffi::record_abs_static(
            dom,
            b,
            indent + f64::from(pos.x) / SCALE,
            line_top(layout, line_index),
        );
    }
}

fn layout_atomics(b: BoxRef<'_>, content_width: f64, ps: *const Style) {
    let mut i = 0;
    while let Some(a) = atomic_at(b, i) {
        i += 1;
        let Some(ab) = a.box_ref() else {
            continue;
        };
        if ffi::is_abs_placeholder(ab) {
            continue;
        }
        let (w0, h0) = (ab.content_width(), ab.content_height());
        ffi::layout_box(ab, content_width, ps);
        if ab.content_width() != w0 || ab.content_height() != h0 {
            ffi::layout_box(ab, content_width, ps);
        }
    }
}

fn place_atomics(
    layout: &Layout,
    b: BoxRef<'_>,
    ps: *const Style,
    content_width: f64,
    line_heights: &[f64],
) {
    let Some(text) = b.text() else {
        return;
    };
    let line_count = line_heights.len();
    layout.set_text(text);
    if layout.width() < 0 && layout.alignment() != pango::ALIGN_LEFT {
        let (pw, _) = layout.pixel_size();
        if f64::from(pw) <= content_width {
            layout.set_width((content_width * SCALE) as i32);
        }
    }
    let mut line_tops = vec![0.0; line_count];
    let mut line_pango_h = vec![0.0; line_count];
    let mut iter = layout.iter();
    for j in 0..line_count {
        let logical = iter.line_logical_extents();
        line_tops[j] = f64::from(logical.y) / SCALE;
        line_pango_h[j] = f64::from(logical.height) / SCALE;
        if !iter.next_line() {
            break;
        }
    }
    drop(iter);
    let mut text_x0 = b.x();
    let ti = ffi::text_indent(b, ps, content_width);
    if ti < 0.0 {
        text_x0 += ti;
    }
    let Some(atomics) = b.inline_atomics() else {
        return;
    };
    for a in atomics {
        let Some(ab) = a.box_ref() else {
            continue;
        };
        let off = a.byte_off() as i32;
        let pos = layout.index_to_pos(off);
        let line = layout.index_to_line(off);
        let on_line = usize::try_from(line).ok().filter(|&l| l < line_count);
        let line_y = line_heights[..usize::try_from(line).unwrap_or(0).min(line_count)]
            .iter()
            .fold(0.0, |acc, h| acc + h);
        let mut within = on_line.map_or(0.0, |l| {
            f64::from(pos.y) / SCALE - line_tops[l] + (line_heights[l] - line_pango_h[l]) / 2.0
        });
        let mut slack = on_line.map_or(0.0, |l| line_heights[l] - attrs::outer_height(ab));
        if slack < 0.0 {
            slack = 0.0;
        }
        if within < 0.0 {
            within = 0.0;
        }
        if within > slack {
            within = slack;
        }
        let nx = text_x0 + f64::from(pos.x) / SCALE;
        let ny = b.y() + line_y + within;
        ffi::shift(ab, nx - ab.x(), ny - ab.y());
    }
}

pub(crate) fn inline_layout(b: BoxRef<'_>, content_width: f64, ps: *const Style) {
    let Some(text) = b.text().filter(|t| !t.is_empty()) else {
        b.set_content_width(0.0);
        b.set_content_height(0.0);
        b.set_first_baseline(0.0);
        return;
    };
    let style = ffi::style(ps);
    b.set_writing_mode(0, 0);
    let atomics = has_atomics(b);
    let wm = ffi::writing_mode(ps);
    if wm != 0 && !atomics {
        b.set_writing_mode(wm, ffi::text_orientation(ps));
        let (thickness, length) = vertical_measure(b, ps);
        b.set_content_width(thickness);
        b.set_content_height(length);
        b.set_first_baseline(0.0);
        b.set_inline_layout_cache(None);
        return;
    }

    layout_atomics(b, content_width, ps);

    let cacheable = measure_cacheable(b);
    if cacheable
        && let Some((cs, cw, ch)) = b.inline_layout_cache()
        && cs == ps
        && (cw - content_width).abs() < 0.001
    {
        b.set_content_width(content_width);
        b.set_content_height(ch);
        return;
    }

    b.set_content_width(content_width);
    let layout = ffi::new_layout(ps);
    let ellip = keyword_is(style, PropId::TextOverflow, c"ellipsis");
    if white_space_nowrap(style) && !ellip {
        layout.set_width(-1);
    } else {
        layout.set_width((content_width * SCALE) as i32);
    }
    layout.set_wrap(ffi::wrap_mode_for(ps));
    if atomics {
        apply_text_align(&layout, style);
    } else {
        ffi::apply_css_line_spacing(&layout, ps);
    }
    let ti = ffi::text_indent(b, ps, content_width);
    if ti > 0.0 {
        layout.set_indent((ti * SCALE) as i32);
    }
    if ellip {
        layout.set_ellipsize(pango::ELLIPSIZE_END);
    }
    if let Some(lc) = style.and_then(|s| s.get(PropId::LineClamp))
        && let Some((v, _)) = lc.length()
        && v >= 1.0
    {
        layout.set_height(-(v as i32));
        layout.set_ellipsize(pango::ELLIPSIZE_END);
    }
    shaping_attrs(&layout, b, ps, text.to_bytes());

    layout.set_text(text);
    ffi::start_align_overflow(&layout);
    let measured = measure::measure(&layout);
    let measured_h = measured.pixel_logical().height;
    if b.inline_atomics().is_some() {
        record_abs_statics(&layout, b, ps, content_width);
    }
    let line_count = measured.lines.max(1) as usize;
    let mut lh_default = line_height(ps);
    if let Some(parent) = b.parent()
        && ffi::is_one_line_text_input(parent.dom_ptr())
    {
        lh_default = lh_default.max(ffi::normal_line_height_px(ps));
    }
    let lh_control = control_line_height(b, lh_default);
    let mut line_heights = vec![lh_control; line_count];
    if layout.css_line_height(ffi::LINE_HEIGHT_KEY).is_some() && lh_control <= lh_default + 0.01 {
        let mut iter = layout.iter();
        let mut j = 0;
        loop {
            let logical = iter.line_logical_extents();
            if j < line_count {
                line_heights[j] = f64::from(logical.height) / SCALE;
            }
            j += 1;
            if !iter.next_line() {
                break;
            }
        }
    }
    if let Some(list) = b.inline_atomics() {
        for a in list {
            let Some(atomic) = a.box_ref() else {
                continue;
            };
            let line = layout.index_to_line(a.byte_off() as i32);
            let Some(line) = usize::try_from(line).ok().filter(|&l| l < line_count) else {
                continue;
            };
            let outer = attrs::outer_height(atomic);
            if outer > line_heights[line] {
                line_heights[line] = outer;
            }
        }
    }
    let expected = line_heights.iter().fold(0.0, |acc, h| acc + h);
    if atomics {
        b.set_atomic_line_heights(&line_heights);
    }
    b.set_content_width(content_width);
    b.set_content_height(expected);
    let ta_h = textarea_total_height(b, ps);
    if ta_h > b.content_height() {
        b.set_content_height(ta_h);
    }
    let mut baseline = f64::from(measured.baseline) / SCALE;
    if line_heights[0] > f64::from(measured_h) {
        baseline += (line_heights[0] - f64::from(measured_h)) / 2.0;
    }
    b.set_first_baseline(baseline);
    if cacheable {
        b.set_inline_layout_cache(Some((ps, content_width, b.content_height())));
    }

    if atomics {
        place_atomics(&layout, b, ps, content_width, &line_heights);
    }
}
