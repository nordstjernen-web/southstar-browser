//! Southstar — the grid entry points layout.c calls and the layout.c and css.c calls grid layout makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};

use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_layout::{BoxRef, Edges, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, ValueRef};

use crate::abs::{self, Area};
use crate::{intrinsic, layout};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TrackEdges {
    pub start: f64,
    pub end: f64,
}

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_measure_natural_width(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_min_width_of(b: *const NsBox, parent_style: *const Style) -> f64;
    fn ns_layout_edges_from_style(
        s: *const Style,
        basis: f64,
        margin: *mut Edges,
        padding: *mut Edges,
        border: *mut Edges,
    );
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_layout_resolve_used_height(
        b: *const NsBox,
        hv: *const NsCssValue,
        width_basis: f64,
        fallback: f64,
    ) -> f64;
    fn ns_layout_specified_height_to_content(b: *const NsBox, h: f64) -> f64;
    fn ns_layout_clamp_height_minmax_px(s: *const Style, h: f64) -> f64;
    fn ns_layout_length_resolve(v: *const NsCssValue, basis: f64, fallback: f64) -> f64;
    fn ns_layout_overflow_establishes_bfc(s: *const Style) -> GBoolean;
    fn ns_layout_value_is_percent(v: *const NsCssValue) -> GBoolean;
    fn ns_layout_self_start_is_far_side(s: *const Style, horizontal_axis: GBoolean) -> GBoolean;
    fn ns_css_alignment_base(kw: *const c_char) -> *const c_char;
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn g_ascii_formatd(
        buffer: *mut c_char,
        buf_len: c_int,
        format: *const c_char,
        d: f64,
    ) -> *mut c_char;
}

fn value_ptr(v: Option<ValueRef<'_>>) -> *const NsCssValue {
    v.map_or(core::ptr::null(), ValueRef::as_ptr)
}

pub(crate) fn style_of<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub(crate) fn style_ptr(s: Option<StyleRef<'_>>) -> *const Style {
    s.map_or(core::ptr::null(), StyleRef::as_ptr)
}

pub(crate) fn layout_child(b: BoxRef<'_>, width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), width, inherited) }
}

pub(crate) fn natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_measure_natural_width(b.as_ptr(), parent_style) }
}

pub(crate) fn min_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    unsafe { ns_layout_min_width_of(b.as_ptr(), parent_style) }
}

pub(crate) fn edges_into(b: BoxRef<'_>, basis: f64) {
    let (m, p, e) = b.edges_mut();
    unsafe { ns_layout_edges_from_style(b.style(), basis, m, p, e) };
}

pub(crate) fn edges(s: *const Style, basis: f64) -> (Edges, Edges, Edges) {
    let mut m = Edges::default();
    let mut p = Edges::default();
    let mut b = Edges::default();
    unsafe { ns_layout_edges_from_style(s, basis, &mut m, &mut p, &mut b) };
    (m, p, b)
}

pub(crate) fn shift(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub(crate) fn used_height(
    b: BoxRef<'_>,
    v: Option<ValueRef<'_>>,
    basis: f64,
    fallback: f64,
) -> f64 {
    unsafe { ns_layout_resolve_used_height(b.as_ptr(), value_ptr(v), basis, fallback) }
}

pub(crate) fn height_to_content(b: BoxRef<'_>, h: f64) -> f64 {
    unsafe { ns_layout_specified_height_to_content(b.as_ptr(), h) }
}

pub(crate) fn clamp_height(s: Option<StyleRef<'_>>, h: f64) -> f64 {
    unsafe { ns_layout_clamp_height_minmax_px(style_ptr(s), h) }
}

pub(crate) fn resolve_length(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(value_ptr(v), basis, fallback) }
}

pub(crate) fn establishes_bfc(s: StyleRef<'_>) -> bool {
    unsafe { ns_layout_overflow_establishes_bfc(s.as_ptr()) != 0 }
}

pub(crate) fn is_percent(v: Option<ValueRef<'_>>) -> bool {
    unsafe { ns_layout_value_is_percent(value_ptr(v)) != 0 }
}

pub(crate) fn start_is_far_side(s: Option<StyleRef<'_>>, horizontal: bool) -> bool {
    unsafe { ns_layout_self_start_is_far_side(style_ptr(s), glib::boolean(horizontal)) != 0 }
}

