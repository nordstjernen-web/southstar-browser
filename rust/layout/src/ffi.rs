//! Southstar — struct ns_box as layout.c lays it out, the borrowed box handle over it and the layout.h calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod image_map;
mod image_source;
mod tree;

pub use tree::{BoxPool, LinkVec, link_clear_fn};

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_glib::{GArray, GBoolean, GHashTable, GPtrArray};

#[repr(C)]
pub struct Style {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxKind {
    Block,
    Inline,
    Text,
    Image,
    Table,
    TableCaption,
    TableRow,
    TableCell,
    Video,
    Math,
    Svg,
    Other,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[repr(C)]
pub struct NsBox {
    kind: c_uint,
    dom: *const c_void,
    style: *const Style,
    x: f64,
    y: f64,
    rel_dx: f64,
    rel_dy: f64,
    content_width: f64,
    content_height: f64,
    first_baseline: f64,
    definite_height: f64,
    definite_height_before_flex: f64,
    flex_pass_x: f64,
    flex_pass_y: f64,
    last_layout_width: f64,
    definite_height_read: GBoolean,
    measured_content_height: f64,
    cb_height_override: f64,
    flex_main_size: f64,
    has_flex_main: GBoolean,
    is_rendered_legend: GBoolean,
    inline_split_tail: GBoolean,
    margin_top_through: f64,
    paint_top: f64,
    paint_bottom: f64,
    margin: Edges,
    padding: Edges,
    border: Edges,
    scroll_x: f64,
    scroll_y: f64,
    scroll_max_x: f64,
    scroll_max_y: f64,
    scrolls: GBoolean,
    text: *const c_char,
    inline_layout_cache_style: *const Style,
    inline_layout_cache_width: f64,
    inline_layout_cache_height: f64,
    inline_layout_cache_valid: GBoolean,
    vertical_wm: c_int,
    text_orient: c_int,
    inline_natural_cache_style: *const Style,
    inline_natural_cache_width: f64,
    inline_natural_cache_valid: GBoolean,
    inline_min_cache_style: *const Style,
    inline_min_cache_width: f64,
    inline_min_cache_valid: GBoolean,
    paint_layout: *mut c_void,
    links: *mut GArray,
    attrs: *mut GArray,
    inline_atomics: *mut GArray,
    atomic_line_heights: *mut GArray,
    table_col_hints: *mut GArray,
    grid_col_tracks: *mut GArray,
    grid_row_tracks: *mut GArray,
    grid_explicit_cols: c_int,
    grid_explicit_rows: c_int,
    media: *mut NsBoxMedia,
    svg_styles: *mut GHashTable,
    colspan: c_int,
    rowspan: c_int,
    columns: c_int,
    parent: *const NsBox,
    first_child: *const NsBox,
    last_child: *const NsBox,
    next_sibling: *const NsBox,
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(size_of::<NsBox>() == 560 && core::mem::offset_of!(NsBox, next_sibling) == 552);

#[repr(C)]
pub struct NsBoxMedia {
    image_src: *mut c_char,
    image: *mut c_void,
    bg_image_src: *mut c_char,
    bg_image: *mut c_void,
    marker_image_src: *mut c_char,
    marker_image: *mut c_void,
    border_image_src: *mut c_char,
    border_image: *mut c_void,
    bg_layer_srcs: *mut GPtrArray,
    bg_layer_images: *mut GPtrArray,
    video_src: *mut c_char,
    video_poster: *mut c_char,
    video_audio_src: *mut c_char,
    video: *mut c_void,
    declared_image_size: GBoolean,
    placeholder_image_size: GBoolean,
    size_independent_of_image: GBoolean,
    intrinsic_ratio_only: GBoolean,
    image_density: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::offset_of!(NsBoxMedia, video) == 104
        && core::mem::offset_of!(NsBoxMedia, image_density) == 128
        && size_of::<NsBoxMedia>() == 136
);

#[repr(C)]
pub struct TableColHint {
    style: *const Style,
    span: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<TableColHint>() == 16);

impl TableColHint {
    pub fn style(&self) -> *const Style {
        self.style
    }

