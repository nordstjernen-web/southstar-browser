//! Southstar — the C ABI of the ported css.c sections, and the GLib and css.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod container;
mod display;
mod font;
mod grid;
mod image;
mod lex;
mod shadow;
mod transform;
mod value;

use core::ffi::{CStr, c_char, c_double, c_int, c_uint};

use southstar_glib::{self as glib, GBoolean};

use crate::{calc, color, math, units};

pub(crate) use container::{Container, container_map};
pub(crate) use font::{font_available, font_generation, font_oracle_serial};
pub use value::NsCssValue;

const COLOR_SCHEME_DARK: c_int = 1;

unsafe extern "C" {
    fn ns_css_get_color_scheme() -> c_int;
    fn ns_css_viewport_w() -> c_double;
    fn ns_css_viewport_h() -> c_double;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
    fn ns_parse_int(s: *const c_char, dflt: c_int, min_v: c_int, max_v: c_int) -> c_int;
    fn g_ascii_formatd(
        buffer: *mut c_char,
        buf_len: c_int,
        format: *const c_char,
        d: c_double,
    ) -> *mut c_char;
}

pub(crate) fn prefers_dark() -> bool {
    unsafe { ns_css_get_color_scheme() == COLOR_SCHEME_DARK }
}

pub(crate) fn viewport() -> (f64, f64) {
    unsafe { (ns_css_viewport_w(), ns_css_viewport_h()) }
}

pub(crate) fn parse_int(text: &[u8], default: i32, min: i32, max: i32) -> i32 {
    let text = std::ffi::CString::new(text).unwrap_or_default();
    unsafe { ns_parse_int(text.as_ptr(), default, min, max) }
}

pub(crate) fn strtod(text: &CStr, pos: usize) -> (f64, usize) {
    glib::ascii_strtod_at(text, pos)
}

pub(crate) fn dtostr(value: f64) -> Vec<u8> {
    glib::ascii_dtostr(value)
}

pub(crate) fn format_double(format: &'static CStr, value: f64) -> Vec<u8> {
    let text = unsafe { glib::GStr::take(g_strdup_printf(format.as_ptr(), value)) };
    text.map(|text| text.to_bytes().to_vec())
        .unwrap_or_default()
}

pub(crate) fn formatd(format: &'static CStr, value: f64) -> Vec<u8> {
    let mut buffer = [0 as c_char; 64];
    unsafe {
        g_ascii_formatd(
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            format.as_ptr(),
            value,
        );
        CStr::from_ptr(buffer.as_ptr()).to_bytes().to_vec()
    }
}

pub(crate) fn format_g6(value: f64) -> Vec<u8> {
    format_double(c"%.6g", value)
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_color(
    s: *const c_char,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
    a: *mut u8,
) -> GBoolean {
    let mut channels: color::Channels = [None; 4];
    let ok = match unsafe { text(s) } {
        Some(s) => color::parse_into(s, &mut channels),
        None => {
            channels[3] = Some(255);
            false
        }
    };
    for (slot, value) in [r, g, b, a].into_iter().zip(channels) {
        if let (false, Some(value)) = (slot.is_null(), value) {
            unsafe { *slot = value };
        }
    }
    glib::boolean(ok)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_length(
    s: *const c_char,
    out_v: *mut c_double,
    out_unit: *mut c_uint,
) -> GBoolean {
    let Some((v, unit)) = unsafe { text(s) }.and_then(units::parse_length) else {
        return glib::FALSE;
    };
    unsafe {
        *out_v = v;
        *out_unit = unit;
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_calc(s: *const c_char) -> *mut NsCssValue {
    match unsafe { text(s) }.and_then(calc::parse_calc) {
        Some(parsed) => value::new_value(&parsed),
        None => core::ptr::null_mut(),
    }
}

unsafe fn resolve_into(
    s: *const c_char,
    len: usize,
    font: bool,
    out_px: *mut c_double,
    out_pct: *mut c_double,
) -> (bool, calc::Resolved) {
    let bytes = if s.is_null() {
        &[][..]
    } else {
        let all = unsafe { glib::slice(s.cast(), len) };
        let nul = all.iter().position(|&c| c == 0).unwrap_or(all.len());
        &all[..nul]
    };
    let (ok, resolved) = calc::resolve_to_px_pct(bytes, font);
    unsafe {
        *out_px = resolved.px;
        *out_pct = resolved.pct;
    }
    (ok, resolved)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_resolve_to_px_pct(
    s: *const c_char,
    len: usize,
    out_px: *mut c_double,
    out_pct: *mut c_double,
) -> GBoolean {
    let (ok, _) = unsafe { resolve_into(s, len, false, out_px, out_pct) };
    glib::boolean(ok)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_resolve_to_px_pct_font(
    s: *const c_char,
    len: usize,
    out_px: *mut c_double,
    out_pct: *mut c_double,
    out_em: *mut c_double,
    out_rem: *mut c_double,
) -> GBoolean {
    let font = !out_em.is_null() && !out_rem.is_null();
    let (ok, resolved) = unsafe { resolve_into(s, len, font, out_px, out_pct) };
    unsafe {
        if let Some(em) = out_em.as_mut() {
            *em = resolved.em;
        }
        if let Some(rem) = out_rem.as_mut() {
            *rem = resolved.rem;
        }
    }
    glib::boolean(ok)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_viewport_resolve(v: c_double, unit: c_uint) -> c_double {
    units::viewport_resolve(v, unit)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_unit_suffix(unit: c_int) -> *const c_char {
    units::unit_suffix(unit as c_uint).as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_number_str(n: c_double) -> *mut c_char {
    glib::strdup(&units::number_text(n))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_value_has_relative_unit(s: *const c_char) -> GBoolean {
    let relative = unsafe { text(s) }.is_some_and(|s| units::has_relative_unit(s.to_bytes()));
    glib::boolean(relative)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_angle_expr_rewrite(
    s: *const c_char,
    to_radians: GBoolean,
) -> *mut c_char {
    let rewritten = unsafe { text(s) }
        .map(|s| units::angle_expr_rewrite(s, to_radians != 0))
        .unwrap_or_default();
    glib::strdup(&rewritten)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_math_canonical(s: *const c_char) -> *mut c_char {
    match unsafe { text(s) }.and_then(math::math_canonical) {
        Some(canonical) => glib::strdup(&canonical),
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_length_or(v: *const NsCssValue, fallback: c_double) -> c_double {
    if let Some(length) = unsafe { value::length_of(v) } {
        if length.unit == units::PX || length.unit == units::NUMBER {
            return length.v;
        }
    }
    unsafe { value::calc_of(v) }.map_or(fallback, |calc| calc.px)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_calc_is_math_fn(v: *const NsCssValue) -> GBoolean {
    let math_fn =
        unsafe { value::calc_of(v) }.is_some_and(|calc| calc.func != 0 && calc.n_args > 0);
    glib::boolean(math_fn)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_calc_math_fn_px(v: *const NsCssValue, basis: c_double) -> c_double {
    let Some(raw) = (unsafe { v.as_ref() }) else {
        return 0.0;
    };
    calc::math_fn_px(&unsafe { raw.u.calc }.to_calc(), basis)
}
