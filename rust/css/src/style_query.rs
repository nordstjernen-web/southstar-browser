//! Southstar — questions layout and paint ask of a computed style: writing mode and text orientation, lengths in px, the used column count and gap, alignment keywords without their safe/unsafe/legacy prefix, and the overflow keyword an axis is used with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::calc;
use crate::container;
use crate::ffi::{RawCalc, Slot, StyleView, font_relative_px, viewport};
use crate::prop::Prop;
use crate::units::{
    self, CAP, CH, EM, EX, IC, LH, NUMBER, PERCENT, PX, RCAP, RCH, REM, REX, RIC, RLH, VH, VW,
};

fn keyword<'a>(style: &'a StyleView<'a>, prop: Prop) -> Option<&'a CStr> {
    match style.get(prop.id()) {
        Some(Slot::Keyword(kw)) => kw,
        _ => None,
    }
}

pub(crate) fn writing_mode(style: Option<&StyleView<'_>>) -> i32 {
    match style
        .and_then(|s| keyword(s, Prop::WritingMode))
        .map(CStr::to_bytes)
    {
        Some(b"vertical-rl" | b"sideways-rl" | b"tb-rl" | b"tb") => 1,
        Some(b"vertical-lr" | b"sideways-lr") => 2,
        _ => 0,
    }
}

pub(crate) fn text_orientation(style: Option<&StyleView<'_>>) -> i32 {
    match style
        .and_then(|s| keyword(s, Prop::TextOrientation))
        .map(CStr::to_bytes)
    {
        Some(b"upright") => 1,
        Some(b"sideways" | b"sideways-right") => 2,
        _ => 0,
    }
}

fn math_fn(c: &RawCalc) -> bool {
    c.func != 0 && c.n_args > 0
}

pub(crate) fn dimension_px(value: Option<Slot<'_>>, font_size: f64, basis: f64) -> f64 {
    let length = match value {
        Some(Slot::Calc(c)) if math_fn(c) && basis > 0.0 => {
            let out = calc::math_fn_px(&c.to_calc(), basis);
            return if out > 0.0 { out } else { 0.0 };
        }
        Some(Slot::Calc(c)) => {
            let mut out = c.px;
            if basis > 0.0 {
                out += c.pct * basis / 100.0;
            }
            return if out > 0.0 { out } else { 0.0 };
        }
        Some(Slot::Length(l)) => l,
        _ => return 0.0,
    };
    let n = length.v;
    match length.unit {
        PX | NUMBER => n,
        EM => n * font_size,
        REM => n * 16.0,
        PERCENT => {
            if basis > 0.0 {
                n * basis / 100.0
            } else {
                0.0
            }
        }
        EX | CH | CAP | IC => n * font_relative_px(length.unit, font_size, None, 400, false),
        LH => n * font_size * 1.5,
        RLH => n * 24.0,
        REX | RCH => n * 8.0,
        RCAP => n * 11.2,
        RIC => n * 16.0,
        unit => {
            let r = container::unit_resolve(n, unit);
            if r != 0.0 {
                r
            } else {
                units::viewport_resolve(n, unit)
            }
        }
    }
}

fn column_len_px(value: Option<Slot<'_>>, basis: f64, fallback: f64) -> f64 {
    let length = match value {
        None => return fallback,
        Some(Slot::Calc(c)) if math_fn(c) => return calc::math_fn_px(&c.to_calc(), basis),
        Some(Slot::Calc(c)) => return c.pct / 100.0 * basis + c.px,
        Some(Slot::Length(l)) => l,
        Some(_) => return fallback,
    };
    let (w, h) = viewport();
    match length.unit {
        PX | NUMBER => length.v,
        EM | REM => length.v * 16.0,
        PERCENT => length.v * basis / 100.0,
        VW => length.v * w / 100.0,
        VH => length.v * h / 100.0,
        _ => fallback,
    }
}

pub(crate) fn used_column_count(style: Option<&StyleView<'_>>, avail_w: f64) -> (i32, f64) {
    let Some(style) = style else {
        return (1, 16.0);
    };
    let mut gap = 16.0;
    let column_gap = style
        .get(Prop::ColumnGap.id())
        .filter(|v| matches!(v, Slot::Length(_)));
    if let Some(g) = column_gap.or_else(|| style.get(Prop::Gap.id())) {
        let g = column_len_px(Some(g), avail_w, -1.0);
        if g >= 0.0 {
            gap = g;
        }
    }
    let mut n = 1;
    if let Some(Slot::Length(count)) = style.get(Prop::ColumnCount.id()) {
        if count.v >= 2.0 {
            n = (count.v + 0.5) as i32;
        }
    }
    if n == 1 {
        if let Some(width @ Slot::Length(_)) = style.get(Prop::ColumnWidth.id()) {
            let colw = column_len_px(Some(width), avail_w, 0.0);
            if colw > 1.0 && avail_w > colw + gap {
                let fit = ((avail_w + gap) / (colw + gap)) as i32;
                if fit > 1 {
                    n = fit;
                }
            }
        }
    }
    (n, gap)
}

pub(crate) fn alignment_base(kw: &CStr) -> &CStr {
    let bytes = kw.to_bytes_with_nul();
    [&b"safe "[..], b"unsafe ", b"legacy "]
        .iter()
        .find(|prefix| bytes.starts_with(prefix))
        .and_then(|prefix| CStr::from_bytes_with_nul(&bytes[prefix.len()..]).ok())
        .unwrap_or(kw)
}

fn is_alignment(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::JustifyContent
            | Prop::AlignItems
            | Prop::AlignSelf
            | Prop::AlignContent
            | Prop::JustifyItems
            | Prop::JustifySelf
    )
}

pub(crate) fn style_keyword<'a>(style: &'a StyleView<'a>, prop: Prop) -> Option<&'a CStr> {
    let kw = keyword(style, prop)?;
    Some(if is_alignment(prop) {
        alignment_base(kw)
    } else {
        kw
    })
}

pub(crate) fn overflow_keyword<'a>(style: Option<&'a StyleView<'a>>, axis: Prop) -> &'a CStr {
    let lookup = |prop| style.and_then(|s| style_keyword(s, prop));
    let value = lookup(axis)
        .or_else(|| lookup(Prop::Overflow))
        .unwrap_or(c"visible");
    let other_axis = if axis == Prop::OverflowX {
        Prop::OverflowY
    } else {
        Prop::OverflowX
    };
    let other = lookup(other_axis)
        .or_else(|| lookup(Prop::Overflow))
        .unwrap_or(c"visible")
        .to_bytes();
    let other_scrollable =
        !other.eq_ignore_ascii_case(b"visible") && !other.eq_ignore_ascii_case(b"clip");
    let used = value.to_bytes();
    if other_scrollable && used.eq_ignore_ascii_case(b"visible") {
        c"auto"
    } else if other_scrollable && used.eq_ignore_ascii_case(b"clip") {
        c"hidden"
    } else {
        value
    }
}
