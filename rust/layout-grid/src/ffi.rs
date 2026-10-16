//! Southstar — the grid entry points layout.c calls and the layout.c and css.c calls grid layout makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint};

use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::StyleRef;

use crate::abs;
use crate::intrinsic;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TrackEdges {
    pub start: f64,
    pub end: f64,
}

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

pub(crate) fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub(crate) fn layout_child(b: BoxRef<'_>, width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), width, inherited) }
}

pub(crate) use southstar_layout_sizing::measure_natural_width as natural_width;

pub(crate) fn min_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub(crate) fn shift(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub(crate) fn establishes_bfc(s: StyleRef<'_>) -> bool {
    southstar_layout_float::overflow_establishes_bfc(Some(s))
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
