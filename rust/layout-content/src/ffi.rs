//! Southstar — the generated-content and form-control entry points layout.c calls, and the layout.c, css, glib and text-layout calls they make back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

use southstar_dom::{Node, NsNode};
use southstar_layout::{BoxRef, InlineAttr, Style};
use southstar_paint::text_layout::{self, Layout};
use southstar_style::StyleRef;

use crate::controls;

unsafe extern "C" {
    fn g_utf8_get_char(p: *const c_char) -> u32;
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

pub fn utf8_strup(s: &CStr) -> Vec<u8> {
    southstar_unicode::upper_string(&s.to_string_lossy()).into_bytes()
}

pub fn utf8_strdown(s: &CStr) -> Vec<u8> {
    southstar_unicode::lower_string(&s.to_string_lossy()).into_bytes()
}

pub fn utf8_casefold(s: &[u8]) -> Vec<u8> {
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    southstar_unicode::fold_string(&String::from_utf8_lossy(&s[..end])).into_bytes()
}

pub fn utf8_get_char(s: &CStr, at: usize) -> u32 {
    let bytes = s.to_bytes_with_nul();
    if at >= bytes.len() {
        return 0;
    }
    unsafe { g_utf8_get_char(bytes.as_ptr().add(at).cast()) }
}

pub fn unichar_isspace(c: u32) -> bool {
    southstar_unicode::is_space(c)
}

pub fn unichar_title_utf8(c: u32) -> Vec<u8> {
    char::from_u32(southstar_unicode::to_title(c))
        .map(|title| title.to_string().into_bytes())
        .unwrap_or_default()
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

pub fn inline_attr_css_extra_w(r: &InlineAttr, s: StyleRef<'_>) -> f64 {
    controls::css_extra(node(r.dom_ptr().cast()), Some(s), false)
}

pub fn inline_attr_control_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    controls::inline_attr_control_width(r, style(r.style()), node(r.dom_ptr().cast()), Some(b))
}
