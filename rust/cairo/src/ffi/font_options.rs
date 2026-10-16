//! Southstar — cairo font options: an owned set and a borrowed view of one another library keeps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

use super::RawFontOptions;
#[cfg(target_os = "macos")]
use crate::types::HintStyle;
use crate::types::{Antialias, HintMetrics, SubpixelOrder};

pub struct FontOptions(NonNull<RawFontOptions>);

#[derive(Clone, Copy)]
pub struct FontOptionsRef<'a> {
    raw: NonNull<RawFontOptions>,
    _options: PhantomData<&'a FontOptions>,
}

impl FontOptions {
    pub fn new() -> FontOptions {
        // SAFETY: cairo_font_options_create takes nothing and returns options the caller owns, a static error object on failure.
        let raw = unsafe { super::cairo_font_options_create() };
        FontOptions(NonNull::new(raw).expect("cairo returns font options, never null"))
    }

    fn raw(&self) -> *mut RawFontOptions {
        self.0.as_ptr()
    }

    pub fn as_raw(&self) -> *const c_void {
        self.raw().cast_const().cast()
    }

    pub fn merge(&mut self, other: FontOptionsRef<'_>) {
        // SAFETY: both option sets are live, and self is borrowed mutably.
        unsafe { super::cairo_font_options_merge(self.raw(), other.raw.as_ptr()) };
    }

    pub fn set_antialias(&mut self, antialias: Antialias) {
        // SAFETY: the options are live, and borrowed mutably.
        unsafe { super::cairo_font_options_set_antialias(self.raw(), antialias as c_int) };
    }

    pub fn set_subpixel_order(&mut self, order: SubpixelOrder) {
        // SAFETY: the options are live, and borrowed mutably.
        unsafe { super::cairo_font_options_set_subpixel_order(self.raw(), order as c_int) };
    }

    pub fn set_hint_metrics(&mut self, metrics: HintMetrics) {
        // SAFETY: the options are live, and borrowed mutably.
        unsafe { super::cairo_font_options_set_hint_metrics(self.raw(), metrics as c_int) };
    }

    #[cfg(target_os = "macos")]
    pub fn set_hint_style(&mut self, style: HintStyle) {
        // SAFETY: the options are live, and borrowed mutably.
        unsafe { super::cairo_font_options_set_hint_style(self.raw(), style as c_int) };
    }
}

impl Default for FontOptions {
    fn default() -> FontOptions {
        FontOptions::new()
    }
}

impl Drop for FontOptions {
    fn drop(&mut self) {
        // SAFETY: the FontOptions owns its options and frees them once, here.
        unsafe { super::cairo_font_options_destroy(self.raw()) };
    }
}

impl<'a> FontOptionsRef<'a> {
    /// # Safety
    ///
    /// `raw` is null or font options that stay alive and unchanged for `'a`.
    pub unsafe fn from_raw(raw: *const c_void) -> Option<FontOptionsRef<'a>> {
        NonNull::new(raw.cast_mut().cast()).map(|raw| FontOptionsRef {
            raw,
            _options: PhantomData,
        })
    }
}
