//! Southstar — the ns-pango calls behind canvas text: a layout on a cairo context, its extents and the font's metrics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CString;

use super::cairo::{Cairo, Context};

pub const PANGO_SCALE: f64 = 1024.0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

unsafe extern "C" {
    fn g_object_unref(object: *mut c_void);
    fn ns_paint_pango_font_size(size_px: f64) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_cairo_create_layout")]
    fn pango_cairo_create_layout(cr: *mut Cairo) -> *mut c_void;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_cairo_show_layout")]
    fn pango_cairo_show_layout(cr: *mut Cairo, layout: *mut c_void);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_cairo_layout_path")]
    fn pango_cairo_layout_path(cr: *mut Cairo, layout: *mut c_void);
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_font_description_from_string"
    )]
    fn pango_font_description_from_string(text: *const c_char) -> *mut c_void;
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_font_description_set_absolute_size"
    )]
    fn pango_font_description_set_absolute_size(desc: *mut c_void, size: f64);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_description_free")]
    fn pango_font_description_free(desc: *mut c_void);
    #[cfg_attr(
        feature = "ns-pango",
        link_name = "ns_pango_layout_set_font_description"
    )]
    fn pango_layout_set_font_description(layout: *mut c_void, desc: *const c_void);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_set_text")]
    fn pango_layout_set_text(layout: *mut c_void, text: *const c_char, length: c_int);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_get_extents")]
    fn pango_layout_get_extents(layout: *mut c_void, ink: *mut Rectangle, logical: *mut Rectangle);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_get_baseline")]
    fn pango_layout_get_baseline(layout: *mut c_void) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_layout_get_context")]
    fn pango_layout_get_context(layout: *mut c_void) -> *mut c_void;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_context_get_metrics")]
    fn pango_context_get_metrics(
        context: *mut c_void,
        desc: *const c_void,
        language: *const c_void,
    ) -> *mut c_void;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_metrics_get_ascent")]
    fn pango_font_metrics_get_ascent(metrics: *mut c_void) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_metrics_get_descent")]
    fn pango_font_metrics_get_descent(metrics: *mut c_void) -> c_int;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_font_metrics_unref")]
    fn pango_font_metrics_unref(metrics: *mut c_void);
}

pub struct FontDescription(*mut c_void);

impl FontDescription {
    pub fn new(families: &[u8], size_px: f64) -> FontDescription {
        let families = CString::new(families).unwrap_or_default();
        let desc = unsafe { pango_font_description_from_string(families.as_ptr()) };
        let size = unsafe { ns_paint_pango_font_size(size_px) };
        unsafe { pango_font_description_set_absolute_size(desc, f64::from(size)) };
        FontDescription(desc)
    }
}

impl Drop for FontDescription {
    fn drop(&mut self) {
        unsafe { pango_font_description_free(self.0) };
    }
}

pub struct Layout(*mut c_void);

pub struct Extents {
    pub ink: Rectangle,
    pub logical: Rectangle,
    pub baseline: c_int,
}

impl Layout {
    pub fn new(cr: Context, desc: &FontDescription, text: &[u8]) -> Layout {
        let text = CString::new(text).unwrap_or_default();
        let layout = Layout(unsafe { pango_cairo_create_layout(cr.raw()) });
        unsafe {
            pango_layout_set_font_description(layout.0, desc.0);
            pango_layout_set_text(layout.0, text.as_ptr(), -1);
        }
        layout
    }

    pub fn extents(&self) -> Extents {
        let mut ink = Rectangle::default();
        let mut logical = Rectangle::default();
        unsafe { pango_layout_get_extents(self.0, &mut ink, &mut logical) };
        Extents {
            ink,
            logical,
            baseline: unsafe { pango_layout_get_baseline(self.0) },
        }
    }

    pub fn font_ascent_descent(&self, desc: &FontDescription) -> Option<(c_int, c_int)> {
        let context = unsafe { pango_layout_get_context(self.0) };
        let metrics = unsafe { pango_context_get_metrics(context, desc.0, core::ptr::null()) };
        if metrics.is_null() {
            return None;
        }
        let found = unsafe {
            (
                pango_font_metrics_get_ascent(metrics),
                pango_font_metrics_get_descent(metrics),
            )
        };
        unsafe { pango_font_metrics_unref(metrics) };
        Some(found)
    }

    pub fn show(&self, cr: Context) {
        unsafe { pango_cairo_show_layout(cr.raw(), self.0) };
    }

    pub fn path(&self, cr: Context) {
        unsafe { pango_cairo_layout_path(cr.raw(), self.0) };
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0) };
    }
}