    pub fn span(&self) -> c_int {
        self.span
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InlineAtomic {
    byte_off: usize,
    b: *const NsBox,
    owner_offset_x: f64,
    owner_offset_y: f64,
}

impl InlineAtomic {
    pub fn new(byte_off: usize, b: *const NsBox) -> InlineAtomic {
        InlineAtomic {
            byte_off,
            b,
            owner_offset_x: 0.0,
            owner_offset_y: 0.0,
        }
    }

    pub fn byte_off(&self) -> usize {
        self.byte_off
    }

    pub fn box_ref(&self) -> Option<BoxRef<'_>> {
        unsafe { BoxRef::from_ptr(self.b) }
    }

    pub fn owner_offset(&self) -> (f64, f64) {
        (self.owner_offset_x, self.owner_offset_y)
    }

    pub fn set_byte_off(&mut self, byte_off: usize) {
        self.byte_off = byte_off;
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LinkRange {
    pub start: usize,
    pub len: usize,
    href: *const c_char,
    target: *const c_char,
    dom: *const c_void,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<LinkRange>() == 40);

impl LinkRange {
    pub fn new(
        start: usize,
        len: usize,
        href: *const c_char,
        target: *const c_char,
        dom: *const c_void,
    ) -> LinkRange {
        LinkRange {
            start,
            len,
            href,
            target,
            dom,
        }
    }

    pub fn href_ptr(&self) -> *const c_char {
        self.href
    }

    pub fn target_ptr(&self) -> *const c_char {
        self.target
    }

    pub fn dom_ptr(&self) -> *const c_void {
        self.dom
    }
}

pub mod inline_kind {
    use core::ffi::c_uint;

    pub const BOLD: c_uint = 0;
    pub const ITALIC: c_uint = 1;
    pub const MONOSPACE: c_uint = 2;
    pub const UNDERLINE: c_uint = 3;
    pub const OVERLINE: c_uint = 4;
    pub const STRIKETHROUGH: c_uint = 5;
    pub const INPUT_FIELD: c_uint = 6;
    pub const INPUT_FIELD_FOCUSED: c_uint = 7;
    pub const BUTTON: c_uint = 8;
    pub const CHECKBOX: c_uint = 9;
    pub const CHECKBOX_CHECKED: c_uint = 10;
    pub const RADIO: c_uint = 11;
    pub const RADIO_CHECKED: c_uint = 12;
    pub const PROGRESS: c_uint = 13;
    pub const METER: c_uint = 14;
    pub const FONT_SIZE: c_uint = 15;
    pub const FONT_WEIGHT: c_uint = 16;
    pub const FONT_STRETCH: c_uint = 17;
    pub const FONT_FEATURES: c_uint = 18;
    pub const FONT_VARIATIONS: c_uint = 19;
    pub const COLOR: c_uint = 20;
    pub const FONT_FAMILY: c_uint = 21;
    pub const BG_COLOR: c_uint = 22;
    pub const SUPERSCRIPT: c_uint = 23;
    pub const SUBSCRIPT: c_uint = 24;
    pub const SMALL_CAPS: c_uint = 25;
    pub const CARET: c_uint = 26;
    pub const SELECTION: c_uint = 27;
    pub const ELEMENT: c_uint = 28;
    pub const SPACER: c_uint = 29;
    pub const SPELLCHECK: c_uint = 30;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InlineAttr {
    pub kind: c_uint,
    pub start: usize,
    pub len: usize,
    pub font_size_px: f64,
    pub font_weight: c_int,
    pub font_stretch: c_int,
    pub font_kerning: c_int,
    font_ligatures: *const c_char,
    font_features: *const c_char,
    font_variations: *const c_char,
    pub box_w: f64,
    pub box_h: f64,
    pub native_chrome: GBoolean,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    family: *const c_char,
    dom: *const c_void,
    style: *const Style,
    bg_image_src: *const c_char,
    bg_image: *mut c_void,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<InlineAttr>() == 136
        && core::mem::offset_of!(InlineAttr, box_w) == 72
        && core::mem::offset_of!(InlineAttr, family) == 96
        && core::mem::offset_of!(InlineAttr, bg_image) == 128
);

impl InlineAttr {
    pub fn new(kind: c_uint, start: usize, len: usize) -> InlineAttr {
        InlineAttr {
            kind,
            start,
            len,
            font_size_px: 0.0,
            font_weight: 0,
            font_stretch: 0,
            font_kerning: 0,
            font_ligatures: core::ptr::null(),
            font_features: core::ptr::null(),
            font_variations: core::ptr::null(),
            box_w: 0.0,
            box_h: 0.0,
            native_chrome: 0,
            r: 0,
            g: 0,
            b: 0,
            a: 0,
            family: core::ptr::null(),
            dom: core::ptr::null(),
            style: core::ptr::null(),
            bg_image_src: core::ptr::null(),
            bg_image: core::ptr::null_mut(),
        }
    }

    pub fn with_font_settings(
        mut self,
        ligatures: *const c_char,
        features: *const c_char,
        variations: *const c_char,
    ) -> InlineAttr {
        self.font_ligatures = ligatures;
        self.font_features = features;
        self.font_variations = variations;
        self
    }

    pub fn with_family(mut self, family: *const c_char) -> InlineAttr {
        self.family = family;
        self
    }

    pub fn with_dom(mut self, dom: *const c_void) -> InlineAttr {
        self.dom = dom;
        self
    }

    pub fn with_style(mut self, style: *const Style) -> InlineAttr {
        self.style = style;
        self
    }

    pub fn with_bg_image(mut self, src: *const c_char, image: *mut c_void) -> InlineAttr {
        self.bg_image_src = src;
        self.bg_image = image;
        self
    }

    pub fn font_ligatures(&self) -> Option<&CStr> {
        c_str(self.font_ligatures)
    }

    pub fn font_features(&self) -> Option<&CStr> {
        c_str(self.font_features)
    }

    pub fn font_variations(&self) -> Option<&CStr> {
        c_str(self.font_variations)
    }

    pub fn family(&self) -> Option<&CStr> {
        c_str(self.family)
    }

    pub fn dom_ptr(&self) -> *const c_void {
        self.dom
    }

    pub fn style(&self) -> *const Style {
        self.style
    }

    pub fn bg_image(&self) -> *mut c_void {
        self.bg_image
    }

    pub fn bg_image_src(&self) -> Option<&CStr> {
        c_str(self.bg_image_src)
    }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[derive(Clone, Copy)]
pub struct MediaRef<'a>(NonNull<NsBoxMedia>, PhantomData<&'a NsBoxMedia>);

impl<'a> MediaRef<'a> {
    fn raw(self) -> &'a NsBoxMedia {
        unsafe { &*self.0.as_ptr() }
    }

    pub fn image_src(self) -> Option<&'a CStr> {
        c_str(self.raw().image_src)
    }

    pub fn bg_image_src(self) -> Option<&'a CStr> {
        c_str(self.raw().bg_image_src)
    }

    pub fn marker_image_src(self) -> Option<&'a CStr> {
        c_str(self.raw().marker_image_src)
    }

