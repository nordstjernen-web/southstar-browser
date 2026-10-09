//! Southstar — units resolved in a computed style: the element's font size in px, then em, rem, ex, ch, viewport and container units, percentages in line-height, and infinite or NaN calc() results clamped, across lengths, calc() values, shadows, grid tracks, transforms and sizes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::calc;
use crate::container;
use crate::ffi::{ComputedStyle, PROP_COUNT, RawCalc, Slot, SlotMut, StyleView, font_relative_px};
use crate::font;
use crate::grid::Tracks;
use crate::prop::Prop;
use crate::shadow::ShadowList;
use crate::transform::{self, Transform};
use crate::units::{
    self, CAP, CH, CQH, CQMAX, CQMIN, CQW, EM, EX, IC, LH, NUMBER, PERCENT, PX, RCAP, RCH, REM,
    REX, RIC, RLH, VH, VMAX, VMIN, VW,
};

const CALC_LIMIT: f64 = 33554400.0;
const FONT_SIZE: usize = Prop::FontSize as usize;
const FONT_WEIGHT: usize = Prop::FontWeight as usize;
const FONT_STYLE: usize = Prop::FontStyle as usize;
const FONT_FAMILY: usize = Prop::FontFamily as usize;
const LINE_HEIGHT: usize = Prop::LineHeight as usize;

struct Font<'a> {
    family: Option<&'a CStr>,
    weight: i32,
    italic: bool,
}

trait Values {
    fn slot(&self, prop: usize) -> Option<Slot<'_>>;

    fn keyword(&self, prop: usize) -> Option<&CStr> {
        match self.slot(prop) {
            Some(Slot::Keyword(kw)) => kw,
            _ => None,
        }
    }

    fn font(&self) -> Font<'_> {
        let style = self.keyword(FONT_STYLE).map(CStr::to_bytes);
        Font {
            family: self.keyword(FONT_FAMILY),
            weight: font::weight_number(self.keyword(FONT_WEIGHT).map(CStr::to_bytes), 400),
            italic: style == Some(b"italic") || style == Some(b"oblique"),
        }
    }

    fn font_px(&self) -> Option<f64> {
        match self.slot(FONT_SIZE) {
            Some(Slot::Length(l)) if l.unit == PX => Some(l.v),
            _ => None,
        }
    }
}

impl Values for ComputedStyle<'_> {
    fn slot(&self, prop: usize) -> Option<Slot<'_>> {
        self.get(prop)
    }
}

impl Values for StyleView<'_> {
    fn slot(&self, prop: usize) -> Option<Slot<'_>> {
        self.get(prop)
    }
}

fn args(c: &RawCalc) -> usize {
    usize::from(c.n_args).min(c.args.len())
}

fn counted(n: i32, cap: usize) -> usize {
    usize::try_from(n).unwrap_or(0).min(cap)
}

fn viewport_coeff_px(c: &RawCalc, width: f64, height: f64) -> f64 {
    let min = if width < height { width } else { height };
    let max = if width > height { width } else { height };
    (c.vw * width + c.vh * height + c.vmin * min + c.vmax * max) / 100.0
}

fn viewport_refresh_px(c: &RawCalc) -> f64 {
    if c.vw == 0.0 && c.vh == 0.0 && c.vmin == 0.0 && c.vmax == 0.0 {
        return 0.0;
    }
    let (w, h) = crate::ffi::viewport();
    viewport_coeff_px(c, w, h) - viewport_coeff_px(c, c.parsed_vw, c.parsed_vh)
}

fn font_size_px(out: &ComputedStyle<'_>, parent: Option<&StyleView<'_>>) -> f64 {
    let parent_px = parent.and_then(Values::font_px).unwrap_or(16.0);
    let length = match out.slot(FONT_SIZE) {
        Some(Slot::Calc(c)) => {
            return c.px
                + c.em * parent_px
                + c.rem * parent_px
                + c.pct * parent_px / 100.0
                + viewport_refresh_px(c);
        }
        Some(Slot::Length(length)) => length,
        _ => return parent_px,
    };
    let v = length.v;
    match length.unit {
        PX | NUMBER => v,
        EM | REM => v * parent_px,
        PERCENT => v * parent_px / 100.0,
        LH => v * parent_px * 1.5,
        RLH => v * 24.0,
        EX | CH | CAP | IC | REX | RCH | RCAP | RIC => {
            let font = parent.map_or(
                Font {
                    family: None,
                    weight: 400,
                    italic: false,
                },
                Values::font,
            );
            v * font_relative_px(
                length.unit,
                parent_px,
                font.family,
                font.weight,
                font.italic,
            )
        }
        VW | VH | VMIN | VMAX => units::viewport_resolve(v, length.unit),
        CQW | CQH | CQMIN | CQMAX => container::unit_resolve(v, length.unit),
        _ => parent_px,
    }
}

fn clamp_finite(v: f64) -> f64 {
    if v.is_nan() {
        0.0
    } else if v.is_infinite() {
        if v < 0.0 { -CALC_LIMIT } else { CALC_LIMIT }
    } else {
        v
    }
}

