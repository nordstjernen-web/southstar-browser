//! Southstar — the generated-content and form-control entry points layout.c calls, and the layout.c, css, glib and pango calls they make back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean, g_free, strdup};
use southstar_layout::{BoxRef, Edges, InlineAttr, NsBox, Style};
use southstar_paint::ffi as paint;
use southstar_paint::ffi::pango::{Context, Layout};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::{controls, pseudo, tags};

unsafe extern "C" {
    fn g_utf8_strup(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_strdown(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_casefold(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_totitle(c: u32) -> u32;
    fn g_unichar_isspace(c: u32) -> GBoolean;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
    fn ns_layout_length_resolve(v: *const NsCssValue, basis: f64, fallback: f64) -> f64;
    fn ns_layout_edges_from_style(
        s: *const Style,
        basis: f64,
        margin: *mut Edges,
        padding: *mut Edges,
        border: *mut Edges,
    );
    fn ns_layout_peek_image(src: *const c_char) -> *mut c_void;
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_css_container_w() -> f64;
    fn ns_css_container_h() -> f64;
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(v.map_or(ptr::null(), ValueRef::as_ptr), basis, fallback) }
}

pub fn edges_of(s: StyleRef<'_>, basis: f64) -> (Edges, Edges, Edges) {
    let (mut m, mut p, mut b) = (Edges::default(), Edges::default(), Edges::default());
    unsafe { ns_layout_edges_from_style(s.as_ptr(), basis, &mut m, &mut p, &mut b) };
    (m, p, b)
}

pub fn peek_image(src: &CStr) -> *mut c_void {
    unsafe { ns_layout_peek_image(src.as_ptr()) }
}

pub fn viewport() -> (f64, f64) {
    unsafe { (ns_css_viewport_w(), ns_css_viewport_h()) }
}

pub fn container() -> (f64, f64) {
    unsafe { (ns_css_container_w(), ns_css_container_h()) }
}

fn take_gstr(p: *mut c_char) -> Vec<u8> {
    if p.is_null() {
        return Vec::new();
    }
    let out = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
    unsafe { g_free(p.cast()) };
    out
}

pub fn utf8_strup(s: &CStr) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_strup(s.as_ptr(), -1) })
}

pub fn utf8_strdown(s: &CStr) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_strdown(s.as_ptr(), -1) })
}

pub fn utf8_casefold(s: &[u8]) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_casefold(s.as_ptr().cast(), s.len() as isize) })
}

pub fn utf8_get_char(s: &CStr, at: usize) -> u32 {
    let bytes = s.to_bytes_with_nul();
    if at >= bytes.len() {
        return 0;
    }
    unsafe { g_utf8_get_char(bytes.as_ptr().add(at).cast()) }
}

pub fn unichar_isspace(c: u32) -> bool {
    unsafe { g_unichar_isspace(c) != 0 }
}

pub fn unichar_title_utf8(c: u32) -> Vec<u8> {
    let mut buf = [0u8; 8];
    let n = unsafe { g_unichar_to_utf8(g_unichar_totitle(c), buf.as_mut_ptr().cast()) };
    buf[..n.clamp(0, 8) as usize].to_vec()
}

pub fn char_cell_px(s: *const Style) -> f64 {
    let layout = Layout::new(unsafe { Context::from_raw(paint::ns_paint_text_context()) });
    unsafe { paint::ns_paint_apply_inline_font(layout.raw(), s) };
    let ctx = layout.context();
    let fd = layout
        .font_description()
        .unwrap_or_else(|| ctx.font_description());
    if fd.raw().is_null() {
        return 0.0;
    }
    ctx.metrics(fd).map_or(0.0, |fm| {
        f64::from(fm.approximate_char_width()) / f64::from(paint::pango::SCALE)
    })
}

pub fn box_dom<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn box_style<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

fn value<'a>(v: *const NsCssValue) -> Option<ValueRef<'a>> {
    unsafe { ValueRef::from_ptr(v) }
}

fn dup_or_null(bytes: Option<Vec<u8>>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), |b| strdup(&b))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_non_rendering(name: *const c_char) -> GBoolean {
    boolean(tags::is_non_rendering(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_control_css_extra_w(dom: *const NsNode, s: *const Style) -> f64 {
    controls::css_extra(node(dom), style(s), false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_control_css_extra_h(dom: *const NsNode, s: *const Style) -> f64 {
    controls::css_extra(node(dom), style(s), true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_attr_control_width(
    r: *const InlineAttr,
    b: *const NsBox,
) -> f64 {
    let Some(r) = (unsafe { r.as_ref() }) else {
        return 0.0;
    };
    controls::inline_attr_control_width(r, style(r.style()), node(r.dom_ptr().cast()), unsafe {
        BoxRef::from_ptr(b)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_pseudo_content_text(
    content: *const NsCssValue,
    host: *const NsNode,
) -> *mut c_char {
    dup_or_null(
        value(content)
            .and_then(ValueRef::keyword_text)
            .and_then(|raw| pseudo::resolve_content(raw.to_bytes(), node(host))),
    )
}
