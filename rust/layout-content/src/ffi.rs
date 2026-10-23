//! Southstar — the generated-content and form-control entry points layout.c calls, and the layout.c, css, glib and text-layout calls they make back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, g_free};
use southstar_layout::{BoxRef, InlineAttr, Style};
use southstar_paint::text_layout::{self, Layout};
use southstar_style::StyleRef;

use crate::controls;

unsafe extern "C" {
    fn g_utf8_strup(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_strdown(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_casefold(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_totitle(c: u32) -> u32;
    fn g_unichar_isspace(c: u32) -> GBoolean;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
}

static IMAGE_PEEK: OnceLock<fn(&CStr) -> *mut c_void> = OnceLock::new();

pub fn install_image_peek(peek: fn(&CStr) -> *mut c_void) {
    let _ = IMAGE_PEEK.set(peek);
}

pub fn peek_image(src: &CStr) -> *mut c_void {
    let peek = IMAGE_PEEK
        .get()
        .expect("the image cache peek is installed before the first build");
    peek(src)
}

pub fn viewport() -> (f64, f64) {
    southstar_css::viewport()
}

pub fn container() -> (f64, f64) {
    southstar_css::container_unit_dims()
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
    let layout = Layout::new(southstar_paint::text_context());
    southstar_paint::apply_inline_font(&layout, style(s));
    let ctx = layout.context();
    let fd = layout
        .font_description()
        .unwrap_or_else(|| ctx.font_description());
    ctx.metrics(&fd).map_or(0.0, |fm| {
        f64::from(fm.approximate_char_width()) / text_layout::SCALE_F
    })
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_control_css_extra_w(dom: *const NsNode, s: *const Style) -> f64 {
    controls::css_extra(node(dom), style(s), false)
}

pub fn inline_attr_control_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    controls::inline_attr_control_width(r, style(r.style()), node(r.dom_ptr().cast()), Some(b))
}
