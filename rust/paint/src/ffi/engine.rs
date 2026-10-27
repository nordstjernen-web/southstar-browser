//! Southstar — the engine calls painting makes into the style engine, fonts, textures and the C that remains, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_anim::AnimatedColor;
use southstar_cairo::{ContextRef, Surface};
use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, InlineAttr};
use southstar_style::{StyleRef, Transform, ValueRef};
use southstar_texture::TextureRef;
use southstar_video::BoxVideo;

use crate::links;
use crate::text_layout::{AttrList, Layout};
use crate::util::Rgba;

pub use southstar_css::FontMetrics;

pub fn font_family_for_pango(family: Option<&CStr>) -> CString {
    southstar_style::font_family_for_pango(family.unwrap_or_default())
}

pub fn font_weight_number(v: Option<ValueRef<'_>>, fallback: i32) -> i32 {
    v.map_or(fallback, |v| v.font_weight_or(fallback))
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> i32 {
    southstar_style::font_stretch_rank(v)
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

pub fn container_w() -> f64 {
    southstar_css::container_unit_dims().0
}

pub fn container_h() -> f64 {
    southstar_css::container_unit_dims().1
}

pub fn node_dir_is_rtl(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| southstar_css::node_dir(n) == c"rtl")
}

pub fn css_unescape(text: &[u8]) -> Vec<u8> {
    let text = text.split(|&c| c == 0).next().unwrap_or_default();
    let mut out = Vec::with_capacity(text.len());
    let mut pos = 0;
    while pos < text.len() {
        southstar_css::append_unescaped(&mut out, text, &mut pos);
    }
    out
}

pub fn font_family_loaded(family: &CStr) -> bool {
    southstar_font::family_loaded(family.to_bytes())
}

pub fn font_generation() -> u32 {
    southstar_font::generation()
}

pub fn parse_int(text: &CStr, dflt: i32, min: i32, max: i32) -> i32 {
    southstar_dom::controls::parse_int(Some(text), dflt, min, max)
}

pub fn format_g8(value: f64) -> Vec<u8> {
    southstar_numfmt::general(value, 8).into_bytes()
}

#[derive(Clone)]
pub struct Texture(TextureRef);

impl Texture {
    pub fn same(&self, other: &Texture) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub fn width(&self) -> i32 {
        self.0.width()
    }

    pub fn height(&self) -> i32 {
        self.0.height()
    }

    pub fn download(&self, dst: &mut [u8], stride: usize) {
        self.0.download(dst, stride);
    }

    pub fn with_cache<T: core::any::Any + Send, R>(
        &self,
        init: impl FnOnce() -> T,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        self.0.with_cache(init, f)
    }
}

pub fn image_texture(image: ImageRef<'_>) -> Option<Texture> {
    image.texture().map(Texture)
}

pub fn style_ptr(s: Option<StyleRef<'_>>) -> *const c_void {
    s.map_or(ptr::null(), |s| s.as_ptr().cast())
}

#[derive(Clone, Copy)]
pub struct SelectionRun {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy)]
pub struct Video(BoxVideo);

impl Video {
    pub fn of(b: BoxRef<'_>) -> Option<Video> {
        BoxVideo::of(b).map(Video)
    }

    pub fn playing(self) -> bool {
        self.0.playing()
    }

    pub fn video_opened(self) -> bool {
        self.0.video_opened()
    }

    pub fn frame_texture(self) -> Option<Texture> {
        self.0.frame_texture().map(Texture)
    }

    pub fn poster_texture(self) -> Option<Texture> {
        self.0.poster_texture().map(Texture)
    }
}

pub fn video_helper_composited(b: BoxRef<'_>) -> bool {
    BoxVideo::of(b).is_some_and(BoxVideo::helper_composited)
}

pub fn video_active_cue_text(b: BoxRef<'_>) -> Option<Vec<u8>> {
    BoxVideo::of(b)?.active_cue_text()
}

pub fn anim_color(b: BoxRef<'_>, which: AnimatedColor) -> Option<[u8; 4]> {
    crate::state::anim()?.color(b.dom()?, which)
}