    pub fn border_image_src(self) -> Option<&'a CStr> {
        c_str(self.raw().border_image_src)
    }

    pub fn bg_layer_srcs(self) -> Option<Vec<Option<&'a CStr>>> {
        let layers = unsafe { self.raw().bg_layer_srcs.as_ref() }?;
        Some(
            (0..layers.len as usize)
                .map(|i| c_str(unsafe { *layers.pdata.add(i) }.cast()))
                .collect(),
        )
    }

    pub fn video_src(self) -> Option<&'a CStr> {
        c_str(self.raw().video_src)
    }

    pub fn video_audio_src(self) -> Option<&'a CStr> {
        c_str(self.raw().video_audio_src)
    }

    pub fn video_poster(self) -> Option<&'a CStr> {
        c_str(self.raw().video_poster)
    }

    pub fn image(self) -> *mut c_void {
        self.raw().image
    }

    pub fn bg_image(self) -> *mut c_void {
        self.raw().bg_image
    }

    pub fn marker_image(self) -> *mut c_void {
        self.raw().marker_image
    }

    pub fn border_image(self) -> *mut c_void {
        self.raw().border_image
    }

    pub fn bg_layer_images(self) -> Option<Vec<*mut c_void>> {
        let layers = unsafe { self.raw().bg_layer_images.as_ref() }?;
        Some(
            (0..layers.len as usize)
                .map(|i| unsafe { *layers.pdata.add(i) })
                .collect(),
        )
    }

    pub fn video(self) -> *mut c_void {
        self.raw().video
    }

    pub fn set_video(self, video: *mut c_void) {
        unsafe { (*self.0.as_ptr()).video = video };
    }

    pub fn declared_image_size(self) -> bool {
        self.raw().declared_image_size != 0
    }

    pub fn placeholder_image_size(self) -> bool {
        self.raw().placeholder_image_size != 0
    }

    pub fn intrinsic_ratio_only(self) -> bool {
        self.raw().intrinsic_ratio_only != 0
    }

    pub fn image_density(self) -> f64 {
        self.raw().image_density
    }

    pub fn set_size_independent_of_image(self, independent: bool) {
        unsafe { (*self.0.as_ptr()).size_independent_of_image = GBoolean::from(independent) };
    }
}