pub(crate) fn alignment_base(kw: &CStr) -> &CStr {
    unsafe { CStr::from_ptr(ns_css_alignment_base(kw.as_ptr())) }
}

pub(crate) fn new_track_array() -> *mut GArray {
    unsafe { glib::g_array_new(0, 0, size_of::<TrackEdges>() as c_uint) }
}

pub(crate) fn free_track_array(a: *mut GArray) {
    if !a.is_null() {
        unsafe { g_array_free(a, glib::TRUE) };
    }
}

pub(crate) fn append_track(a: *mut GArray, e: TrackEdges) {
    unsafe { glib::g_array_append_vals(a, (&raw const e).cast(), 1) };
}

pub(crate) fn track_edges<'a>(a: *mut GArray) -> Option<&'a [TrackEdges]> {
    let a = unsafe { a.as_ref() }?;
    if a.len == 0 {
        return Some(&[]);
    }
    Some(unsafe { core::slice::from_raw_parts(a.data.cast::<TrackEdges>(), a.len as usize) })
}

pub(crate) fn format_g(value: f64, out: &mut Vec<u8>) {
    let mut buffer = [0 as c_char; 39];
    unsafe {
        g_ascii_formatd(
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            c"%g".as_ptr(),
            value,
        );
        out.extend_from_slice(CStr::from_ptr(buffer.as_ptr()).to_bytes());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid(
    b: *mut NsBox,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
    cursor_y_out: *mut f64,
) {
    let Some(b) = (unsafe { BoxRef::from_ptr(b) }) else {
        return;
    };
    let cursor_y = layout::layout_grid(b, cw, inner_x, inner_y, child_inherited);
    if let Some(out) = unsafe { cursor_y_out.as_mut() } {
        *out = cursor_y;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_flows_by_column(s: *const Style) -> GBoolean {
    glib::boolean(intrinsic::flows_by_column(unsafe { StyleRef::from_ptr(s) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_column_flow_width(
    b: *mut NsBox,
    child_style: *const Style,
    min_content: GBoolean,
) -> f64 {
    match unsafe { BoxRef::from_ptr(b) } {
        Some(b) => intrinsic::column_flow_width(b, child_style, min_content != 0),
        None => -1.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_natural_width(
    b: *mut NsBox,
    child_style: *const Style,
) -> f64 {
    match unsafe { BoxRef::from_ptr(b) } {
        Some(b) => intrinsic::natural_width(b, child_style),
        None => -1.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_abs_containing_block(
    cb: *const NsBox,
    st: *const Style,
    x: *mut f64,
    y: *mut f64,
    w: *mut f64,
    h: *mut f64,
) -> GBoolean {
    let (Some(cb), Some(st)) = (unsafe { BoxRef::from_ptr(cb) }, unsafe {
        StyleRef::from_ptr(st)
    }) else {
        return glib::FALSE;
    };
    let Some(area) = abs::containing_block(cb, st) else {
        return glib::FALSE;
    };
    unsafe {
        *x = area.x;
        *y = area.y;
        *w = area.w;
        *h = area.h;
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_static_position(
    abox: *mut NsBox,
    cb: *const NsBox,
    area_x: f64,
    area_y: f64,
    area_w: f64,
    area_h: f64,
    static_x: GBoolean,
    static_y: GBoolean,
) {
    let (Some(abox), Some(cb)) = (unsafe { BoxRef::from_ptr(abox) }, unsafe {
        BoxRef::from_ptr(cb)
    }) else {
        return;
    };
    let area = Area {
        x: area_x,
        y: area_y,
        w: area_w,
        h: area_h,
    };
    abs::static_position(abox, cb, &area, static_x != 0, static_y != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_static_align_offset(
    align: *const c_char,
    free_space: f64,
    flip: GBoolean,
) -> f64 {
    let align = (!align.is_null()).then(|| unsafe { CStr::from_ptr(align) });
    abs::static_align_offset(align, free_space, flip != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_grid_resolved_tracks(
    b: *const NsBox,
    columns: GBoolean,
) -> *mut c_char {
    let Some(b) = (unsafe { BoxRef::from_ptr(b) }) else {
        return core::ptr::null_mut();
    };
    match abs::resolved_tracks(b, columns != 0) {
        Some(text) => glib::strdup(&text),
        None => core::ptr::null_mut(),
    }
}
