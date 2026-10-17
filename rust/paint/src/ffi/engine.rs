//! Southstar — the engine calls painting makes into the style engine, fonts, textures and the C that remains, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_cairo::{ContextRef, Surface, SurfaceRef};
use southstar_dom::Node;
use southstar_glib::{GBoolean, GHashTable};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style};
use southstar_style::{StyleRef, Transform, ValueRef};

use super::pango::{AttrList, Layout};
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Texture(*mut c_void);

impl Texture {
    pub unsafe fn from_raw(texture: *mut c_void) -> Option<Texture> {
        (!texture.is_null()).then_some(Texture(texture))
    }

    pub fn raw(self) -> *mut c_void {
        self.0
    }

    fn get(&self) -> &southstar_texture::Texture {
        // SAFETY: from_raw's caller vouched for a live texture its owner keeps for this handle's use.
        unsafe { &*self.0.cast::<southstar_texture::Texture>() }
    }

    pub fn width(self) -> i32 {
        self.get().width()
    }

    pub fn height(self) -> i32 {
        self.get().height()
    }
}

pub fn style_ptr(s: Option<StyleRef<'_>>) -> *const c_void {
    s.map_or(ptr::null(), |s| s.as_ptr().cast())
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectionRun {
    pub start: usize,
    pub end: usize,
}

#[repr(C)]
struct NsVideo {
    url: *mut c_char,
    natural_width: c_int,
    natural_height: c_int,
    poster_texture: *mut c_void,
    poster_url: *mut c_char,
    frame_texture: *mut c_void,
    loaded: GBoolean,
    load_events_fired: GBoolean,
    failed: GBoolean,
    player: *mut c_void,
    dom_node: *const c_void,
    is_camera: GBoolean,
    autoplay: GBoolean,
    loop_: GBoolean,
    controls: GBoolean,
    muted: GBoolean,
    has_audio: GBoolean,
    audio_opened: GBoolean,
    audio_url: *mut c_char,
    audio_file: *mut c_char,
    audio_file_len: usize,
    audio_file_gen: c_uint,
    audio_timeline_start: f64,
    pending_audio_file: *mut c_char,
    pending_audio_file_len: usize,
    pending_audio_file_gen: c_uint,
    pending_audio_start: f64,
    video_file: *mut c_char,
    video_file_len: usize,
    video_file_gen: c_uint,
    pending_video_file: *mut c_char,
    pending_video_file_len: usize,
    pending_video_file_gen: c_uint,
    pending_video_start: f64,
    video_opened: GBoolean,
    rect: [f64; 4],
    clip: [f64; 4],
    rect_fit: c_int,
    last_paint_us: i64,
    rect_dirty: GBoolean,
    rect_sent_us: i64,
    sent_rect: [f64; 4],
    sent_clip: [f64; 4],
    sent_rect_fit: c_int,
    sent_rect_page: GBoolean,
    token: *mut c_char,
    playing: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::offset_of!(NsVideo, player) == 56
        && core::mem::offset_of!(NsVideo, video_opened) == 232
        && core::mem::offset_of!(NsVideo, playing) == 416
);

#[derive(Clone, Copy)]
pub struct Video<'a>(&'a NsVideo);

unsafe extern "C" {
    fn ns_anim_get_color(
        anim: *mut c_void,
        dom: *const c_void,
        which: c_int,
        out: *mut u8,
    ) -> GBoolean;
    fn ns_box_fieldset_legend_gap(
        b: *const NsBox,
        inset: *mut c_double,
        x0: *mut c_double,
        x1: *mut c_double,
        y0: *mut c_double,
        y1: *mut c_double,
    ) -> GBoolean;
    fn ns_inline_apply_atomic_shapes(attrs: *mut c_void, b: *const NsBox);
    fn ns_inline_layout_set_attrs(layout: *mut c_void, attrs: *mut c_void, b: *const NsBox);
    fn ns_control_css_extra_w(dom: *const c_void, s: *const Style) -> c_double;
    fn ns_spell_available() -> GBoolean;
    fn ns_spell_word_ok(word: *const c_char, len: isize, lang: *const c_char) -> GBoolean;
    fn ns_js_image_for_node(js: *mut c_void, el: *const c_void) -> *const c_void;
    fn ns_svg_render_node(
        cr: *mut c_void,
        svg: *const c_void,
        w: c_double,
        h: c_double,
        styles: *mut GHashTable,
        inherited: *const Style,
    );
    fn ns_math_paint(
        cr: *mut c_void,
        math: *const c_void,
        x: c_double,
        y: c_double,
        font_px: c_double,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );
    fn ns_video_helper_composited(v: *const NsVideo) -> GBoolean;
    fn ns_video_active_cue_text(v: *const NsVideo) -> *const c_char;
    fn ns_video_note_paint_rect(
        v: *const NsVideo,
        x: c_double,
        y: c_double,
        w: c_double,
        h: c_double,
        fit: c_int,
    );
    fn ns_video_note_paint_clip(
        v: *const NsVideo,
        x: c_double,
        y: c_double,
        w: c_double,
        h: c_double,
    );
    fn g_get_monotonic_time() -> i64;
    fn g_unichar_isalpha(c: u32) -> GBoolean;
    fn g_utf8_get_char(p: *const c_char) -> u32;
}

