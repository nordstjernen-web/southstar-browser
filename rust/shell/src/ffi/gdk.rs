//! Southstar — the GTK snapshot the page layers draw into, and the textures they draw with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_gtk::SnapshotRef;
use southstar_gtk_ffi::{GdkRgba, GrapheneRect};

pub use southstar_gtk::Texture;

pub trait Canvas {
    fn append_color(&mut self, color: &GdkRgba, bounds: &GrapheneRect);
    fn append_texture(&mut self, texture: &Texture, bounds: &GrapheneRect);
    fn push_clip(&mut self, bounds: &GrapheneRect);
    fn pop(&mut self);
    fn under(&mut self, offset_x: f64, offset_y: f64);
}

pub type Under<'f> = &'f mut dyn FnMut(SnapshotRef<'_>, f64, f64);

pub struct Snapshot<'a, 'f> {
    pub(super) snapshot: SnapshotRef<'a>,
    pub(super) under: Under<'f>,
}

impl Canvas for Snapshot<'_, '_> {
    fn append_color(&mut self, color: &GdkRgba, bounds: &GrapheneRect) {
        self.snapshot.append_color(color, bounds);
    }

    fn append_texture(&mut self, texture: &Texture, bounds: &GrapheneRect) {
        self.snapshot.append_texture(texture, bounds);
    }

    fn push_clip(&mut self, bounds: &GrapheneRect) {
        self.snapshot.push_clip(bounds);
    }

    fn pop(&mut self) {
        self.snapshot.pop();
    }

    fn under(&mut self, offset_x: f64, offset_y: f64) {
        (self.under)(self.snapshot, offset_x, offset_y);
    }
}
