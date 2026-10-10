//! Southstar — the min-content and max-content widths of an inline run, with the plain-ASCII longest-word shortcut and the per-box caches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::{BoxRef, Style, inline_kind as k};
use southstar_paint::ffi::pango;
use southstar_style::{PropId, StyleRef};

use crate::attrs::px_length;
use crate::layout::{
    has_atomics, measure_cacheable, shaping_attrs, vertical_measure, white_space_nowrap,
};
use crate::{ffi, measure};

const SCALE: f64 = pango::SCALE_F;

fn affects_measure(kind: u32) -> bool {
    matches!(
        kind,
        k::BOLD
            | k::ITALIC
            | k::MONOSPACE
            | k::FONT_SIZE
            | k::FONT_WEIGHT
            | k::FONT_STRETCH
            | k::FONT_FEATURES
            | k::FONT_VARIATIONS
            | k::FONT_FAMILY
            | k::SUPERSCRIPT
            | k::SUBSCRIPT
            | k::SMALL_CAPS
    )
}

fn keyword_other_than(s: StyleRef<'_>, prop: PropId, allowed: &[&CStr]) -> bool {
    s.keyword_of(prop).is_some_and(|kw| !allowed.contains(&kw))
}

fn has_measure_adjustments(style: Option<StyleRef<'_>>) -> bool {
    let Some(s) = style else {
        return false;
    };
    let nonzero = |p| px_length(s.get(p)).is_some_and(|v| v.abs() > 0.001);
    nonzero(PropId::LetterSpacing)
        || nonzero(PropId::WordSpacing)
        || keyword_other_than(s, PropId::FontKerning, &[c"auto", c"normal"])
        || keyword_other_than(s, PropId::FontVariantLigatures, &[c"normal"])
        || keyword_other_than(s, PropId::FontFeatureSettings, &[c"normal"])
        || keyword_other_than(s, PropId::FontVariationSettings, &[c"normal"])
}

fn longest_word(text: &[u8]) -> (usize, usize) {
    let (mut best, mut best_len) = (0, 0);
    let (mut run, mut run_len) = (0, 0);
    for (i, &c) in text.iter().chain(core::iter::once(&0)).enumerate() {
        if matches!(c, 0 | b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
            if run_len > best_len {
                best = run;
                best_len = run_len;
            }
            run_len = 0;
        } else {
            if run_len == 0 {
                run = i;
            }
            run_len += 1;
        }
    }
    (best, best_len)
}

fn ascii_min_width(b: BoxRef<'_>, ps: *const Style, text: &CStr) -> f64 {
    let style = ffi::style(ps);
    if has_atomics(b)
        || b.attrs().iter().any(|a| affects_measure(a.kind))
        || has_measure_adjustments(style)
        || !text.to_bytes().is_ascii()
    {
        return -1.0;
    }
    if white_space_nowrap(style) {
        return natural_width(b, ps);
    }
    let bytes = text.to_bytes();
    let (best, best_len) = longest_word(bytes);
    if best_len == 0 {
        return 0.0;
    }
    if best_len == bytes.len() {
        return natural_width(b, ps);
    }
    let layout = ffi::new_layout(ps);
    layout.set_width(-1);
    layout.set_text_bytes(&bytes[best..best + best_len]);
    let m = measure::measure(&layout);
    (f64::from(m.logical.width) / SCALE).ceil()
}

fn atomics_outer_width_sum(b: BoxRef<'_>, ps: *const Style) -> f64 {
    let saved = ffi::measure_atomics_begin(b, ps, true);
    let mut sum = 0.0;
    for ab in b
        .inline_atomics()
        .unwrap_or(&[])
        .iter()
        .filter_map(|a| a.box_ref())
    {
        let (m, p, e) = (ab.margin(), ab.padding(), ab.border());
        sum += ab.content_width() + m.left + m.right + p.left + p.right + e.left + e.right;
    }
    ffi::measure_atomics_end(saved);
    sum
}

pub(crate) fn natural_width(b: BoxRef<'_>, ps: *const Style) -> f64 {
    let text = b.text().filter(|t| !t.is_empty());
    if text.is_some() && ffi::writing_mode(ps) != 0 && !has_atomics(b) {
        return vertical_measure(b, ps).0;
    }
    let Some(text) = text else {
        if !has_atomics(b) {
            return 0.0;
        }
        return atomics_outer_width_sum(b, ps);
    };
    let cacheable = measure_cacheable(b);
    if cacheable
        && let Some((cs, w)) = b.inline_natural_cache()
        && cs == ps
    {
        return w;
    }
    let layout = ffi::new_layout(ps);
    layout.set_width(-1);
    let saved = ffi::measure_atomics_begin(b, ps, true);
    layout.set_text(text);
    shaping_attrs(&layout, b, ps, text.to_bytes());
    let natural = measure::measure(&layout);
    let slack = px_length(ffi::style(ps).and_then(|s| s.get(PropId::LetterSpacing)))
        .filter(|&v| v > 0.0)
        .unwrap_or(0.0);
    let mut pw = (f64::from(natural.logical.width) / SCALE + slack).ceil();
    for a in b.inline_atomics().unwrap_or(&[]) {
        if a.box_ref().is_none() {
            continue;
        }
        let pos = layout.index_to_pos(a.byte_off() as i32);
        let end = f64::from(pos.x + pos.width) / SCALE;
        if pw < end {
            pw = end.ceil();
        }
    }
    ffi::measure_atomics_end(saved);
    drop(layout);
    if cacheable {
        b.set_inline_natural_cache(ps, pw);
    }
    pw
}

pub(crate) fn min_width(b: BoxRef<'_>, ps: *const Style) -> f64 {
    let Some(text) = b.text().filter(|t| !t.is_empty()) else {
        return 0.0;
    };
    if ffi::writing_mode(ps) != 0 && !has_atomics(b) {
        return vertical_measure(b, ps).0;
    }
    if white_space_nowrap(ffi::style(ps)) {
        return natural_width(b, ps);
    }
    let cacheable = measure_cacheable(b);
    if cacheable
        && let Some((cs, w)) = b.inline_min_cache()
        && cs == ps
    {
        return w;
    }
    let fast = ascii_min_width(b, ps, text);
    if fast >= 0.0 {
        if cacheable {
            b.set_inline_min_cache(ps, fast);
        }
        return fast;
    }
    let layout = ffi::new_layout(ps);
    layout.set_width(1);
    layout.set_wrap(pango::WRAP_WORD);
    let saved = ffi::measure_atomics_begin(b, ps, false);
    layout.set_text(text);
    shaping_attrs(&layout, b, ps, text.to_bytes());
    let pw = f64::from(measure::measure(&layout).pixel_logical().width);
    ffi::measure_atomics_end(saved);
    drop(layout);
    if cacheable {
        b.set_inline_min_cache(ps, pw);
    }
    pw
}
