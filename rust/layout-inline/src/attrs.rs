//! Southstar — the ns-pango attributes an inline run is shaped with: spacing, inline font runs, atomic inline shapes, line-height struts and stretched text inputs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::{BoxKind, BoxRef, InlineAttr, inline_kind as k};
use southstar_paint::ffi::pango::{self, AttrList, Attribute, Layout, Rectangle};
use southstar_style::{PropId, StyleRef, ValueRef, font_family_for_pango};

use crate::ffi;

const SCALE: f64 = pango::SCALE_F;
const UNIT_PX: u32 = 0;

const STRUT_ROOT: u8 = 0;
const STRUT_SHORTER: u8 = 1;
const STRUT_SPACER: u8 = 2;

pub(crate) fn length_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    v.map_or(fallback, |v| v.length_or(fallback))
}

pub(crate) fn px_length(v: Option<ValueRef<'_>>) -> Option<f64> {
    v.and_then(ValueRef::length)
        .and_then(|(v, unit)| (unit == UNIT_PX).then_some(v))
}

pub(crate) fn font_size(s: Option<StyleRef<'_>>) -> f64 {
    length_or(s.and_then(|s| s.get(PropId::FontSize)), 16.0)
}

fn insert(list: &AttrList, a: Option<Attribute>, start: usize, len: usize) {
    if let Some(a) = a
        && len != 0
    {
        list.insert(a, start as u32, (start + len) as u32);
    }
}

pub(crate) fn apply_spacing(list: &AttrList, style: Option<StyleRef<'_>>, text: &[u8]) {
    let Some(style) = style else {
        return;
    };
    let ls_px = px_length(style.get(PropId::LetterSpacing)).unwrap_or(0.0);
    let ws_px = px_length(style.get(PropId::WordSpacing)).unwrap_or(0.0);
    if ls_px != 0.0 {
        list.insert(
            Attribute::letter_spacing((ls_px * SCALE) as i32),
            0,
            u32::MAX,
        );
    }
    if ws_px != 0.0 {
        let per_space = ((ls_px + ws_px) * SCALE) as i32;
        for (idx, _) in text.iter().enumerate().filter(|(_, b)| **b == b' ') {
            list.insert(
                Attribute::letter_spacing(per_space),
                idx as u32,
                idx as u32 + 1,
            );
        }
    }
}

fn weight_from_css(weight: i32) -> i32 {
    match weight {
        ..=100 => pango::WEIGHT_THIN,
        101..=200 => pango::WEIGHT_ULTRALIGHT,
        201..=300 => pango::WEIGHT_LIGHT,
        301..=400 => pango::WEIGHT_NORMAL,
        401..=500 => pango::WEIGHT_MEDIUM,
        501..=600 => pango::WEIGHT_SEMIBOLD,
        601..=700 => pango::WEIGHT_BOLD,
        701..=800 => pango::WEIGHT_ULTRABOLD,
        801..=900 => pango::WEIGHT_HEAVY,
        _ => weight,
    }
}

fn stretch_from_css(rank: i32) -> i32 {
    rank.clamp(0, 8)
}

fn dom_is(r: &InlineAttr, name: &CStr) -> bool {
    ffi::node(r.dom_ptr()).and_then(|n| n.name()) == Some(name)
}

pub(crate) fn is_textarea(r: &InlineAttr) -> bool {
    dom_is(r, c"textarea")
}

pub(crate) fn apply_layout_attrs(list: &AttrList, b: BoxRef<'_>) {
    for r in b.attrs().iter().rev() {
        let a = match r.kind {
            k::BOLD => Some(Attribute::weight(pango::WEIGHT_BOLD)),
            k::FONT_WEIGHT => Some(Attribute::weight(weight_from_css(r.font_weight))),
            k::FONT_STRETCH => Some(Attribute::stretch(stretch_from_css(r.font_stretch))),
            k::FONT_FEATURES => ffi::font_features_attr(r),
            k::FONT_VARIATIONS => ffi::font_variations_attr(r),
            k::ITALIC => Some(Attribute::style(pango::STYLE_ITALIC)),
            k::MONOSPACE => Some(Attribute::family(c"monospace")),
            k::INPUT_FIELD | k::INPUT_FIELD_FOCUSED | k::BUTTON => {
                if !is_textarea(r) {
                    insert(list, Some(Attribute::allow_breaks(false)), r.start, r.len);
                }
                None
            }
            k::FONT_SIZE => Some(Attribute::size_absolute(ffi::pango_font_size(
                r.font_size_px,
            ))),
            k::FONT_FAMILY => r
                .family()
                .and_then(font_family_for_pango)
                .map(|f| Attribute::family(&f)),
            k::SUPERSCRIPT => {
                insert(list, Some(Attribute::rise(4000)), r.start, r.len);
                Some(Attribute::scale(0.75))
            }
            k::SUBSCRIPT => {
                insert(list, Some(Attribute::rise(-3000)), r.start, r.len);
                Some(Attribute::scale(0.75))
            }
            k::SMALL_CAPS => Some(Attribute::variant(pango::VARIANT_SMALL_CAPS)),
            k::SPACER => Some(Attribute::shape_rect(Rectangle {
                x: 0,
                y: 0,
                width: (r.box_w * SCALE) as i32,
                height: 0,
            })),
            _ => None,
        };
        insert(list, a, r.start, r.len);
    }
}

