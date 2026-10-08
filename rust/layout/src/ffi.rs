//! Southstar — struct ns_box as layout.c lays it out, the borrowed box handle over it and the layout.h calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_glib::{GArray, GBoolean, GHashTable};

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
#[derive(Clone, Copy, Debug)]
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
    _rel_dx: f64,
    _rel_dy: f64,
    content_width: f64,
    content_height: f64,
    _first_baseline: f64,
    _definite_height: f64,
    _definite_height_before_flex: f64,
    _flex_pass_x: f64,
    _flex_pass_y: f64,
    _last_layout_width: f64,
    _definite_height_read: GBoolean,
    _measured_content_height: f64,
    _cb_height_override: f64,
    _flex_main_size: f64,
    _has_flex_main: GBoolean,
    _is_rendered_legend: GBoolean,
    _inline_split_tail: GBoolean,
    _margin_top_through: f64,
    _paint_top: f64,
    _paint_bottom: f64,
    margin: Edges,
    padding: Edges,
    border: Edges,
    scroll_x: f64,
    scroll_y: f64,
    _scroll_max_x: f64,
    _scroll_max_y: f64,
    _scrolls: GBoolean,
    text: *const c_char,
    _inline_layout_cache_style: *const Style,
    _inline_layout_cache_width: f64,
    _inline_layout_cache_height: f64,
    _inline_layout_cache_valid: GBoolean,
    _vertical_wm: c_int,
    _text_orient: c_int,
    _inline_natural_cache_style: *const Style,
    _inline_natural_cache_width: f64,
    _inline_natural_cache_valid: GBoolean,
    _inline_min_cache_style: *const Style,
    _inline_min_cache_width: f64,
    _inline_min_cache_valid: GBoolean,
    _paint_layout: *mut c_void,
    _links: *mut GArray,
    _attrs: *mut GArray,
    _inline_atomics: *mut GArray,
    _atomic_line_heights: *mut GArray,
    _table_col_hints: *mut GArray,
    _grid_col_tracks: *mut GArray,
    _grid_row_tracks: *mut GArray,
    _grid_explicit_cols: c_int,
    _grid_explicit_rows: c_int,
    media: *mut c_void,
    _svg_styles: *mut GHashTable,
    _colspan: c_int,
    _rowspan: c_int,
    _columns: c_int,
    parent: *const NsBox,
    first_child: *const NsBox,
    _last_child: *const NsBox,
    next_sibling: *const NsBox,
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(size_of::<NsBox>() == 560 && core::mem::offset_of!(NsBox, next_sibling) == 552);

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

    pub fn style(self) -> *const Style {
        self.raw().style
    }

    pub fn dom_ptr(self) -> *const c_void {
        self.raw().dom
    }

    pub fn media_ptr(self) -> *mut c_void {
        self.raw().media
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
}
