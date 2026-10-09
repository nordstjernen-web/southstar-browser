//! Southstar — the C ABI of CSS font values, and the font oracle, generation and metrics callbacks the painter installs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint};
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};
use std::ffi::CString;
use std::sync::{Mutex, PoisonError};

use southstar_glib::{self as glib, GBoolean};

use super::value::{KIND_KEYWORD, KIND_LENGTH, NsCssValue};
use crate::font;
use crate::scan::split_ws_paren;
use crate::units::PERCENT;

type AvailableFn = unsafe extern "C" fn(family: *const c_char) -> GBoolean;
type GenerationFn = unsafe extern "C" fn() -> u64;
type MetricsFn = unsafe extern "C" fn(
    family: *const c_char,
    size_px: c_double,
    weight: c_int,
    italic: GBoolean,
    out: *mut FontMetrics,
);

#[repr(C)]
pub(crate) struct FontMetrics {
    ex_px: f64,
    ch_px: f64,
    cap_px: f64,
    ic_px: f64,
    line_px: f64,
    ascent_px: f64,
    descent_px: f64,
}

const _: () = assert!(core::mem::size_of::<FontMetrics>() == 56);

static AVAILABLE: Mutex<Option<AvailableFn>> = Mutex::new(None);
static GENERATION: Mutex<Option<GenerationFn>> = Mutex::new(None);
static METRICS: Mutex<Option<MetricsFn>> = Mutex::new(None);
static ORACLE_SERIAL: AtomicU32 = AtomicU32::new(0);

fn load<T: Copy>(slot: &Mutex<Option<T>>) -> Option<T> {
    *slot.lock().unwrap_or_else(PoisonError::into_inner)
}

fn store<T>(slot: &Mutex<Option<T>>, value: Option<T>) {
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = value;
}

pub(crate) fn font_available(family: &[u8]) -> Option<bool> {
    let cb = load(&AVAILABLE)?;
    let family = CString::new(family).unwrap_or_default();
    Some(unsafe { cb(family.as_ptr()) } != 0)
}

pub(crate) fn font_generation() -> u64 {
    load(&GENERATION).map_or(0, |cb| unsafe { cb() })
}

pub(crate) fn font_oracle_serial() -> u32 {
    ORACLE_SERIAL.load(Ordering::Relaxed)
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

unsafe fn keyword<'a>(v: *const NsCssValue) -> Option<&'a [u8]> {
    let v = unsafe { v.as_ref() }?;
    if v.kind != KIND_KEYWORD {
        return None;
    }
    unsafe { bytes(v.u.keyword) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_font_available_cb(cb: Option<AvailableFn>) {
    store(&AVAILABLE, cb);
    ORACLE_SERIAL.fetch_add(1, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_font_generation_cb(cb: Option<GenerationFn>) {
    store(&GENERATION, cb);
    ORACLE_SERIAL.fetch_add(1, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_font_metrics_cb(cb: Option<MetricsFn>) {
    store(&METRICS, cb);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_relative_unit_px(
    unit: c_uint,
    font_px: c_double,
    family: *const c_char,
    weight: c_int,
    italic: GBoolean,
) -> c_double {
    let mut m = FontMetrics {
        ex_px: font_px * 0.5,
        ch_px: font_px * 0.5,
        cap_px: font_px * 0.7,
        ic_px: font_px,
        line_px: 0.0,
        ascent_px: 0.0,
        descent_px: 0.0,
    };
    if let Some(cb) = load(&METRICS).filter(|_| font_px > 0.0) {
        unsafe { cb(family, font_px, weight, italic, &mut m) };
    }
    font::relative_unit_px(unit, &[m.ex_px, m.ch_px, m.cap_px, m.ic_px], font_px)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_family_for_pango(css_family: *const c_char) -> *mut c_char {
    glib::strdup(&font::family_for_pango(
        unsafe { bytes(css_family) }.unwrap_or_default(),
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_font_weight_relative(parent: c_int, bolder: GBoolean) -> c_int {
    font::weight_relative(parent, bolder != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_weight_number(v: *const NsCssValue, fallback: c_int) -> c_int {
    font::weight_number(unsafe { keyword(v) }, fallback)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_stretch_rank(v: *const NsCssValue) -> c_int {
    let percent = unsafe { v.as_ref() }
        .filter(|v| v.kind == KIND_LENGTH && unsafe { v.u.length.unit } == PERCENT)
        .map(|v| unsafe { v.u.length.v });
    font::stretch_rank(unsafe { keyword(v) }, percent)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_split_ws_paren(
    text: *const c_char,
    out: *mut *mut c_char,
    max: c_int,
) -> c_int {
    let Some(text) = (unsafe { bytes(text) }) else {
        return 0;
    };
    let tokens = split_ws_paren(text, usize::try_from(max).unwrap_or(0));
    for (i, tok) in tokens.iter().enumerate() {
        unsafe { *out.add(i) = glib::strdup(tok) };
    }
    tokens.len() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_shorthand_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(font::shorthand_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_family_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(font::family_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_ligatures_valid(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(font::ligatures_valid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_feature_settings_valid(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(font::feature_settings_valid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_font_variation_settings_valid(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(font::variation_settings_valid))
}
