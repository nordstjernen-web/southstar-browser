//! Southstar — owned GDK textures and a GTK snapshot the page layers draw into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::ptr::NonNull;

use southstar_gtk_ffi::{self as gtk, GdkRgba, GdkTexture, GrapheneRect, GtkSnapshot};

pub struct Texture(NonNull<GdkTexture>);

impl Texture {
    pub fn from_bgra(pixels: &[u8], width: i32, rows: i32, stride: usize) -> Option<Texture> {
        unsafe {
            let bytes = gtk::g_bytes_new(pixels.as_ptr().cast(), pixels.len());
            let texture =
                gtk::gdk_memory_texture_new(width, rows, gtk::MEMORY_DEFAULT, bytes, stride);
            gtk::g_bytes_unref(bytes);
            NonNull::new(texture).map(Texture)
        }
    }

    pub fn width(&self) -> i32 {
        unsafe { gtk::gdk_texture_get_width(self.0.as_ptr()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { gtk::gdk_texture_get_height(self.0.as_ptr()) }
    }
}

impl Clone for Texture {
    fn clone(&self) -> Texture {
        unsafe { gtk::g_object_ref(self.0.as_ptr().cast()) };
        Texture(self.0)
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { gtk::g_object_unref(self.0.as_ptr().cast()) };
    }
}

pub trait Canvas {
    fn append_color(&mut self, color: &GdkRgba, bounds: &GrapheneRect);
    fn append_texture(&mut self, texture: &Texture, bounds: &GrapheneRect);
    fn push_clip(&mut self, bounds: &GrapheneRect);
    fn pop(&mut self);
    fn under(&mut self, offset_x: f64, offset_y: f64);
}

pub type Under =
    Option<unsafe extern "C" fn(snapshot: *mut GtkSnapshot, x: f64, y: f64, data: *mut c_void)>;

pub struct Snapshot {
    pub(super) snapshot: *mut GtkSnapshot,
    pub(super) under: Under,
    pub(super) under_data: *mut c_void,
}

impl Canvas for Snapshot {
    fn append_color(&mut self, color: &GdkRgba, bounds: &GrapheneRect) {
        unsafe { gtk::gtk_snapshot_append_color(self.snapshot, color, bounds) };
    }

    fn append_texture(&mut self, texture: &Texture, bounds: &GrapheneRect) {
        unsafe { gtk::gtk_snapshot_append_texture(self.snapshot, texture.0.as_ptr(), bounds) };
    }

    fn push_clip(&mut self, bounds: &GrapheneRect) {
        unsafe { gtk::gtk_snapshot_push_clip(self.snapshot, bounds) };
    }

    fn pop(&mut self) {
        unsafe { gtk::gtk_snapshot_pop(self.snapshot) };
    }

    fn under(&mut self, offset_x: f64, offset_y: f64) {
        if let Some(under) = self.under {
            unsafe { under(self.snapshot, offset_x, offset_y, self.under_data) };
        }
    }
}