impl Texture {
    pub fn download(self, dst: &mut [u8], stride: usize) {
        self.get().download(dst, stride);
    }

    pub fn user_data(self) -> *mut c_void {
        self.get().user_data()
    }

    pub unsafe fn set_user_data(
        self,
        data: *mut c_void,
        destroy: unsafe extern "C" fn(*mut c_void),
    ) {
        // SAFETY: the caller promises destroy may be called once with data on any thread.
        unsafe { self.get().set_user_data(data, Some(destroy)) };
    }
}

impl Video<'_> {
    pub unsafe fn from_ptr<'a>(v: *mut c_void) -> Option<Video<'a>> {
        unsafe { v.cast::<NsVideo>().as_ref() }.map(Video)
    }

    fn raw(self) -> *const NsVideo {
        self.0
    }

    pub fn playing(self) -> bool {
        self.0.playing != 0
    }

    pub fn video_opened(self) -> bool {
        self.0.video_opened != 0
    }

    pub fn frame_texture(self) -> Option<Texture> {
        unsafe { Texture::from_raw(self.0.frame_texture) }
    }

    pub fn poster_texture(self) -> Option<Texture> {
        unsafe { Texture::from_raw(self.0.poster_texture) }
    }

    pub fn note_paint_rect(self, x: f64, y: f64, w: f64, h: f64, fit: i32) {
        unsafe { ns_video_note_paint_rect(self.raw(), x, y, w, h, fit) };
    }

    pub fn note_paint_clip(self, x: f64, y: f64, w: f64, h: f64) {
        unsafe { ns_video_note_paint_clip(self.raw(), x, y, w, h) };
    }
}

fn video_ptr(v: Option<Video<'_>>) -> *const NsVideo {
    v.map_or(ptr::null(), Video::raw)
}

pub fn video_helper_composited(v: Option<Video<'_>>) -> bool {
    unsafe { ns_video_helper_composited(video_ptr(v)) != 0 }
}

pub fn video_active_cue_text(v: Option<Video<'_>>) -> Option<&CStr> {
    let t = unsafe { ns_video_active_cue_text(video_ptr(v)) };
    (!t.is_null()).then(|| unsafe { CStr::from_ptr(t) })
}

pub fn anim_color(b: BoxRef<'_>, which: i32) -> Option<[u8; 4]> {
    let anim = crate::state::anim();
    let dom = b.dom_ptr();
    if anim.is_null() || dom.is_null() {
        return None;
    }
    let mut c = [0u8; 4];
    (unsafe { ns_anim_get_color(anim, dom, which, c.as_mut_ptr()) } != 0).then_some(c)
}

