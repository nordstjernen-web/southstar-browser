//! Southstar — the default display, its icon theme and the CSS providers styling it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};

use southstar_gtk_ffi as sys;

use crate::Object;

pub use sys::STYLE_PROVIDER_PRIORITY_APPLICATION;

#[derive(Clone, PartialEq, Eq)]
pub struct Display(Object);

#[derive(Clone, PartialEq, Eq)]
pub struct IconTheme(Object);

#[derive(Clone, PartialEq, Eq)]
pub struct CssProvider(Object);

impl Display {
    pub fn default_display() -> Option<Display> {
        // SAFETY: gdk_display_get_default has no preconditions and returns null or a display GDK owns.
        let display = unsafe { sys::gdk_display_get_default() };
        // SAFETY: display is null or a live GdkDisplay; the handle takes its own reference.
        unsafe { Object::from_ptr(display) }.map(Display)
    }

    pub fn icon_theme(&self) -> Option<IconTheme> {
        // SAFETY: self is a live GdkDisplay; the theme returned is owned by the display.
        let theme = unsafe { sys::gtk_icon_theme_get_for_display(self.0.as_ptr()) };
        // SAFETY: theme is null or a live GtkIconTheme; the handle takes its own reference.
        unsafe { Object::from_ptr(theme) }.map(IconTheme)
    }

    pub fn add_style_provider(&self, provider: &CssProvider, priority: c_uint) {
        // SAFETY: self is a live GdkDisplay and provider a live GtkStyleProvider; GTK takes its own reference.
        unsafe {
            sys::gtk_style_context_add_provider_for_display(
                self.0.as_ptr(),
                provider.0.as_ptr(),
                priority,
            )
        };
    }
}

impl IconTheme {
    pub fn add_resource_path(&self, path: &CStr) {
        // SAFETY: self is a live GtkIconTheme and path is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_icon_theme_add_resource_path(self.0.as_ptr(), path.as_ptr()) };
    }

    pub fn add_search_path(&self, path: &CStr) {
        // SAFETY: self is a live GtkIconTheme and path is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_icon_theme_add_search_path(self.0.as_ptr(), path.as_ptr()) };
    }
}

impl CssProvider {
    pub fn new() -> CssProvider {
        // SAFETY: gtk_css_provider_new has no preconditions and returns a new reference.
        let provider = unsafe { sys::gtk_css_provider_new() };
        // SAFETY: a GtkCssProvider is not floating and the constructor's reference is handed over.
        CssProvider(unsafe { Object::from_full(provider) })
    }

    pub fn load_from_string(&self, css: &CStr) {
        // SAFETY: self is a live GtkCssProvider and css is NUL-terminated.
        unsafe { sys::gtk_css_provider_load_from_string(self.0.as_ptr(), css.as_ptr()) };
    }
}

impl Default for CssProvider {
    fn default() -> CssProvider {
        CssProvider::new()
    }
}