pub fn border_image_source(s: StyleRef<'_>) -> Option<ValueRef<'_>> {
    southstar_style::border_image_source(Some(s))
}

pub fn fieldset_legend_gap(b: BoxRef<'_>) -> Option<(f64, f64, f64, f64, f64)> {
    let gap = (links::get()?.fieldset_legend_gap)(b)?;
    Some((gap.inset, gap.x0, gap.x1, gap.y0, gap.y1))
}

pub fn inline_apply_atomic_shapes(attrs: &AttrList, b: BoxRef<'_>) {
    if let Some(links) = links::get() {
        (links.inline_apply_atomic_shapes)(attrs, b);
    }
}

pub fn inline_layout_set_attrs(layout: &Layout, attrs: &AttrList, b: BoxRef<'_>) {
    if let Some(links) = links::get() {
        (links.inline_layout_set_attrs)(layout, attrs, b);
    }
}

pub fn control_css_extra_w(r: &InlineAttr, s: StyleRef<'_>) -> f64 {
    links::get().map_or(0.0, |links| (links.control_css_extra_w)(r, s))
}

pub fn vertical_stack_text(text: &CStr) -> CString {
    southstar_layout_geom::vertical_stack_text(text.to_bytes())
}

pub fn spell_available() -> bool {
    southstar_spellcheck::available()
}

pub fn spell_word_ok(word: &[u8]) -> bool {
    southstar_spellcheck::word_ok(word, None)
}

pub fn node_spellcheck_host(n: Node<'_>) -> bool {
    southstar_dom::controls::spellcheck_host(n).is_some()
}

pub fn js_image_for_node(b: BoxRef<'_>) -> Option<ImageRef<'_>> {
    let js = crate::state::js()?;
    southstar_js_images::image_for_node(Some(js), b.dom()?)
}

pub fn svg_render_node(cr: ContextRef, b: BoxRef<'_>, w: f64, h: f64) {
    let Some(svg) = b.dom() else {
        return;
    };
    // SAFETY: a box's SVG style table is null or a live table its layout owns.
    let styles = unsafe { southstar_style::StyleTable::from_ptr(b.svg_styles()) };
    southstar_svg::render_node(cr.into(), svg, w, h, styles, southstar_style::box_style(b));
}

pub fn math_paint(cr: ContextRef, b: BoxRef<'_>, x: f64, y: f64, font_px: f64, c: Rgba) {
    if let Some((links, math)) = links::get().zip(b.dom()) {
        (links.math_paint)(cr, math, x, y, font_px, [c.r, c.g, c.b, c.a]);
    }
}

pub fn anim_opacity(b: BoxRef<'_>) -> Option<f64> {
    crate::state::anim()?.opacity(b.dom()?)
}

pub fn effective_transform(b: BoxRef<'_>, s: Option<StyleRef<'_>>) -> Transform {
    let anim_tf = crate::state::anim()
        .zip(b.dom())
        .and_then(|(anim, dom)| anim.transform(dom));
    southstar_style::effective_transform(s, anim_tf.as_ref())
}

pub fn box_is_fixed(b: BoxRef<'_>) -> bool {
    southstar_layout_geom::box_is_fixed(b)
}

pub fn box_in_scroller(b: BoxRef<'_>) -> bool {
    links::get().is_some_and(|links| (links.in_scroller)(b))
}

pub fn box_sticky_offset(b: BoxRef<'_>, clip: (f64, f64, f64, f64)) -> (f64, f64) {
    links::get().map_or((0.0, 0.0), |links| {
        (links.sticky_offset)(b, clip.0, clip.1, clip.2, clip.3)
    })
}

pub fn active_modal<'a>() -> Option<Node<'a>> {
    southstar_dom::tree::active_modal()
}

pub fn canvas_surface(b: BoxRef<'_>) -> Option<Surface> {
    let js = crate::state::js()?;
    southstar_canvas::canvas_surface_for(js, b.dom()?)
}

pub fn unichar_isalpha(c: u32) -> bool {
    southstar_unicode::is_alpha(c)
}

pub fn utf8_get_char(text: &CStr, pos: usize) -> u32 {
    southstar_layout::utf8_get_char(text.to_bytes(), pos)
}

fn utf8_skip(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

pub fn utf8_next(text: &[u8], pos: usize) -> usize {
    pos + text.get(pos).map_or(1, |&c| utf8_skip(c))
}

pub fn utf8_offset_to_byte(text: &[u8], start: usize, chars: i32) -> usize {
    let mut p = start;
    for _ in 0..chars.max(0) {
        p = utf8_next(text, p);
    }
    p
}

pub fn utf8_pointer_to_offset(text: &[u8], byte: usize) -> usize {
    let mut p = 0;
    let mut n = 0;
    while p < byte {
        p = utf8_next(text, p);
        n += 1;
    }
    n
}