pub fn border_image_source(s: StyleRef<'_>) -> Option<ValueRef<'_>> {
    southstar_style::border_image_source(Some(s))
}

pub fn fieldset_legend_gap(b: BoxRef<'_>) -> Option<(f64, f64, f64, f64, f64)> {
    let (mut inset, mut x0, mut x1, mut y0, mut y1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let found = unsafe {
        ns_box_fieldset_legend_gap(b.as_ptr(), &mut inset, &mut x0, &mut x1, &mut y0, &mut y1)
    };
    (found != 0).then_some((inset, x0, x1, y0, y1))
}

pub fn inline_apply_atomic_shapes(attrs: &AttrList, b: BoxRef<'_>) {
    unsafe { ns_inline_apply_atomic_shapes(attrs.raw(), b.as_ptr()) };
}

pub fn inline_layout_set_attrs(layout: &Layout, attrs: &AttrList, b: BoxRef<'_>) {
    unsafe { ns_inline_layout_set_attrs(layout.raw(), attrs.raw(), b.as_ptr()) };
}

pub fn control_css_extra_w(r: &InlineAttr, s: StyleRef<'_>) -> f64 {
    unsafe { ns_control_css_extra_w(r.dom_ptr(), s.as_ptr()) }
}

pub fn vertical_stack_text(text: &CStr) -> CString {
    southstar_layout_geom::vertical_stack_text(text.to_bytes())
}

pub fn spell_available() -> bool {
    unsafe { ns_spell_available() != 0 }
}

pub fn spell_word_ok(word: &[u8]) -> bool {
    unsafe { ns_spell_word_ok(word.as_ptr().cast(), word.len() as isize, ptr::null()) != 0 }
}

pub fn node_spellcheck_host(n: Node<'_>) -> bool {
    southstar_dom::controls::spellcheck_host(n).is_some()
}

pub fn js_image_for_node(b: BoxRef<'_>) -> Option<ImageRef<'_>> {
    let js = crate::state::js();
    if js.is_null() {
        return None;
    }
    unsafe { ImageRef::from_ptr(ns_js_image_for_node(js, b.dom_ptr())) }
}

pub fn svg_render_node(cr: ContextRef, b: BoxRef<'_>, w: f64, h: f64) {
    unsafe { ns_svg_render_node(cr.as_raw(), b.dom_ptr(), w, h, b.svg_styles(), b.style()) };
}

pub fn math_paint(cr: ContextRef, b: BoxRef<'_>, x: f64, y: f64, font_px: f64, c: Rgba) {
    unsafe { ns_math_paint(cr.as_raw(), b.dom_ptr(), x, y, font_px, c.r, c.g, c.b, c.a) };
}

unsafe extern "C" {
    fn ns_anim_get_opacity(anim: *mut c_void, dom: *const c_void, out: *mut c_double) -> GBoolean;
    fn ns_anim_get_transform(anim: *mut c_void, dom: *const c_void) -> *const Transform;
    fn ns_box_is_fixed(b: *const NsBox) -> GBoolean;
    fn ns_box_in_scroller(b: *const NsBox) -> GBoolean;
    fn ns_box_sticky_offset(
        b: *const NsBox,
        x0: c_double,
        y0: c_double,
        x1: c_double,
        y1: c_double,
        dx: *mut c_double,
        dy: *mut c_double,
    );
    fn ns_box_hit_test(root: *const NsBox, x: c_double, y: c_double) -> *const NsBox;
    fn ns_selection_ranges(root: *const NsBox, sel: *const c_void) -> *mut GHashTable;
    fn ns_js_canvas_surface(js: *mut c_void, n: *const c_void) -> *mut c_void;
}

pub fn anim_opacity(b: BoxRef<'_>) -> Option<f64> {
    let anim = crate::state::anim();
    if anim.is_null() {
        return None;
    }
    let mut o = 0.0;
    (unsafe { ns_anim_get_opacity(anim, b.dom_ptr(), &mut o) } != 0).then_some(o)
}

pub fn effective_transform(b: BoxRef<'_>, s: Option<StyleRef<'_>>) -> Transform {
    let anim = crate::state::anim();
    let anim_tf = if anim.is_null() {
        None
    } else {
        unsafe { ns_anim_get_transform(anim, b.dom_ptr()).as_ref() }
    };
    southstar_style::effective_transform(s, anim_tf)
}

pub fn box_is_fixed(b: BoxRef<'_>) -> bool {
    unsafe { ns_box_is_fixed(b.as_ptr()) != 0 }
}

pub fn box_in_scroller(b: BoxRef<'_>) -> bool {
    unsafe { ns_box_in_scroller(b.as_ptr()) != 0 }
}

pub fn box_sticky_offset(b: BoxRef<'_>, clip: (f64, f64, f64, f64)) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_sticky_offset(b.as_ptr(), clip.0, clip.1, clip.2, clip.3, &mut dx, &mut dy) };
    (dx, dy)
}

pub fn box_hit_test(root: BoxRef<'_>, x: f64, y: f64) -> *const NsBox {
    unsafe { ns_box_hit_test(root.as_ptr(), x, y) }
}

pub fn active_modal<'a>() -> Option<Node<'a>> {
    southstar_dom::tree::active_modal()
}

pub unsafe fn selection_ranges(root: BoxRef<'_>, sel: *const c_void) -> *mut GHashTable {
    unsafe { ns_selection_ranges(root.as_ptr(), sel) }
}

pub fn canvas_surface(b: BoxRef<'_>) -> Option<Surface> {
    let js = crate::state::js();
    if js.is_null() {
        return None;
    }
    let surface = unsafe { ns_js_canvas_surface(js, b.dom_ptr()) };
    // SAFETY: the canvas keeps its surface alive while the page paints, and it is referenced here before anything can replace it.
    unsafe { SurfaceRef::from_raw(surface) }.map(|s| s.reference())
}

pub fn monotonic_time() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn unichar_isalpha(c: u32) -> bool {
    unsafe { g_unichar_isalpha(c) != 0 }
}

pub fn utf8_get_char(text: &CStr, pos: usize) -> u32 {
    if pos >= text.to_bytes().len() {
        return 0;
    }
    unsafe { g_utf8_get_char(text.as_ptr().add(pos)) }
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
