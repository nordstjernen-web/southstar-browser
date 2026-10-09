//! Southstar — struct ns_css_value as css.h lays it out for the value kinds the Rust sections build, and building one css.c can own and free.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::mem::{offset_of, size_of};

use southstar_glib as glib;

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

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Length {
    pub v: f64,
    pub unit: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawCalcArg {
    pub px: f64,
    pub pct: f64,
}

#[repr(C)]
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

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawAreaRect {
    pub name: *mut c_char,
    pub r0: c_int,
    pub r1: c_int,
    pub c0: c_int,
    pub c1: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawAreas {
    pub n_rows: c_int,
    pub n_cols: c_int,
    pub n_rects: c_int,
    pub rects: [RawAreaRect; AREAS_MAX],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawSize {
    pub w: f64,
    pub h: f64,
    pub w_unit: c_uint,
    pub h_unit: c_uint,
    pub w_auto: c_int,
    pub h_auto: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawRect {
    pub v: [f64; 4],
    pub unit: [c_uint; 4],
    pub is_auto: [c_int; 4],
}

#[repr(C)]
pub(crate) union ValueUnion {
    pub length: Length,
    pub size: RawSize,
    pub color: RawColor,
    pub rect: RawRect,
    pub url: *mut c_char,
    pub calc: RawCalc,
    pub gradient: Gradient,
    pub transform: Transform,
    pub shadow: ShadowList,
    pub anim: RawList,
    pub tracks: Tracks,
    pub areas: RawAreas,
    pub keyword: *mut c_char,
    _storage: [u64; 381],
}

#[repr(C)]
pub struct NsCssValue {
    pub(crate) kind: c_uint,
    pub(crate) ref_count: c_int,
    pub(crate) u: ValueUnion,
    pub(crate) image_set_text: *mut c_char,
    pub(crate) specified: *mut c_char,
    pub(crate) next_layer: *mut NsCssValue,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<NsCssValue>() == 3080
        && offset_of!(NsCssValue, u) == 8
        && offset_of!(NsCssValue, next_layer) == 3072
        && offset_of!(RawCalc, func) == 80
        && offset_of!(RawCalc, args) == 88
        && size_of::<RawCalc>() == 152
        && size_of::<RawAreaRect>() == 24
        && size_of::<RawAreas>() == 784
        && size_of::<RawSize>() == 32
        && size_of::<RawRect>() == 64
);

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

pub(crate) fn new_value(parsed: &Parsed) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    match parsed {
        Parsed::Length(v, unit) => {
            value_ref.kind = KIND_LENGTH;
            value_ref.u.length = Length { v: *v, unit: *unit };
        }
        Parsed::Calc(calc) => {
            value_ref.kind = KIND_CALC;
            value_ref.u.calc = RawCalc::from_calc(calc);
        }
    }
    value
}

pub(crate) fn new_gradient(gradient: &Gradient) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_GRADIENT;
    value_ref.u.gradient = *gradient;
    value
}

pub(crate) fn new_transform(transform: &Transform) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_TRANSFORM;
    value_ref.u.transform = *transform;
    value
}

pub(crate) unsafe fn calc_of(value: *const NsCssValue) -> Option<Calc> {
    let value = unsafe { value.as_ref() }?;
    (value.kind == KIND_CALC).then(|| unsafe { value.u.calc }.to_calc())
}

pub(crate) unsafe fn length_of(value: *const NsCssValue) -> Option<Length> {
    let value = unsafe { value.as_ref() }?;
    if value.kind == KIND_LENGTH {
        Some(unsafe { value.u.length })
    } else {
        None
    }
}

pub(crate) fn new_tracks(tracks: &Tracks) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_TRACKS;
    value_ref.u.tracks = *tracks;
    value
}

pub(crate) fn new_areas(areas: &Areas) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_AREAS;
    let raw = unsafe { &mut value_ref.u.areas };
    raw.n_rows = areas.rows;
    raw.n_cols = areas.cols;
    raw.n_rects = areas.rects.len() as c_int;
    for (slot, rect) in raw.rects.iter_mut().zip(&areas.rects) {
        *slot = RawAreaRect {
            name: glib::strdup(&rect.name),
            r0: rect.r0,
            r1: rect.r1,
            c0: rect.c0,
            c1: rect.c1,
        };
    }
    value
}

pub(crate) fn new_shadow(list: &ShadowList) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_SHADOW;
    value_ref.u.shadow = *list;
    value
}

pub(crate) fn new_keyword(text: &[u8]) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_KEYWORD;
    value_ref.u.keyword = glib::strdup(text);
    value
}

pub(crate) fn new_anim(list: &RawList) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    let value_ref = unsafe { &mut *value };
    value_ref.kind = KIND_ANIM;
    value_ref.u.anim = *list;
    value
}

pub(crate) unsafe fn style_value<'a>(style: *const c_void, prop: c_int) -> Option<&'a NsCssValue> {
    let index = usize::try_from(prop).ok()?;
    let values = style.cast::<*const NsCssValue>();
    unsafe { (*values.add(index)).as_ref() }
}

pub(crate) unsafe fn keyword_of(v: Option<&NsCssValue>) -> Option<&[u8]> {
    let v = v?;
    if v.kind != KIND_KEYWORD || unsafe { v.u.keyword }.is_null() {
        return None;
    }
    Some(unsafe { core::ffi::CStr::from_ptr(v.u.keyword) }.to_bytes())
}

pub(crate) fn alloc(kind: c_uint) -> *mut NsCssValue {
    let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
    unsafe { (*value).kind = kind };
    value
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_value_dup(v: *const NsCssValue) -> *mut NsCssValue {
    let v = v.cast_mut();
    if let Some(value) = unsafe { v.as_mut() } {
        value.ref_count += 1;
    }
    v
}

unsafe fn free_payload(value: &mut NsCssValue) {
    unsafe {
        match value.kind {
            KIND_KEYWORD => glib::g_free(value.u.keyword.cast()),
            KIND_URL => glib::g_free(value.u.url.cast()),
            KIND_AREAS => {
                let areas = &value.u.areas;
                let n = usize::try_from(areas.n_rects).unwrap_or(0).min(AREAS_MAX);
                for rect in &areas.rects[..n] {
                    glib::g_free(rect.name.cast());
                }
            }
            KIND_ANIM => value.u.anim.free_names(),
            _ => {}
        }
        glib::g_free(value.image_set_text.cast());
        glib::g_free(value.specified.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_value_free(mut v: *mut NsCssValue) {
    while let Some(value) = unsafe { v.as_mut() } {
        if value.ref_count > 0 {
            value.ref_count -= 1;
            return;
        }
        unsafe { free_payload(value) };
        let next = value.next_layer;
        unsafe { glib::g_free(v.cast()) };
        v = next;
    }
}
