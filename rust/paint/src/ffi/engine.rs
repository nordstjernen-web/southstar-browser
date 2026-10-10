//! Southstar — the engine calls painting makes into the style engine, fonts, textures and the C that remains, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GStr};
use southstar_style::{StyleRef, ValueRef};

use super::cairo::SurfaceRef;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FontMetrics {
    pub ex_px: c_double,
    pub ch_px: c_double,
    pub cap_px: c_double,
    pub ic_px: c_double,
    pub line_px: c_double,
    pub ascent_px: c_double,
    pub descent_px: c_double,
}

#[repr(C)]
struct GString {
    text: *mut c_char,
    len: usize,
    allocated_len: usize,
}

pub type FontAvailableFn = unsafe extern "C" fn(family: *const c_char) -> GBoolean;
pub type FontGenerationFn = unsafe extern "C" fn() -> u64;
pub type FontMetricsFn = unsafe extern "C" fn(
    family: *const c_char,
    size_px: c_double,
    weight: c_int,
    italic: GBoolean,
    out: *mut FontMetrics,
);

unsafe extern "C" {
    fn ns_css_font_family_for_pango(css_family: *const c_char) -> *mut c_char;
    fn ns_css_font_weight_number(v: *const c_void, fallback: c_int) -> c_int;
    fn ns_css_font_stretch_rank(v: *const c_void) -> c_int;
    fn ns_css_viewport_w() -> c_double;
    fn ns_css_viewport_h() -> c_double;
    fn ns_css_container_w() -> c_double;
    fn ns_css_container_h() -> c_double;
    fn ns_css_node_dir(el: *const NsNode) -> *const c_char;
    fn ns_css_append_unescaped(out: *mut GString, pp: *mut *const c_char);
    fn ns_css_set_font_available_cb(cb: Option<FontAvailableFn>);
    fn ns_css_set_font_generation_cb(cb: Option<FontGenerationFn>);
    fn ns_css_set_font_metrics_cb(cb: Option<FontMetricsFn>);
    fn ns_font_family_loaded(family: *const c_char) -> GBoolean;
    fn ns_font_generation() -> c_uint;
    fn ns_parse_int(s: *const c_char, dflt: c_int, min_v: c_int, max_v: c_int) -> c_int;
    fn ns_texture_get_width(texture: *mut c_void) -> c_int;
    fn ns_texture_get_height(texture: *mut c_void) -> c_int;
    fn ns_paint_texture_surface_cached(texture: *mut c_void, filter: *const c_char) -> *mut c_void;
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(s: *mut GString, free_segment: GBoolean) -> *mut c_char;
    fn g_ascii_formatd(
        buffer: *mut c_char,
        len: c_int,
        format: *const c_char,
        d: c_double,
    ) -> *mut c_char;
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const c_void {
    v.map_or(ptr::null(), |v| v.as_ptr().cast())
}

pub fn font_family_for_pango(family: Option<&CStr>) -> GStr {
    let raw = unsafe { ns_css_font_family_for_pango(family.map_or(ptr::null(), CStr::as_ptr)) };
    unsafe { GStr::take(raw) }.unwrap_or_else(|| {
        let empty = southstar_glib::strdup(b"");
        unsafe { GStr::take(empty) }.expect("g_malloc never returns NULL")
    })
}

pub fn font_weight_number(v: Option<ValueRef<'_>>, fallback: i32) -> i32 {
    unsafe { ns_css_font_weight_number(value_ptr(v), fallback) }
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> i32 {
    unsafe { ns_css_font_stretch_rank(value_ptr(v)) }
}

pub fn viewport_w() -> f64 {
    unsafe { ns_css_viewport_w() }
}

pub fn viewport_h() -> f64 {
    unsafe { ns_css_viewport_h() }
}

pub fn container_w() -> f64 {
    unsafe { ns_css_container_w() }
}

pub fn container_h() -> f64 {
    unsafe { ns_css_container_h() }
}

pub fn node_dir_is_rtl(node: Option<Node<'_>>) -> bool {
    let dir = unsafe { ns_css_node_dir(Node::ptr_or_null(node)) };
    !dir.is_null() && unsafe { CStr::from_ptr(dir) } == c"rtl"
}

pub fn css_unescape(text: &[u8]) -> Vec<u8> {
    let raw = CString::new(text.split(|&c| c == 0).next().unwrap_or_default()).unwrap_or_default();
    let out = unsafe { g_string_new(ptr::null()) };
    let mut at = raw.as_ptr();
    while unsafe { *at } != 0 {
        unsafe { ns_css_append_unescaped(out, &mut at) };
    }
    let bytes = unsafe {
        let s = &*out;
        southstar_glib::slice(s.text.cast(), s.len).to_vec()
    };
    unsafe { g_string_free(out, 1) };
    bytes
}

pub fn set_font_oracle(
    available: FontAvailableFn,
    generation: FontGenerationFn,
    metrics: FontMetricsFn,
) {
    unsafe {
        ns_css_set_font_available_cb(Some(available));
        ns_css_set_font_generation_cb(Some(generation));
        ns_css_set_font_metrics_cb(Some(metrics));
    }
}

pub fn font_family_loaded(family: &CStr) -> bool {
    unsafe { ns_font_family_loaded(family.as_ptr()) != 0 }
}

pub fn font_generation() -> u32 {
    unsafe { ns_font_generation() }
}

pub fn parse_int(text: &CStr, dflt: i32, min: i32, max: i32) -> i32 {
    unsafe { ns_parse_int(text.as_ptr(), dflt, min, max) }
}

pub fn format_g8(value: f64) -> Vec<u8> {
    let mut buf = [0 as c_char; 32];
    unsafe {
        g_ascii_formatd(buf.as_mut_ptr(), 32, c"%.8g".as_ptr(), value);
        CStr::from_ptr(buf.as_ptr()).to_bytes().to_vec()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Texture(*mut c_void);

impl Texture {
    pub unsafe fn from_raw(texture: *mut c_void) -> Option<Texture> {
        (!texture.is_null()).then_some(Texture(texture))
    }

    pub fn raw(self) -> *mut c_void {
        self.0
    }

    pub fn width(self) -> i32 {
        unsafe { ns_texture_get_width(self.0) }
    }

    pub fn height(self) -> i32 {
        unsafe { ns_texture_get_height(self.0) }
    }

    pub fn surface(self, filter: Option<&CStr>) -> Option<SurfaceRef> {
        let raw = unsafe {
            ns_paint_texture_surface_cached(self.0, filter.map_or(ptr::null(), CStr::as_ptr))
        };
        unsafe { SurfaceRef::from_raw(raw) }
    }
}

pub fn style_ptr(s: Option<StyleRef<'_>>) -> *const c_void {
    s.map_or(ptr::null(), |s| s.as_ptr().cast())
}