pub(crate) fn outer_height(b: BoxRef<'_>) -> f64 {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    m.top + e.top + p.top + b.content_height() + p.bottom + e.bottom + m.bottom
}

fn outer_width(b: BoxRef<'_>) -> f64 {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    m.left + e.left + p.left + b.content_width() + p.right + e.right + m.right
}

fn vertical_align(b: BoxRef<'_>) -> Option<&CStr> {
    ffi::style(b.style()).and_then(|s| s.keyword_of(PropId::VerticalAlign))
}

fn atomic_ascent(ab: BoxRef<'_>) -> f64 {
    let h = outer_height(ab).max(0.0);
    let fs = font_size(ffi::style(ab.style()));
    let xh = fs * 0.5;
    let mut a_asc = h;
    if !ffi::clips_children(ab)
        && let Some(baseline) = ffi::first_baseline(ab)
    {
        a_asc = ab.margin().top + baseline;
    }
    if let Some(va) = vertical_align(ab) {
        match va.to_bytes() {
            b"middle" => a_asc = h / 2.0 + xh / 2.0,
            b"super" => a_asc = h + fs * 0.3,
            b"sub" => a_asc = h - fs * 0.2,
            b"top" | b"text-top" | b"bottom" | b"text-bottom" => a_asc = fs * 0.8,
            _ => {}
        }
    }
    a_asc
}

pub(crate) fn apply_atomic_shapes(list: &AttrList, b: BoxRef<'_>) {
    let Some(atomics) = b.inline_atomics() else {
        return;
    };
    let max_asc = atomics
        .iter()
        .filter_map(|a| a.box_ref())
        .map(atomic_ascent)
        .fold(0.0, |acc, a| if a > acc { a } else { acc });
    for a in atomics {
        let Some(ab) = a.box_ref() else {
            continue;
        };
        let w = outer_width(ab).max(0.0);
        let h = outer_height(ab).max(0.0);
        let fs = font_size(ffi::style(ab.style()));
        let (asc, desc, xh) = (fs * 0.8, fs * 0.2, fs * 0.5);
        let mut top = -h;
        if !ffi::clips_children(ab)
            && let Some(baseline) = ffi::first_baseline(ab)
        {
            top = -(ab.margin().top + baseline);
        }
        if ab.kind() == BoxKind::Math {
            let ma = ffi::math_ascent(ab.dom_ptr(), fs);
            top = -(ab.margin().top + ab.border().top + ab.padding().top + ma);
        }
        if let Some(va) = vertical_align(ab) {
            let line_asc = if max_asc > asc { max_asc } else { asc };
            match va.to_bytes() {
                b"middle" => top = -(h / 2.0 + xh / 2.0),
                b"text-top" => top = -asc,
                b"top" => top = -line_asc,
                b"text-bottom" | b"bottom" => top = desc - h,
                b"super" => top -= fs * 0.3,
                b"sub" => top += fs * 0.2,
                _ => {}
            }
        }
        let rect = Rectangle {
            x: 0,
            y: (top * SCALE) as i32,
            width: (w * SCALE) as i32,
            height: (h * SCALE) as i32,
        };
        let off = a.byte_off() as u32;
        list.insert(Attribute::shape_rect(rect), off, off + 3);
    }
}

fn insert_line_height(list: &AttrList, px: f64, start: u32, end: u32) {
    let px = px.clamp(0.0, f64::from(i16::MAX));
    list.insert(
        Attribute::line_height_absolute((px * SCALE).round() as i32),
        start,
        end,
    );
}

fn insert_spacer_line_heights(list: &AttrList, b: BoxRef<'_>) {
    for r in b.attrs() {
        if r.kind == k::SPACER && r.len > 0 {
            insert_line_height(list, 0.0, r.start as u32, (r.start + r.len) as u32);
        }
    }
}

fn apply_line_heights(list: &AttrList, b: Option<BoxRef<'_>>, strut_px: f64) -> bool {
    insert_line_height(list, strut_px, 0, u32::MAX);
    let Some(b) = b.filter(|b| b.has_attrs()) else {
        return false;
    };
    let mut has_shorter = false;
    for r in b.attrs().iter().rev() {
        if r.kind != k::ELEMENT || r.style().is_null() || r.len == 0 {
            continue;
        }
        let px = ffi::css_line_height_px(r.style());
        if px <= 0.0 || (px - strut_px).abs() < 0.01 {
            continue;
        }
        if px < strut_px {
            has_shorter = true;
        }
        insert_line_height(list, px, r.start as u32, (r.start + r.len) as u32);
    }
    insert_spacer_line_heights(list, b);
    has_shorter
}