unsafe extern "C" {
    fn ns_box_max_bottom(root: *const NsBox, seed: f64) -> f64;
    fn ns_box_clips_out_point(b: *const NsBox, x: f64, y: f64) -> GBoolean;
}

#[derive(Clone, Copy)]
pub struct BoxRef<'a>(NonNull<NsBox>, PhantomData<&'a NsBox>);

impl<'a> BoxRef<'a> {
    pub unsafe fn from_ptr(b: *const NsBox) -> Option<BoxRef<'a>> {
        NonNull::new(b.cast_mut()).map(|p| BoxRef(p, PhantomData))
    }

    pub fn as_ptr(self) -> *const NsBox {
        self.0.as_ptr()
    }

    fn raw(self) -> &'a NsBox {
        unsafe { &*self.0.as_ptr() }
    }

    pub fn kind(self) -> BoxKind {
        match self.raw().kind {
            0 => BoxKind::Block,
            1 => BoxKind::Inline,
            2 => BoxKind::Text,
            3 => BoxKind::Image,
            4 => BoxKind::Table,
            5 => BoxKind::TableCaption,
            6 => BoxKind::TableRow,
            7 => BoxKind::TableCell,
            8 => BoxKind::Video,
            9 => BoxKind::Math,
            10 => BoxKind::Svg,
            _ => BoxKind::Other,
        }
    }

    pub fn kind_raw(self) -> c_uint {
        self.raw().kind
    }

    pub fn style(self) -> *const Style {
        self.raw().style
    }

    pub fn dom_ptr(self) -> *const c_void {
        self.raw().dom
    }

    pub fn media(self) -> Option<MediaRef<'a>> {
        NonNull::new(self.raw().media).map(|m| MediaRef(m, PhantomData))
    }

    pub fn inline_atomic_boxes(self) -> Vec<BoxRef<'a>> {
        let Some(atomics) = (unsafe { self.raw().inline_atomics.as_ref() }) else {
            return Vec::new();
        };
        let data = atomics.data.cast::<InlineAtomic>();
        (0..atomics.len as usize)
            .filter_map(|i| unsafe { BoxRef::from_ptr((*data.add(i)).b) })
            .collect()
    }

    pub fn x(self) -> f64 {
        self.raw().x
    }

    pub fn y(self) -> f64 {
        self.raw().y
    }

    pub fn content_width(self) -> f64 {
        self.raw().content_width
    }

    pub fn content_height(self) -> f64 {
        self.raw().content_height
    }

    pub fn scroll_x(self) -> f64 {
        self.raw().scroll_x
    }

    pub fn scroll_y(self) -> f64 {
        self.raw().scroll_y
    }

    pub fn scroll_max_x(self) -> f64 {
        self.raw().scroll_max_x
    }

    pub fn scroll_max_y(self) -> f64 {
        self.raw().scroll_max_y
    }

    pub fn scrolls(self) -> bool {
        self.raw().scrolls != 0
    }

    pub fn set_scroll(self, x: f64, y: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).scroll_y = y;
            (*b).scroll_x = x;
        }
    }

    pub fn margin(self) -> Edges {
        self.raw().margin
    }

    pub fn padding(self) -> Edges {
        self.raw().padding
    }

    pub fn border(self) -> Edges {
        self.raw().border
    }

    pub fn text(self) -> Option<&'a CStr> {
        let text = self.raw().text;
        (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
    }

    pub fn parent(self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.raw().parent) }
    }

    pub fn first_child(self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.raw().first_child) }
    }

    pub fn next_sibling(self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.raw().next_sibling) }
    }

    pub fn max_bottom(self, seed: f64) -> f64 {
        unsafe { ns_box_max_bottom(self.as_ptr(), seed) }
    }

    pub fn clips_out_point(self, x: f64, y: f64) -> bool {
        unsafe { ns_box_clips_out_point(self.as_ptr(), x, y) != 0 }
    }

    pub fn same(self, other: BoxRef<'_>) -> bool {
        core::ptr::eq(self.as_ptr(), other.as_ptr())
    }

    pub fn rel_dx(self) -> f64 {
        self.raw().rel_dx
    }

    pub fn rel_dy(self) -> f64 {
        self.raw().rel_dy
    }

    pub fn set_rel_offset(self, dx: f64, dy: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).rel_dx = dx;
            (*b).rel_dy = dy;
        }
    }

    pub fn paint_top(self) -> f64 {
        self.raw().paint_top
    }

    pub fn paint_bottom(self) -> f64 {
        self.raw().paint_bottom
    }

    pub fn set_paint_bounds(self, top: f64, bottom: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).paint_top = top;
            (*b).paint_bottom = bottom;
        }
    }

    pub fn vertical_wm(self) -> c_int {
        self.raw().vertical_wm
    }

    pub fn text_orient(self) -> c_int {
        self.raw().text_orient
    }

    pub fn columns(self) -> c_int {
        self.raw().columns
    }

    pub fn svg_styles(self) -> *mut GHashTable {
        self.raw().svg_styles
    }

    pub fn text_ptr(self) -> *const c_char {
        self.raw().text
    }

    pub fn paint_layout(self) -> *mut c_void {
        self.raw().paint_layout
    }

    pub fn set_paint_layout(self, layout: *mut c_void) {
        unsafe { (*self.0.as_ptr()).paint_layout = layout };
    }

    pub fn attrs(self) -> &'a [InlineAttr] {
        let Some(attrs) = (unsafe { self.raw().attrs.as_ref() }) else {
            return &[];
        };
        if attrs.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(attrs.data.cast::<InlineAttr>(), attrs.len as usize) }
    }

    pub fn links(self) -> &'a [LinkRange] {
        let Some(links) = (unsafe { self.raw().links.as_ref() }) else {
            return &[];
        };
        if links.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(links.data.cast::<LinkRange>(), links.len as usize) }
    }

    pub fn has_attrs(self) -> bool {
        !self.raw().attrs.is_null()
    }

    pub fn inline_atomics(self) -> Option<&'a [InlineAtomic]> {
        let atomics = unsafe { self.raw().inline_atomics.as_ref() }?;
        if atomics.len == 0 {
            return Some(&[]);
        }
        Some(unsafe {
            core::slice::from_raw_parts(atomics.data.cast::<InlineAtomic>(), atomics.len as usize)
        })
    }

    pub fn set_atomic_owner_offset(self, index: usize, x: f64, y: f64) {
        let Some(atomics) = (unsafe { self.raw().inline_atomics.as_ref() }) else {
            return;
        };
        if index >= atomics.len as usize {
            return;
        }
        let atomic = unsafe { atomics.data.cast::<InlineAtomic>().add(index) };
        unsafe {
            (*atomic).owner_offset_x = x;
            (*atomic).owner_offset_y = y;
        }
    }

    pub fn as_mut_ptr(self) -> *mut NsBox {
        self.0.as_ptr()
    }

    pub fn colspan(self) -> c_int {
        self.raw().colspan
    }

    pub fn rowspan(self) -> c_int {
        self.raw().rowspan
    }

    pub fn table_col_hints(self) -> &'a [TableColHint] {
        let Some(hints) = (unsafe { self.raw().table_col_hints.as_ref() }) else {
            return &[];
        };
        if hints.len == 0 {
            return &[];
        }
        unsafe {
            core::slice::from_raw_parts(hints.data.cast::<TableColHint>(), hints.len as usize)
        }
    }

    pub fn set_x(self, x: f64) {
        unsafe { (*self.0.as_ptr()).x = x };
    }

    pub fn set_y(self, y: f64) {
        unsafe { (*self.0.as_ptr()).y = y };
    }

    pub fn set_content_width(self, w: f64) {
        unsafe { (*self.0.as_ptr()).content_width = w };
    }

    pub fn set_content_height(self, h: f64) {
        unsafe { (*self.0.as_ptr()).content_height = h };
    }

    pub fn set_margin(self, e: Edges) {
        unsafe { (*self.0.as_ptr()).margin = e };
    }

    pub fn take_flex_main(self) -> Option<f64> {
        let b = self.0.as_ptr();
        unsafe {
            if (*b).has_flex_main == 0 {
                return None;
            }
            (*b).has_flex_main = 0;
            Some((*b).flex_main_size)
        }
    }

    pub fn margin_top_through(self) -> f64 {
        self.raw().margin_top_through
    }

    pub fn set_margin_top_through(self, v: f64) {
        unsafe { (*self.0.as_ptr()).margin_top_through = v };
    }

    pub fn set_columns(self, n: c_int) {
        unsafe { (*self.0.as_ptr()).columns = n };
    }

    pub fn set_scrolls(self, scrolls: bool) {
        unsafe { (*self.0.as_ptr()).scrolls = GBoolean::from(scrolls) };
    }

    pub fn set_scroll_max(self, x: Option<f64>, y: Option<f64>) {
        let b = self.0.as_ptr();
        unsafe {
            if let Some(x) = x {
                (*b).scroll_max_x = x;
            }
            if let Some(y) = y {
                (*b).scroll_max_y = y;
            }
        }
    }

    pub fn begin_layout(self, parent_content_width: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).definite_height_read = 0;
            (*b).last_layout_width = parent_content_width;
        }
    }

    pub fn translate_grid_tracks(self, dx: f64, dy: f64) {
        for (tracks, d) in [
            (self.raw().grid_col_tracks, dx),
            (self.raw().grid_row_tracks, dy),
        ] {
            let Some(tracks) = (unsafe { tracks.as_ref() }) else {
                continue;
            };
            let edges = tracks.data.cast::<[f64; 2]>();
            for i in 0..tracks.len as usize {
                unsafe {
                    (*edges.add(i))[0] += d;
                    (*edges.add(i))[1] += d;
                }
            }
        }
    }

    pub fn detach_child(self, child: BoxRef<'_>) {
        let parent = self.0.as_ptr();
        let child = child.0.as_ptr();
        unsafe {
            let mut prev: *mut NsBox = core::ptr::null_mut();
            let mut c = (*parent).first_child.cast_mut();
            while !c.is_null() && c != child {
                prev = c;
                c = (*c).next_sibling.cast_mut();
            }
            if prev.is_null() {
                (*parent).first_child = (*child).next_sibling;
            } else {
                (*prev).next_sibling = (*child).next_sibling;
            }
            if (*parent).last_child == child.cast_const() {
                (*parent).last_child = prev;
            }
            (*child).next_sibling = core::ptr::null();
        }
    }

    pub fn prepend_child(self, child: BoxRef<'_>) {
        let parent = self.0.as_ptr();
        let child = child.0.as_ptr();
        unsafe {
            (*child).next_sibling = (*parent).first_child;
            (*parent).first_child = child;
            if (*parent).last_child.is_null() {
                (*parent).last_child = child;
            }
        }
    }

    pub fn set_padding(self, e: Edges) {
        unsafe { (*self.0.as_ptr()).padding = e };
    }

    pub fn is_rendered_legend(self) -> bool {
        self.raw().is_rendered_legend != 0
    }

    pub fn set_border(self, e: Edges) {
        unsafe { (*self.0.as_ptr()).border = e };
    }

    pub fn definite_height(self) -> f64 {
        self.raw().definite_height
    }

    pub fn set_definite_height(self, h: f64) {
        unsafe { (*self.0.as_ptr()).definite_height = h };
    }

    pub fn definite_height_before_flex(self) -> f64 {
        self.raw().definite_height_before_flex
    }

    pub fn set_definite_height_before_flex(self, h: f64) {
        unsafe { (*self.0.as_ptr()).definite_height_before_flex = h };
    }

    pub fn definite_height_read(self) -> bool {
        self.raw().definite_height_read != 0
    }

    pub fn flex_pass(self) -> (f64, f64) {
        let b = self.raw();
        (b.flex_pass_x, b.flex_pass_y)
    }

    pub fn set_flex_pass(self, x: f64, y: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).flex_pass_x = x;
            (*b).flex_pass_y = y;
        }
    }

    pub fn last_layout_width(self) -> f64 {
        self.raw().last_layout_width
    }

    pub fn measured_content_height(self) -> f64 {
        self.raw().measured_content_height
    }

    pub fn set_measured_content_height(self, h: f64) {
        unsafe { (*self.0.as_ptr()).measured_content_height = h };
    }

    pub fn set_flex_main(self, size: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).flex_main_size = size;
            (*b).has_flex_main = 1;
        }
    }

    pub fn set_scroll_overflow_y(self, max_y: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).scrolls = 1;
            (*b).scroll_max_y = max_y;
        }
    }

    pub fn set_cb_height_override(self, h: f64) {
        unsafe { (*self.0.as_ptr()).cb_height_override = h };
    }

    pub fn grid_tracks(self, columns: bool) -> *mut GArray {
        if columns {
            self.raw().grid_col_tracks
        } else {
            self.raw().grid_row_tracks
        }
    }

    pub fn set_grid_tracks(self, cols: *mut GArray, rows: *mut GArray) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).grid_col_tracks = cols;
            (*b).grid_row_tracks = rows;
        }
    }

    pub fn grid_explicit(self) -> (c_int, c_int) {
        let b = self.raw();
        (b.grid_explicit_cols, b.grid_explicit_rows)
    }

    pub fn set_grid_explicit(self, cols: c_int, rows: c_int) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).grid_explicit_cols = cols;
            (*b).grid_explicit_rows = rows;
        }
    }

    pub fn edges_mut(self) -> (*mut Edges, *mut Edges, *mut Edges) {
        let b = self.0.as_ptr();
        unsafe {
            (
                &raw mut (*b).margin,
                &raw mut (*b).padding,
                &raw mut (*b).border,
            )
        }
    }

    pub fn first_baseline(self) -> f64 {
        self.raw().first_baseline
    }

    pub fn set_first_baseline(self, v: f64) {
        unsafe { (*self.0.as_ptr()).first_baseline = v };
    }

    pub fn set_writing_mode(self, vertical_wm: c_int, text_orient: c_int) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).vertical_wm = vertical_wm;
            (*b).text_orient = text_orient;
        }
    }

    pub fn inline_layout_cache(self) -> Option<(*const Style, f64, f64)> {
        let b = self.raw();
        (b.inline_layout_cache_valid != 0).then_some((
            b.inline_layout_cache_style,
            b.inline_layout_cache_width,
            b.inline_layout_cache_height,
        ))
    }

    pub fn set_inline_layout_cache(self, cache: Option<(*const Style, f64, f64)>) {
        let b = self.0.as_ptr();
        unsafe {
            match cache {
                Some((style, width, height)) => {
                    (*b).inline_layout_cache_style = style;
                    (*b).inline_layout_cache_width = width;
                    (*b).inline_layout_cache_height = height;
                    (*b).inline_layout_cache_valid = 1;
                }
                None => (*b).inline_layout_cache_valid = 0,
            }
        }
    }

    pub fn inline_natural_cache(self) -> Option<(*const Style, f64)> {
        let b = self.raw();
        (b.inline_natural_cache_valid != 0)
            .then_some((b.inline_natural_cache_style, b.inline_natural_cache_width))
    }

    pub fn set_inline_natural_cache(self, style: *const Style, width: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).inline_natural_cache_style = style;
            (*b).inline_natural_cache_width = width;
            (*b).inline_natural_cache_valid = 1;
        }
    }

    pub fn inline_min_cache(self) -> Option<(*const Style, f64)> {
        let b = self.raw();
        (b.inline_min_cache_valid != 0)
            .then_some((b.inline_min_cache_style, b.inline_min_cache_width))
    }

    pub fn set_inline_min_cache(self, style: *const Style, width: f64) {
        let b = self.0.as_ptr();
        unsafe {
            (*b).inline_min_cache_style = style;
            (*b).inline_min_cache_width = width;
            (*b).inline_min_cache_valid = 1;
        }
    }

    pub fn set_atomic_line_heights(self, heights: &[f64]) {
        let b = self.0.as_ptr();
        unsafe {
            if (*b).atomic_line_heights.is_null() {
                (*b).atomic_line_heights =
                    southstar_glib::g_array_new(0, 0, size_of::<f64>() as c_uint);
            }
            let a = (*b).atomic_line_heights;
            (*a).len = 0;
            southstar_glib::g_array_append_vals(
                a,
                heights.as_ptr().cast(),
                heights.len() as c_uint,
            );
        }
    }

    pub fn atomic_line_heights(self) -> Option<&'a [f64]> {
        let heights = unsafe { self.raw().atomic_line_heights.as_ref() }?;
        if heights.len == 0 {
            return Some(&[]);
        }
        Some(unsafe {
            core::slice::from_raw_parts(heights.data.cast::<f64>(), heights.len as usize)
        })
    }
}