fn moves_or_scales(op: &transform::Op) -> bool {
    op.kind == transform::TRANSLATE || op.kind == transform::SCALE
}

fn transform_ops(t: &Transform) -> &[transform::Op] {
    &t.ops[..counted(t.n_ops, t.ops.len())]
}

fn is_finite(slot: Slot<'_>) -> bool {
    match slot {
        Slot::Length(l) => l.v.is_finite(),
        Slot::Transform(t) => transform_ops(t)
            .iter()
            .filter(|op| moves_or_scales(op))
            .all(|op| {
                [op.a, op.b, op.c, op.a_pct, op.b_pct]
                    .iter()
                    .all(|v| v.is_finite())
            }),
        Slot::Calc(c) => {
            [c.px, c.pct, c.em, c.rem].iter().all(|v| v.is_finite())
                && c.args[..args(c)]
                    .iter()
                    .all(|a| a.px.is_finite() && a.pct.is_finite())
        }
        _ => true,
    }
}

fn clamp_value(slot: SlotMut<'_>) {
    match slot {
        SlotMut::Length(l) => l.v = clamp_finite(l.v),
        SlotMut::Transform(t) => {
            let n = counted(t.n_ops, t.ops.len());
            for op in t.ops[..n].iter_mut().filter(|op| moves_or_scales(op)) {
                for v in [
                    &mut op.a,
                    &mut op.b,
                    &mut op.c,
                    &mut op.a_pct,
                    &mut op.b_pct,
                ] {
                    *v = clamp_finite(*v);
                }
            }
        }
        SlotMut::Calc(c) => {
            for v in [&mut c.px, &mut c.pct, &mut c.em, &mut c.rem] {
                *v = clamp_finite(*v);
            }
            let n = args(c);
            for a in &mut c.args[..n] {
                a.px = clamp_finite(a.px);
                a.pct = clamp_finite(a.pct);
            }
        }
        _ => {}
    }
}

fn shadows_need(s: &ShadowList) -> bool {
    s.s[..counted(s.n, s.s.len())]
        .iter()
        .any(|sh| (0..4).any(|m| sh.em[m] != 0.0 || sh.rem[m] != 0.0))
}

fn tracks_need(t: &Tracks) -> bool {
    t.tracks[..counted(t.n, t.tracks.len())]
        .iter()
        .any(|tr| tr.em != 0.0 || tr.rem != 0.0 || tr.min_em != 0.0 || tr.min_rem != 0.0)
}

fn transform_needs(t: &Transform) -> bool {
    transform_ops(t)
        .iter()
        .any(|op| (0..3).any(|m| op.em[m] != 0.0 || op.rem[m] != 0.0))
}

fn has_percent(c: &RawCalc) -> bool {
    c.pct != 0.0 || c.args[..args(c)].iter().any(|a| a.pct != 0.0)
}

fn fold_percent(c: &mut RawCalc, basis: f64) {
    if c.func != 0 && c.n_args != 0 {
        let n = args(c);
        for a in &mut c.args[..n] {
            a.px += a.pct * basis / 100.0;
            a.pct = 0.0;
        }
        c.px = calc::math_fn_px(&c.to_calc(), basis);
        c.func = 0;
        c.n_args = 0;
        c.arg_none = 0;
    } else {
        c.px += c.pct * basis / 100.0;
    }
    c.pct = 0.0;
}

struct Basis<'a> {
    font_px: f64,
    root_px: f64,
    font: Font<'a>,
}

enum Change {
    Shadows,
    Tracks,
    Transform,
    Calc { refresh: f64, line_pct: bool },
    Size,
    Length,
}

fn change_for(prop: usize, slot: Slot<'_>) -> Option<Change> {
    match slot {
        Slot::Shadow(s) => shadows_need(s).then_some(Change::Shadows),
        Slot::Tracks(t) => tracks_need(t).then_some(Change::Tracks),
        Slot::Transform(t) => transform_needs(t).then_some(Change::Transform),
        Slot::Calc(c) => {
            let refresh = viewport_refresh_px(c);
            let line_pct = prop == LINE_HEIGHT && has_percent(c);
            let fixed = c.em == 0.0
                && c.rem == 0.0
                && c.vw == 0.0
                && c.vh == 0.0
                && c.vmin == 0.0
                && c.vmax == 0.0
                && !line_pct;
            (!fixed).then_some(Change::Calc { refresh, line_pct })
        }
        Slot::Size(s) if s.w_auto == 0 && s.h_auto == 0 => {
            let font_relative = |unit| unit == EM || unit == REM;
            (font_relative(s.w_unit) || font_relative(s.h_unit)).then_some(Change::Size)
        }
        Slot::Length(l) => match l.unit {
            PERCENT if prop == LINE_HEIGHT => Some(Change::Length),
            EM | REM | VW | VH | VMIN | VMAX | EX | CH | CAP | IC => Some(Change::Length),
            _ => None,
        },
        _ => None,
    }
}

