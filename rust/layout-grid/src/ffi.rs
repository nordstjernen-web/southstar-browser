//! Southstar — the resolved-tracks entry point getComputedStyle calls, and the style and box helpers grid layout borrows from the other layout crates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint};

use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::StyleRef;

use crate::abs;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TrackEdges {
    pub start: f64,
    pub end: f64,
}

unsafe extern "C" {
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

pub(crate) fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub(crate) use southstar_layout::dispatch::layout_box as layout_child;

pub(crate) use southstar_layout_sizing::measure_natural_width as natural_width;

pub(crate) fn min_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub(crate) use southstar_layout::dispatch::shift_box_tree as shift;

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
