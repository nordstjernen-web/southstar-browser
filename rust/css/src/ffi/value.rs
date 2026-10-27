//! Southstar — a parsed or computed CSS value: its payload for each value kind, its specified and image-set text and further layers, and the shared reference styles, declarations and other crates hold on one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint};
use core::ops::Deref;
use std::ffi::CString;
use std::rc::Rc;

use southstar_css_color::{Color, Space};

use super::animation::RawList;
use crate::calc::{Calc, CalcArg, Parsed};
use crate::gradient::Gradient;
use crate::grid::{AREAS_MAX, Areas, Tracks};
use crate::shadow::ShadowList;
use crate::transform::Transform;

pub(crate) const KIND_KEYWORD: c_uint = 0;
pub(crate) const KIND_LENGTH: c_uint = 1;
pub(crate) const KIND_SIZE: c_uint = 2;
pub(crate) const KIND_COLOR: c_uint = 3;
pub(crate) const KIND_CALC: c_uint = 4;
pub(crate) const KIND_SHADOW: c_uint = 5;
pub(crate) const KIND_GRADIENT: c_uint = 6;
pub(crate) const KIND_TRACKS: c_uint = 7;
pub(crate) const KIND_URL: c_uint = 8;
pub(crate) const KIND_TRANSFORM: c_uint = 9;
pub(crate) const KIND_AREAS: c_uint = 10;
pub(crate) const KIND_ANIM: c_uint = 11;
pub(crate) const KIND_RECT: c_uint = 12;

#[derive(Clone, Copy)]
pub(crate) struct Length {
    pub v: f64,
    pub unit: c_uint,
}

#[derive(Clone, Copy)]
pub(crate) struct RawCalcArg {
    pub px: f64,
    pub pct: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct RawCalc {
    pub pct: f64,
    pub px: f64,
    pub em: f64,
    pub rem: f64,
    pub vw: f64,
    pub vh: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub parsed_vw: f64,
    pub parsed_vh: f64,
    pub func: u8,
    pub n_args: u8,
    pub arg_none: u8,
    pub args: [RawCalcArg; 4],
}

#[derive(Clone, Default)]
pub struct RawAreaRect {
    name: Option<CString>,
    pub r0: c_int,
    pub r1: c_int,
    pub c0: c_int,
    pub c1: c_int,
}

impl RawAreaRect {
    pub fn name(&self) -> Option<&CStr> {
        self.name.as_deref()
    }
}

#[derive(Clone)]
pub struct RawAreas {
    pub n_rows: c_int,
    pub n_cols: c_int,
    pub n_rects: c_int,
    pub rects: [RawAreaRect; AREAS_MAX],
}

#[derive(Clone, Copy)]
pub(crate) struct RawSize {
    pub w: f64,
    pub h: f64,
    pub w_unit: c_uint,
    pub h_unit: c_uint,
    pub w_auto: c_int,
    pub h_auto: c_int,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct RawColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub space: u8,
    pub missing: u8,
    pub legacy: u8,
    pub tagged: u8,
    pub components: [f64; 3],
    pub alpha: f64,
}

impl RawColor {
    pub(crate) fn from_color(color: &Color) -> RawColor {
        let [r, g, b, a] = color.to_rgba8();
        let mut missing = 0;
        for (bit, component) in color.components.iter().enumerate() {
            if component.is_none() {
                missing |= 1 << bit;
            }
        }
        if color.alpha.is_none() {
            missing |= 1 << 3;
        }
        RawColor {
            r,
            g,
            b,
            a,
            space: color.space.index(),
            missing,
            legacy: u8::from(color.legacy),
            tagged: 1,
            components: color.values(),
            alpha: color.alpha_value(),
        }
    }