fn apply(change: Change, slot: SlotMut<'_>, b: &Basis<'_>) {
    let scaled = |em: f64, rem: f64| em * b.font_px + rem * b.root_px;
    match (change, slot) {
        (Change::Shadows, SlotMut::Shadow(list)) => {
            let n = counted(list.n, list.s.len());
            for sh in &mut list.s[..n] {
                let fields = [&mut sh.x, &mut sh.y, &mut sh.blur, &mut sh.spread];
                for (m, field) in fields.into_iter().enumerate() {
                    *field += scaled(sh.em[m], sh.rem[m]);
                }
                sh.em = [0.0; 4];
                sh.rem = [0.0; 4];
                sh.blur = sh.blur.clamp(0.0, 1000.0);
            }
        }
        (Change::Tracks, SlotMut::Tracks(t)) => {
            let n = counted(t.n, t.tracks.len());
            for tr in &mut t.tracks[..n] {
                tr.v += scaled(tr.em, tr.rem);
                tr.em = 0.0;
                tr.rem = 0.0;
                tr.min_v += scaled(tr.min_em, tr.min_rem);
                tr.min_em = 0.0;
                tr.min_rem = 0.0;
            }
        }
        (Change::Transform, SlotMut::Transform(t)) => {
            let n = counted(t.n_ops, t.ops.len());
            for op in &mut t.ops[..n] {
                let axes = [&mut op.a, &mut op.b, &mut op.c];
                for (m, axis) in axes.into_iter().enumerate() {
                    *axis += scaled(op.em[m], op.rem[m]);
                }
                op.em = [0.0; 3];
                op.rem = [0.0; 3];
            }
        }
        (Change::Calc { refresh, line_pct }, SlotMut::Calc(c)) => {
            c.px += c.em * b.font_px + c.rem * b.root_px + refresh;
            c.em = 0.0;
            c.rem = 0.0;
            c.vw = 0.0;
            c.vh = 0.0;
            c.vmin = 0.0;
            c.vmax = 0.0;
            if line_pct {
                fold_percent(c, b.font_px);
            }
        }
        (Change::Size, SlotMut::Size(s)) => {
            let factor = |unit| if unit == EM { b.font_px } else { b.root_px };
            if s.w_unit == EM || s.w_unit == REM {
                s.w *= factor(s.w_unit);
                s.w_unit = PX;
            }
            if s.h_unit == EM || s.h_unit == REM {
                s.h *= factor(s.h_unit);
                s.h_unit = PX;
            }
        }
        (Change::Length, SlotMut::Length(l)) => {
            match l.unit {
                PERCENT => l.v *= b.font_px / 100.0,
                EM => l.v *= b.font_px,
                REM => l.v *= b.root_px,
                VW | VH | VMIN | VMAX => l.v = units::viewport_resolve(l.v, l.unit),
                _ => {
                    l.v *= font_relative_px(
                        l.unit,
                        b.font_px,
                        b.font.family,
                        b.font.weight,
                        b.font.italic,
                    )
                }
            }
            l.unit = PX;
        }
        _ => {}
    }
}

fn own_font_px(out: &ComputedStyle<'_>, parent: Option<&StyleView<'_>>, root_px: f64) -> f64 {
    let mut font_px = font_size_px(out, parent);
    if font_px.is_nan() || font_px < 0.0 {
        font_px = 0.0;
    }
    let rem_px = if root_px > 0.0 { root_px } else { 16.0 };
    match out.slot(FONT_SIZE) {
        Some(Slot::Length(l)) if l.unit == REM => font_px = l.v * rem_px,
        Some(Slot::Calc(c)) if c.rem != 0.0 => {
            let parent_px = parent.and_then(Values::font_px).unwrap_or(16.0);
            font_px = c.px
                + c.em * parent_px
                + c.rem * rem_px
                + c.pct * parent_px / 100.0
                + viewport_refresh_px(c);
        }
        _ => {}
    }
    if font_px.is_nan() || font_px < 0.0 {
        0.0
    } else {
        font_px
    }
}

pub(crate) fn resolve(out: &mut ComputedStyle<'_>, parent: Option<&StyleView<'_>>, root_px: f64) {
    let font_px = own_font_px(out, parent, root_px);
    let root_px = if root_px <= 0.0 { font_px } else { root_px };
    out.set_length_px(FONT_SIZE, font_px);
    let font = out.font();
    let family = font.family.map(CStr::to_owned);
    let basis = Basis {
        font_px,
        root_px,
        font: Font {
            family: family.as_deref(),
            weight: font.weight,
            italic: font.italic,
        },
    };
    for prop in (0..PROP_COUNT).filter(|&p| p != FONT_SIZE) {
        let Some(slot) = out.slot(prop) else {
            continue;
        };
        if !is_finite(slot) {
            if let Some(slot) = out.make_mut(prop) {
                clamp_value(slot);
            }
        }
        let Some(change) = out.slot(prop).and_then(|slot| change_for(prop, slot)) else {
            continue;
        };
        if let Some(slot) = out.make_mut(prop) {
            apply(change, slot, &basis);
        }
    }
}

pub(crate) fn style_font_px(style: &StyleView<'_>) -> f64 {
    style.font_px().unwrap_or(16.0)
}
