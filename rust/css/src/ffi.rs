//! Southstar — the C ABI of the ported css.c sections, and the GLib and css.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod attr_fn;
mod border_image;
mod cascade;
mod computed_units;
mod container;
mod custom_props;
mod declarations;
mod display;
mod element_state;
mod font;
mod gather;
mod grid;
mod hints;
mod host_scope;
mod image;
mod inline;
mod layers;
mod lex;
mod matcher;
mod nesting;
mod pending;
mod property;
mod restyle;
mod rule_index;
mod selector;
mod selector_view;
mod sheet;
mod shorthand;
mod style_query;
mod supports;
mod transform;
mod ua;
mod value;
mod values;
mod vars;

use core::ffi::{CStr, c_char, c_double, c_int, c_uint};

use southstar_glib::{self as glib, GBoolean};

use crate::{calc, color, math, units};

pub(crate) use computed_units::{ComputedStyle, PROP_COUNT, Slot, SlotMut, StyleView};
pub(crate) use container::{Container, container_map};
pub(crate) use declarations::{media_query_matches, syntax_def_valid};
pub(crate) use element_state::{
    regex_matches_whole, unichar_is_alpha, unichar_is_rtl_script, url_is_valid_absolute,
    url_resolve,
};
pub(crate) use font::relative_unit_px as font_relative_px;
pub(crate) use font::{font_available, font_generation, font_oracle_serial};
pub(crate) use hints::image_supports_mime;
pub(crate) use inline::{SheetDecl, first_rule_declares, sheet_declarations};
pub(crate) use matcher::{
    active_node, focus_node, focus_visible_node, fullscreen_node, hover_node, match_scope,
    set_match_scope,
};
pub(crate) use selector_view::{
    AttrRef, CompoundRef, GroupRef, PseudoRef, RuleRef, ScopeRef, SelectorRef, SheetRef,
};
pub(crate) use sheet::{PageRule, SheetBuilder, SyntaxDef};
pub(crate) use shorthand::prop_named;
pub use value::NsCssValue;
pub(crate) use value::RawCalc;

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

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_viewport_resolve(v: c_double, unit: c_uint) -> c_double {
    units::viewport_resolve(v, unit)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_number_str(n: c_double) -> *mut c_char {
    glib::strdup(&units::number_text(n))
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
