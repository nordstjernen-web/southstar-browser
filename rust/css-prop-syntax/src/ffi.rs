//! Southstar — the C ABI of registered custom property syntax, as declared in src/css_prop_syntax.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use southstar_glib::{self as glib, GBoolean};

use crate::{Context, SyntaxDef};

#[repr(C)]
pub struct SyntaxCtx {
    font_size: f64,
    root_font_size: f64,
    line_height: f64,
    root_line_height: f64,
    ex_px: f64,
    ch_px: f64,
    cap_px: f64,
    ic_px: f64,
    root_ex_px: f64,
    root_ch_px: f64,
    root_cap_px: f64,
    root_ic_px: f64,
    viewport_w: f64,
    viewport_h: f64,
    container_w: f64,
    container_h: f64,
    current_color: *const c_char,
}

const DTOSTR_BUF_SIZE: usize = 29 + 10;

unsafe extern "C" {
    fn g_ascii_formatd(
        buffer: *mut c_char,
        buf_len: c_int,
        format: *const c_char,
        d: f64,
    ) -> *mut c_char;
    fn ns_css_parse_color(
        s: *const c_char,
        r: *mut u8,
        g: *mut u8,
        b: *mut u8,
        a: *mut u8,
    ) -> GBoolean;
}

pub(crate) fn format_fixed6(n: f64) -> Vec<u8> {
    let mut buf = [0 as c_char; DTOSTR_BUF_SIZE];
    unsafe {
        g_ascii_formatd(
            buf.as_mut_ptr(),
            DTOSTR_BUF_SIZE as c_int,
            c"%.6f".as_ptr(),
            n,
        );
        CStr::from_ptr(buf.as_ptr()).to_bytes().to_vec()
    }
}

pub(crate) fn parse_color(text: &[u8]) -> Option<[u8; 4]> {
    let text = std::ffi::CString::new(text).ok()?;
    let mut rgba = [0u8, 0, 0, 255];
    let [r, g, b, a] = &mut rgba;
    (unsafe { ns_css_parse_color(text.as_ptr(), r, g, b, a) } != 0).then_some(rgba)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_parse(text: *const c_char) -> *mut SyntaxDef {
    unsafe { glib::bytes(text) }
        .and_then(crate::parse)
        .map_or(ptr::null_mut(), |syntax| Box::into_raw(Box::new(syntax)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_free(syntax: *mut SyntaxDef) {
    if !syntax.is_null() {
        drop(unsafe { Box::from_raw(syntax) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_universal(syntax: *const SyntaxDef) -> GBoolean {
    glib::boolean(unsafe { syntax.as_ref() }.is_some_and(SyntaxDef::universal))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_matches(
    syntax: *const SyntaxDef,
    value: *const c_char,
) -> GBoolean {
    let (syntax, value) = unsafe { (syntax.as_ref(), glib::bytes(value)) };
    glib::boolean(
        matches!((syntax, value), (Some(syntax), Some(value)) if crate::matches(syntax, value)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_initial_valid(
    syntax: *const SyntaxDef,
    value: *const c_char,
) -> GBoolean {
    let (syntax, value) = unsafe { (syntax.as_ref(), glib::bytes(value)) };
    glib::boolean(
        matches!((syntax, value), (Some(syntax), Some(value)) if crate::initial_valid(syntax, value)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_def_compute(
    syntax: *const SyntaxDef,
    value: *const c_char,
    ctx: *const SyntaxCtx,
) -> *mut c_char {
    let Some(syntax) = (unsafe { syntax.as_ref() }) else {
        return ptr::null_mut();
    };
    let Some(value) = (unsafe { glib::bytes(value) }) else {
        return ptr::null_mut();
    };
    let ctx = unsafe { ctx.as_ref() }.map(|c| Context {
        font_size: c.font_size,
        root_font_size: c.root_font_size,
        line_height: c.line_height,
        root_line_height: c.root_line_height,
        ex_px: c.ex_px,
        ch_px: c.ch_px,
        cap_px: c.cap_px,
        ic_px: c.ic_px,
        root_ex_px: c.root_ex_px,
        root_ch_px: c.root_ch_px,
        root_cap_px: c.root_cap_px,
        root_ic_px: c.root_ic_px,
        viewport_w: c.viewport_w,
        viewport_h: c.viewport_h,
        container_w: c.container_w,
        container_h: c.container_h,
        current_color: unsafe { glib::bytes(c.current_color) },
    });
    crate::compute(syntax, value, ctx.as_ref()).map_or(ptr::null_mut(), |out| glib::strdup(&out))
}