fn mark_range(kind: &mut [u8], r: &InlineAttr, k: u8) {
    let end = r.start.saturating_add(r.len).min(kind.len());
    if r.start < end {
        kind[r.start..end].fill(k);
    }
}

fn strut_kinds(b: BoxRef<'_>, n: usize, strut_px: f64) -> Vec<u8> {
    let mut kind = vec![STRUT_ROOT; n];
    for r in b.attrs().iter().rev() {
        if r.kind != k::ELEMENT || r.style().is_null() {
            continue;
        }
        let px = ffi::css_line_height_px(r.style());
        let shorter = px > 0.0 && px < strut_px - 0.01;
        mark_range(
            &mut kind,
            r,
            if shorter { STRUT_SHORTER } else { STRUT_ROOT },
        );
    }
    for r in b.attrs() {
        if r.kind == k::SPACER {
            mark_range(&mut kind, r, STRUT_SPACER);
        }
    }
    kind
}

fn line_lacks_root(kind: &[u8]) -> bool {
    let mut shorter = false;
    for &k in kind {
        if k == STRUT_ROOT {
            return false;
        }
        if k == STRUT_SHORTER {
            shorter = true;
        }
    }
    shorter
}

fn restore_strut_lines(layout: &Layout, list: &AttrList, b: BoxRef<'_>, strut_px: f64) {
    let Some(text) = b.text() else {
        return;
    };
    let n = text.to_bytes().len();
    if n == 0 {
        return;
    }
    let kind = strut_kinds(b, n, strut_px);
    if layout.text().is_none_or(CStr::is_empty) {
        layout.set_text(text);
    }
    let mut with_struts: Option<AttrList> = None;
    let mut iter = layout.iter();
    loop {
        if let Some(line) = iter.line()
            && line.length() > 0
        {
            let s0 = line.start_index() as usize;
            let s1 = n.min(s0 + line.length() as usize);
            if line_lacks_root(kind.get(s0..s1).unwrap_or(&[])) {
                let ws = with_struts.get_or_insert_with(|| list.copy());
                insert_line_height(ws, strut_px, s0 as u32, s1 as u32);
            }
        }
        if !iter.next_line() {
            break;
        }
    }
    drop(iter);
    let Some(with_struts) = with_struts else {
        return;
    };
    insert_spacer_line_heights(&with_struts, b);
    layout.set_attributes(&with_struts);
}

fn is_text_input(r: &InlineAttr) -> bool {
    if r.kind != k::INPUT_FIELD && r.kind != k::INPUT_FIELD_FOCUSED {
        return false;
    }
    let Some(n) = ffi::node(r.dom_ptr()) else {
        return false;
    };
    if n.name() != Some(c"input") {
        return false;
    }
    let Some(ty) = n.attr(c"type").map(CStr::to_bytes) else {
        return true;
    };
    ty.is_empty()
        || [
            &b"text"[..],
            b"search",
            b"email",
            b"url",
            b"tel",
            b"number",
            b"password",
        ]
        .iter()
        .any(|t| ty.eq_ignore_ascii_case(t))
}

pub(crate) fn set_attrs(layout: &Layout, list: Option<&AttrList>, b: Option<BoxRef<'_>>) {
    let strut_px = layout.css_line_height(ffi::LINE_HEIGHT_KEY);
    let has_shorter = match (strut_px, list) {
        (Some(strut), Some(list)) => apply_line_heights(list, b, strut),
        _ => false,
    };
    match list {
        Some(list) => layout.set_attributes(list),
        None => layout.clear_attributes(),
    }
    let Some(b) = b.filter(|b| b.has_attrs()) else {
        return;
    };
    let mut stretched = false;
    for r in b.attrs() {
        if !is_text_input(r) || r.len < 4 {
            continue;
        }
        let css_w = ffi::control_width(r, b);
        if css_w <= 0.0 {
            continue;
        }
        let p0 = layout.index_to_pos(r.start as i32);
        let p1 = layout.index_to_pos((r.start + r.len - 2) as i32);
        if p1.y != p0.y {
            continue;
        }
        let prefix = f64::from(p1.x - p0.x) / SCALE;
        if prefix < 0.0 {
            continue;
        }
        let fs = font_size(ffi::style(r.style()));
        let w = css_w - prefix;
        if w < fs * 0.4 {
            continue;
        }
        let rect = Rectangle {
            x: 0,
            y: (-fs * 0.8 * SCALE) as i32,
            width: (w * SCALE) as i32,
            height: (fs * SCALE) as i32,
        };
        if let Some(list) = list {
            list.insert(
                Attribute::shape_rect(rect),
                (r.start + r.len - 2) as u32,
                (r.start + r.len) as u32,
            );
        }
        stretched = true;
    }
    if stretched {
        layout.context_changed();
    }
    if has_shorter && let (Some(list), Some(strut)) = (list, strut_px) {
        restore_strut_lines(layout, list, b, strut);
    }
}
