//! Southstar — the engine calls painting makes into the style engine, fonts, textures and the C that remains, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, GStr};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style};
use southstar_style::{Gradient, NsCssValue, StyleRef, ValueRef};

use super::cairo::Cr;
use super::pango::{AttrList, Layout};
use crate::util::Rgba;

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
}

pub fn style_ptr(s: Option<StyleRef<'_>>) -> *const c_void {
    s.map_or(ptr::null(), |s| s.as_ptr().cast())
}

#[repr(C)]
#[derive(Default)]
pub struct BorderImage {
    pub slice: [f64; 4],
    pub slice_percent: [GBoolean; 4],
    pub fill: GBoolean,
    pub width: [f64; 4],
    pub width_unit: [c_int; 4],
    pub width_auto: [GBoolean; 4],
    pub outset: [f64; 4],
    pub outset_unit: [c_int; 4],
    pub tile: [c_int; 2],
}

const _: () = assert!(
    core::mem::size_of::<BorderImage>() == 176 && core::mem::offset_of!(BorderImage, width) == 56
);

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
    fn ns_texture_download(texture: *mut c_void, dst: *mut u8, stride: usize);
    fn ns_texture_get_user_data(texture: *mut c_void) -> *mut c_void;
    fn ns_texture_set_user_data(
        texture: *mut c_void,
        data: *mut c_void,
        destroy: unsafe extern "C" fn(*mut c_void),
    );
    fn ns_anim_get_color(
        anim: *mut c_void,
        dom: *const c_void,
        which: c_int,
        out: *mut u8,
    ) -> GBoolean;
    fn ns_css_gradient_angle(gr: *const Gradient, w: c_double, h: c_double) -> c_double;
    fn ns_css_gradient_radii(
        gr: *const Gradient,
        w: c_double,
        h: c_double,
        cx: c_double,
        cy: c_double,
        rx: *mut c_double,
        ry: *mut c_double,
    );
    fn ns_css_border_image_source(s: *const Style) -> *const NsCssValue;
    fn ns_css_border_image_params(s: *const Style, out: *mut BorderImage);
    fn ns_box_fieldset_legend_gap(
        b: *const NsBox,
        inset: *mut c_double,
        x0: *mut c_double,
        x1: *mut c_double,
        y0: *mut c_double,
        y1: *mut c_double,
    ) -> GBoolean;
    fn ns_paint_viewport_origin(x: *mut c_double, y: *mut c_double) -> GBoolean;
    fn ns_paint_selection_runs() -> *mut GHashTable;
    fn ns_paint_walk_atomic(cr: *mut c_void, b: *const NsBox, highlight: *const c_char);
    fn ns_paint_layers_mode() -> c_int;
    fn ns_paint_layers_note_video(cr: *mut c_void);
    fn ns_paint_video_hole_record(
        cr: *mut c_void,
        x: c_double,
        y: c_double,
        w: c_double,
        h: c_double,
    );
    fn ns_inline_text_indent_px(b: *const NsBox, s: *const Style, basis: c_double) -> c_double;
    fn ns_inline_apply_atomic_shapes(attrs: *mut c_void, b: *const NsBox);
    fn ns_inline_layout_set_attrs(layout: *mut c_void, attrs: *mut c_void, b: *const NsBox);
    fn ns_control_css_extra_w(dom: *const c_void, s: *const Style) -> c_double;
    fn ns_vertical_stack_text(text: *const c_char) -> *mut c_char;
    fn ns_spell_available() -> GBoolean;
    fn ns_spell_word_ok(word: *const c_char, len: isize, lang: *const c_char) -> GBoolean;
    fn ns_node_spellcheck_host(n: *const NsNode) -> *const NsNode;
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
        unsafe { ns_texture_download(self.0, dst.as_mut_ptr(), stride) };
    }

    pub fn user_data(self) -> *mut c_void {
        unsafe { ns_texture_get_user_data(self.0) }
    }

    pub unsafe fn set_user_data(
        self,
        data: *mut c_void,
        destroy: unsafe extern "C" fn(*mut c_void),
    ) {
        unsafe { ns_texture_set_user_data(self.0, data, destroy) };
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

pub fn gradient_angle(gr: &Gradient, w: f64, h: f64) -> f64 {
    unsafe { ns_css_gradient_angle(gr, w, h) }
}

pub fn gradient_radii(gr: &Gradient, w: f64, h: f64, cx: f64, cy: f64) -> (f64, f64) {
    let (mut rx, mut ry) = (1.0, 1.0);
    unsafe { ns_css_gradient_radii(gr, w, h, cx, cy, &mut rx, &mut ry) };
    (rx, ry)
}

pub fn border_image_source(s: StyleRef<'_>) -> Option<ValueRef<'_>> {
    unsafe { ValueRef::from_ptr(ns_css_border_image_source(s.as_ptr())) }
}

pub fn border_image_params(s: StyleRef<'_>) -> BorderImage {
    let mut bi = BorderImage::default();
    unsafe { ns_css_border_image_params(s.as_ptr(), &mut bi) };
    bi
}

pub fn fieldset_legend_gap(b: BoxRef<'_>) -> Option<(f64, f64, f64, f64, f64)> {
    let (mut inset, mut x0, mut x1, mut y0, mut y1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let found = unsafe {
        ns_box_fieldset_legend_gap(b.as_ptr(), &mut inset, &mut x0, &mut x1, &mut y0, &mut y1)
    };
    (found != 0).then_some((inset, x0, x1, y0, y1))
}

pub fn viewport_origin() -> (bool, f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let have = unsafe { ns_paint_viewport_origin(&mut x, &mut y) } != 0;
    (have, x, y)
}

pub fn selection_run(b: BoxRef<'_>) -> Option<SelectionRun> {
    let runs = unsafe { ns_paint_selection_runs() };
    if runs.is_null() {
        return None;
    }
    let run = unsafe { southstar_glib::g_hash_table_lookup(runs, b.as_ptr().cast()) };
    unsafe { run.cast::<SelectionRun>().as_ref() }.copied()
}

pub fn paint_walk_atomic(cr: Cr, b: BoxRef<'_>, highlight: Option<&CStr>) {
    unsafe {
        ns_paint_walk_atomic(
            cr.raw(),
            b.as_ptr(),
            highlight.map_or(ptr::null(), CStr::as_ptr),
        )
    };
}

pub fn layers_mode() -> i32 {
    unsafe { ns_paint_layers_mode() }
}

pub fn layers_note_video(cr: Cr) {
    unsafe { ns_paint_layers_note_video(cr.raw()) };
}

pub fn video_hole_record(cr: Cr, x: f64, y: f64, w: f64, h: f64) {
    unsafe { ns_paint_video_hole_record(cr.raw(), x, y, w, h) };
}

pub fn inline_text_indent_px(b: BoxRef<'_>, s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    let s = s.map_or(ptr::null(), StyleRef::as_ptr);
    unsafe { ns_inline_text_indent_px(b.as_ptr(), s, basis) }
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

pub fn vertical_stack_text(text: &CStr) -> GStr {
    unsafe { GStr::take(ns_vertical_stack_text(text.as_ptr())) }
        .unwrap_or_else(|| unsafe { GStr::take(southstar_glib::strdup(b"")) }.expect("g_malloc"))
}

pub fn spell_available() -> bool {
    unsafe { ns_spell_available() != 0 }
}

pub fn spell_word_ok(word: &[u8]) -> bool {
    unsafe { ns_spell_word_ok(word.as_ptr().cast(), word.len() as isize, ptr::null()) != 0 }
}

pub fn node_spellcheck_host(n: Node<'_>) -> bool {
    unsafe { !ns_node_spellcheck_host(n.as_ptr()).is_null() }
}

pub fn js_image_for_node(b: BoxRef<'_>) -> Option<ImageRef<'_>> {
    let js = crate::state::js();
    if js.is_null() {
        return None;
    }
    unsafe { ImageRef::from_ptr(ns_js_image_for_node(js, b.dom_ptr())) }
}

pub fn svg_render_node(cr: Cr, b: BoxRef<'_>, w: f64, h: f64) {
    unsafe { ns_svg_render_node(cr.raw(), b.dom_ptr(), w, h, b.svg_styles(), b.style()) };
}

pub fn math_paint(cr: Cr, b: BoxRef<'_>, x: f64, y: f64, font_px: f64, c: Rgba) {
    unsafe { ns_math_paint(cr.raw(), b.dom_ptr(), x, y, font_px, c.r, c.g, c.b, c.a) };
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