    pub(crate) fn to_color(self) -> Color {
        let Some(space) = Space::from_index(self.space).filter(|_| self.tagged != 0) else {
            return Color::from_rgba8([self.r, self.g, self.b, self.a]);
        };
        let present = |bit: u8| self.missing & (1 << bit) == 0;
        Color {
            space,
            components: core::array::from_fn(|i| present(i as u8).then_some(self.components[i])),
            alpha: present(3).then_some(self.alpha),
            legacy: self.legacy != 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RawRect {
    pub v: [f64; 4],
    pub unit: [c_uint; 4],
    pub is_auto: [c_int; 4],
}

#[derive(Clone)]
pub(crate) enum Payload {
    Keyword(Option<CString>),
    Length(Length),
    Size(RawSize),
    Color(RawColor),
    Calc(RawCalc),
    Shadow(Box<ShadowList>),
    Gradient(Box<Gradient>),
    Tracks(Box<Tracks>),
    Url(Option<CString>),
    Transform(Box<Transform>),
    Areas(Box<RawAreas>),
    Anim(Box<RawList>),
    Rect(RawRect),
}

#[derive(Clone)]
pub struct NsCssValue {
    pub(crate) u: Payload,
    pub(crate) image_set_text: Option<CString>,
    pub(crate) specified: Option<CString>,
    pub(crate) next_layer: Option<OwnedValue>,
}

impl RawCalc {
    pub(crate) fn to_calc(self) -> Calc {
        let mut args = [CalcArg::default(); 4];
        for (slot, arg) in args.iter_mut().zip(self.args) {
            *slot = CalcArg {
                px: arg.px,
                pct: arg.pct,
            };
        }
        Calc {
            pct: self.pct,
            px: self.px,
            em: self.em,
            rem: self.rem,
            vw: self.vw,
            vh: self.vh,
            vmin: self.vmin,
            vmax: self.vmax,
            parsed_vw: self.parsed_vw,
            parsed_vh: self.parsed_vh,
            func: self.func,
            n_args: self.n_args,
            arg_none: self.arg_none,
            args,
        }
    }

    fn from_calc(calc: &Calc) -> RawCalc {
        let mut args = [RawCalcArg { px: 0.0, pct: 0.0 }; 4];
        for (slot, arg) in args.iter_mut().zip(calc.args) {
            *slot = RawCalcArg {
                px: arg.px,
                pct: arg.pct,
            };
        }
        RawCalc {
            pct: calc.pct,
            px: calc.px,
            em: calc.em,
            rem: calc.rem,
            vw: calc.vw,
            vh: calc.vh,
            vmin: calc.vmin,
            vmax: calc.vmax,
            parsed_vw: calc.parsed_vw,
            parsed_vh: calc.parsed_vh,
            func: calc.func,
            n_args: calc.n_args,
            arg_none: calc.arg_none,
            args,
        }
    }
}

pub(crate) fn c_dup(text: &[u8]) -> CString {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    CString::new(&text[..end]).unwrap_or_default()
}

pub(crate) fn areas_of(areas: &Areas) -> RawAreas {
    let mut raw = RawAreas {
        n_rows: areas.rows,
        n_cols: areas.cols,
        n_rects: areas.rects.len() as c_int,
        rects: core::array::from_fn(|_| RawAreaRect::default()),
    };
    for (slot, rect) in raw.rects.iter_mut().zip(&areas.rects) {
        *slot = RawAreaRect {
            name: Some(c_dup(&rect.name)),
            r0: rect.r0,
            r1: rect.r1,
            c0: rect.c0,
            c1: rect.c1,
        };
    }
    raw
}

impl Payload {
    pub(crate) fn of_parsed(parsed: &Parsed) -> Payload {
        match parsed {
            Parsed::Length(v, unit) => Payload::Length(Length { v: *v, unit: *unit }),
            Parsed::Calc(calc) => Payload::Calc(RawCalc::from_calc(calc)),
        }
    }

    pub(crate) fn keyword(text: &[u8]) -> Payload {
        Payload::Keyword(Some(c_dup(text)))
    }

    fn kind(&self) -> c_uint {
        match self {
            Payload::Keyword(_) => KIND_KEYWORD,
            Payload::Length(_) => KIND_LENGTH,
            Payload::Size(_) => KIND_SIZE,
            Payload::Color(_) => KIND_COLOR,
            Payload::Calc(_) => KIND_CALC,
            Payload::Shadow(_) => KIND_SHADOW,
            Payload::Gradient(_) => KIND_GRADIENT,
            Payload::Tracks(_) => KIND_TRACKS,
            Payload::Url(_) => KIND_URL,
            Payload::Transform(_) => KIND_TRANSFORM,
            Payload::Areas(_) => KIND_AREAS,
            Payload::Anim(_) => KIND_ANIM,
            Payload::Rect(_) => KIND_RECT,
        }
    }
}

pub(crate) fn new_keyword(text: &[u8]) -> OwnedValue {
    OwnedValue::of(Payload::keyword(text))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    Keyword,
    Length,
    Size,
    Color,
    Calc,
    Shadow,
    Gradient,
    Url,
    Transform,
    Rect,
    Other,
}

impl NsCssValue {
    pub(crate) fn of(u: Payload) -> NsCssValue {
        NsCssValue {
            u,
            image_set_text: None,
            specified: None,
            next_layer: None,
        }
    }

    pub fn kind(&self) -> c_uint {
        self.u.kind()
    }

    pub fn value_kind(&self) -> ValueKind {
        match self.u {
            Payload::Keyword(_) => ValueKind::Keyword,
            Payload::Length(_) => ValueKind::Length,
            Payload::Size(_) => ValueKind::Size,
            Payload::Color(_) => ValueKind::Color,
            Payload::Calc(_) => ValueKind::Calc,
            Payload::Shadow(_) => ValueKind::Shadow,
            Payload::Gradient(_) => ValueKind::Gradient,
            Payload::Url(_) => ValueKind::Url,
            Payload::Transform(_) => ValueKind::Transform,
            Payload::Rect(_) => ValueKind::Rect,
            Payload::Tracks(_) | Payload::Areas(_) | Payload::Anim(_) => ValueKind::Other,
        }
    }

    pub fn keyword(&self) -> Option<&CStr> {
        match &self.u {
            Payload::Keyword(text) => text.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn keyword_bytes(&self) -> Option<&[u8]> {
        self.keyword().map(CStr::to_bytes)
    }

    pub fn length(&self) -> Option<(f64, c_uint)> {
        match self.u {
            Payload::Length(v) => Some((v.v, v.unit)),
            _ => None,
        }
    }

    pub(crate) fn raw_color(&self) -> Option<RawColor> {
        match self.u {
            Payload::Color(v) => Some(v),
            _ => None,
        }
    }

    pub fn color_rgba(&self) -> Option<[u8; 4]> {
        self.raw_color().map(|v| [v.r, v.g, v.b, v.a])
    }

    pub fn size_parts(&self) -> Option<([f64; 2], [c_uint; 2], [bool; 2])> {
        match self.u {
            Payload::Size(v) => Some((
                [v.w, v.h],
                [v.w_unit, v.h_unit],
                [v.w_auto != 0, v.h_auto != 0],
            )),
            _ => None,
        }
    }

    pub fn calc(&self) -> Option<Calc> {
        match self.u {
            Payload::Calc(v) => Some(v.to_calc()),
            _ => None,
        }
    }

    pub fn url(&self) -> Option<&CStr> {
        match &self.u {
            Payload::Url(url) => url.as_deref(),
            _ => None,
        }
    }

    pub fn rect_parts(&self) -> Option<([f64; 4], [c_uint; 4], [bool; 4])> {
        match self.u {
            Payload::Rect(v) => Some((v.v, v.unit, v.is_auto.map(|a| a != 0))),
            _ => None,
        }
    }

    pub fn gradient(&self) -> Option<&Gradient> {
        match &self.u {
            Payload::Gradient(v) => Some(v),
            _ => None,
        }
    }

    pub fn shadows(&self) -> Option<&ShadowList> {
        match &self.u {
            Payload::Shadow(v) => Some(v),
            _ => None,
        }
    }

    pub fn tracks(&self) -> Option<&Tracks> {
        match &self.u {
            Payload::Tracks(v) => Some(v),
            _ => None,
        }
    }

    pub fn areas(&self) -> Option<&RawAreas> {
        match &self.u {
            Payload::Areas(v) => Some(v),
            _ => None,
        }
    }

    pub fn transform(&self) -> Option<&Transform> {
        match &self.u {
            Payload::Transform(v) => Some(v),
            _ => None,
        }
    }

    pub fn next_layer(&self) -> Option<&NsCssValue> {
        self.next_layer.as_deref()
    }

    pub fn specified_text(&self) -> Option<&CStr> {
        self.specified.as_deref()
    }

    pub fn length_or(&self, fallback: f64) -> f64 {
        if let Some((v, unit)) = self.length()
            && (unit == crate::units::PX || unit == crate::units::NUMBER)
        {
            return v;
        }
        self.calc().map_or(fallback, |calc| calc.px)
    }
}

#[derive(Clone)]
pub struct OwnedValue(Rc<NsCssValue>);

impl OwnedValue {
    pub(crate) fn new(value: NsCssValue) -> OwnedValue {
        OwnedValue(Rc::new(value))
    }

    pub(crate) fn of(u: Payload) -> OwnedValue {
        OwnedValue::new(NsCssValue::of(u))
    }

    pub fn placeholder() -> OwnedValue {
        OwnedValue::of(Payload::Keyword(None))
    }

    pub fn as_ptr(&self) -> *const NsCssValue {
        Rc::as_ptr(&self.0)
    }

    pub fn ptr_eq(a: &OwnedValue, b: &OwnedValue) -> bool {
        Rc::ptr_eq(&a.0, &b.0)
    }

    pub(crate) fn make_mut(&mut self) -> &mut NsCssValue {
        Rc::make_mut(&mut self.0)
    }
}

impl Deref for OwnedValue {
    type Target = NsCssValue;

    fn deref(&self) -> &NsCssValue {
        &self.0
    }
}
