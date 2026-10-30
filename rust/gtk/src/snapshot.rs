//! Southstar — a drawing area whose snapshot is a Rust closure, the GtkSnapshot it records into, and the textures and cursors it draws with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;
use std::rc::Rc;
use std::sync::OnceLock;

use southstar_cairo::Context;
use southstar_gtk_ffi::{self as sys, GBytes, GdkRgba, GrapheneRect, GtkSnapshot, GtkWidget};

use crate::event::borrow_object;
use crate::event::sealed::Wrap;
use crate::viewport::drop_box;
use crate::{Object, Widget};

unsafe extern "C" {
    fn g_bytes_new_with_free_func(
        data: *const c_void,
        size: usize,
        free_func: sys::GDestroyNotify,
        user_data: *mut c_void,
    ) -> *mut GBytes;
}

#[derive(Clone, Copy)]
pub struct SnapshotRef<'a> {
    raw: NonNull<GtkSnapshot>,
    _call: PhantomData<&'a GtkSnapshot>,
}

impl<'a> SnapshotRef<'a> {
    /// # Safety
    /// `snapshot` is null or a GtkSnapshot that stays live, and is used only on this thread, for `'a`.
    pub unsafe fn from_raw(snapshot: *mut GtkSnapshot) -> Option<SnapshotRef<'a>> {
        NonNull::new(snapshot).map(|raw| SnapshotRef {
            raw,
            _call: PhantomData,
        })
    }

    pub fn as_ptr(&self) -> *mut GtkSnapshot {
        self.raw.as_ptr()
    }

    pub fn save(&self) {
        // SAFETY: self is a live snapshot.
        unsafe { sys::gtk_snapshot_save(self.as_ptr()) };
    }

    pub fn restore(&self) {
        // SAFETY: self is a live snapshot; GTK warns rather than misbehaves on an unmatched restore.
        unsafe { sys::gtk_snapshot_restore(self.as_ptr()) };
    }

    pub fn translate(&self, x: f32, y: f32) {
        let point = sys::GraphenePoint { x, y };
        // SAFETY: self is a live snapshot and point outlives the call.
        unsafe { sys::gtk_snapshot_translate(self.as_ptr(), &point) };
    }

    pub fn push_clip(&self, bounds: &GrapheneRect) {
        // SAFETY: self is a live snapshot and bounds outlives the call.
        unsafe { sys::gtk_snapshot_push_clip(self.as_ptr(), bounds) };
    }

    pub fn pop(&self) {
        // SAFETY: self is a live snapshot; GTK warns rather than misbehaves on an unmatched pop.
        unsafe { sys::gtk_snapshot_pop(self.as_ptr()) };
    }

    pub fn append_color(&self, color: &GdkRgba, bounds: &GrapheneRect) {
        // SAFETY: self is a live snapshot; color and bounds outlive the call.
        unsafe { sys::gtk_snapshot_append_color(self.as_ptr(), color, bounds) };
    }

    pub fn append_texture(&self, texture: &Texture, bounds: &GrapheneRect) {
        // SAFETY: self is a live snapshot and texture a live texture the snapshot references.
        unsafe { sys::gtk_snapshot_append_texture(self.as_ptr(), texture.as_ptr(), bounds) };
    }

    pub fn append_cairo(&self, bounds: &GrapheneRect) -> Option<Context> {
        // SAFETY: self is a live snapshot and bounds outlives the call.
        let cr = unsafe { sys::gtk_snapshot_append_cairo(self.as_ptr(), bounds) };
        // SAFETY: GTK returns a new cairo_t reference that the Context takes over on this thread.
        unsafe { Context::from_raw_full(cr.cast()) }
    }
}

type SnapshotFn = Rc<dyn Fn(&Widget, SnapshotRef<'_>)>;

const SNAPSHOT_FN: &CStr = c"snapshot-func";

fn snapshot_area_type() -> sys::GType {
    static TYPE: OnceLock<sys::GType> = OnceLock::new();
    *TYPE.get_or_init(|| {
        let mut query = sys::GTypeQuery {
            type_: 0,
            type_name: core::ptr::null(),
            class_size: 0,
            instance_size: 0,
        };
        // SAFETY: the GTK type getter has no preconditions.
        let parent = unsafe { sys::gtk_drawing_area_get_type() };
        // SAFETY: parent is a registered type and query a writable GTypeQuery.
        unsafe { sys::g_type_query(parent, &mut query) };
        // SAFETY: the subclass adds no fields, so it takes the parent's class and instance sizes.
        unsafe {
            sys::g_type_register_static_simple(
                parent,
                c"SouthstarSnapshotArea".as_ptr(),
                query.class_size,
                Some(snapshot_area_class_init),
                query.instance_size,
                None,
                0,
            )
        }
    })
}

unsafe extern "C" fn snapshot_area_class_init(klass: *mut c_void, _data: *mut c_void) {
    // SAFETY: GLib passes the class of a GtkDrawingArea subclass, which starts with GtkWidgetClass.
    unsafe { (*klass.cast::<sys::GtkWidgetClass>()).snapshot = Some(snapshot_area_snapshot) };
}

unsafe extern "C" fn snapshot_area_snapshot(widget: *mut GtkWidget, snapshot: *mut GtkSnapshot) {
    // SAFETY: GTK calls the vfunc on a live widget.
    let widget = Widget(unsafe { borrow_object(widget.cast()) });
    let Some(f) = widget.object().data::<SnapshotFn>(SNAPSHOT_FN) else {
        return;
    };
    // SAFETY: GTK passes the snapshot being recorded, live for this call on the GTK thread.
    if let Some(snapshot) = unsafe { SnapshotRef::from_raw(snapshot) } {
        f(&widget, snapshot);
    }
}

impl Widget {
    pub fn new_snapshot_area() -> Widget {
        // SAFETY: the type is a registered GtkDrawingArea subclass with no construct properties; the list ends at the null name.
        Widget::adopt(unsafe { sys::g_object_new(snapshot_area_type(), core::ptr::null()) }.cast())
    }

    pub fn set_snapshot_func(&self, f: impl Fn(&Widget, SnapshotRef<'_>) + 'static) {
        let f: SnapshotFn = Rc::new(f);
        self.object().set_data(SNAPSHOT_FN, Some(f));
    }

    pub fn clear_snapshot_func(&self) {
        self.object().set_data::<SnapshotFn>(SNAPSHOT_FN, None);
    }

    pub fn set_cursor(&self, cursor: Option<&Cursor>) {
        let cursor = cursor.map_or(core::ptr::null_mut(), Cursor::as_ptr);
        // SAFETY: self is a live widget and cursor null or a live cursor the widget references.
        unsafe { sys::gtk_widget_set_cursor(self.as_ptr(), cursor) };
    }

    pub fn set_cursor_from_name(&self, name: Option<&CStr>) {
        let name = name.map_or(core::ptr::null(), CStr::as_ptr);
        // SAFETY: self is a live widget and name null or NUL-terminated.
        unsafe { sys::gtk_widget_set_cursor_from_name(self.as_ptr(), name) };
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Texture(Object);

impl Wrap for Texture {
    fn wrap(object: Object) -> Texture {
        Texture(object)
    }
}

fn texture_fits(width: c_int, height: c_int, stride: usize, len: usize) -> bool {
    let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
        return false;
    };
    let row = w.saturating_mul(4);
    w > 0
        && h > 0
        && stride >= row
        && (h - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(row))
            .is_some_and(|n| n <= len)
}

impl Texture {
    fn from_gbytes(width: c_int, height: c_int, bytes: *mut GBytes, stride: usize) -> Texture {
        // SAFETY: bytes holds at least the rows texture_fits checked; the texture takes its own reference to it.
        let texture = unsafe {
            sys::gdk_memory_texture_new(width, height, sys::MEMORY_DEFAULT, bytes, stride)
        };
        // SAFETY: releases the creating reference; the texture keeps the bytes alive.
        unsafe { sys::g_bytes_unref(bytes) };
        // SAFETY: gdk_memory_texture_new returns a new, non-floating reference.
        Texture(unsafe { Object::from_full(texture.cast()) })
    }

    pub fn from_bytes(width: c_int, height: c_int, data: &[u8], stride: usize) -> Option<Texture> {
        if !texture_fits(width, height, stride, data.len()) {
            return None;
        }
        // SAFETY: data is readable for its length; GLib copies it.
        let bytes = unsafe { sys::g_bytes_new(data.as_ptr().cast(), data.len()) };
        Some(Texture::from_gbytes(width, height, bytes, stride))
    }

    pub fn from_vec(width: c_int, height: c_int, data: Vec<u8>, stride: usize) -> Option<Texture> {
        if !texture_fits(width, height, stride, data.len()) {
            return None;
        }
        let data = Box::new(data);
        let (ptr, len) = (data.as_ptr(), data.len());
        let owner = Box::into_raw(data).cast::<c_void>();
        // SAFETY: the boxed Vec's buffer stays put and alive until drop_box frees the box with the GBytes.
        let bytes = unsafe {
            g_bytes_new_with_free_func(ptr.cast(), len, Some(drop_box::<Vec<u8>>), owner)
        };
        Some(Texture::from_gbytes(width, height, bytes, stride))
    }

    pub fn as_ptr(&self) -> *mut sys::GdkTexture {
        self.0.as_ptr().cast()
    }

    pub fn into_raw(self) -> *mut sys::GdkTexture {
        let raw = self.as_ptr();
        core::mem::forget(self);
        raw
    }

    pub fn width(&self) -> c_int {
        // SAFETY: self is a live texture.
        unsafe { sys::gdk_texture_get_width(self.as_ptr()) }
    }

    pub fn height(&self) -> c_int {
        // SAFETY: self is a live texture.
        unsafe { sys::gdk_texture_get_height(self.as_ptr()) }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Cursor(Object);

impl Wrap for Cursor {
    fn wrap(object: Object) -> Cursor {
        Cursor(object)
    }
}

impl Cursor {
    fn own(cursor: *mut sys::GdkCursor) -> Option<Cursor> {
        // SAFETY: GDK cursor constructors return null or a new, non-floating reference.
        (!cursor.is_null()).then(|| Cursor(unsafe { Object::from_full(cursor.cast()) }))
    }

    fn fallback_ptr(fallback: Option<&Cursor>) -> *mut sys::GdkCursor {
        fallback.map_or(core::ptr::null_mut(), Cursor::as_ptr)
    }

    pub fn from_name(name: &CStr, fallback: Option<&Cursor>) -> Option<Cursor> {
        let fallback = Cursor::fallback_ptr(fallback);
        // SAFETY: name is NUL-terminated and fallback null or a live cursor the new one references.
        Cursor::own(unsafe { sys::gdk_cursor_new_from_name(name.as_ptr(), fallback) })
    }

    pub fn from_texture(
        texture: &Texture,
        hotspot_x: c_int,
        hotspot_y: c_int,
        fallback: Option<&Cursor>,
    ) -> Option<Cursor> {
        let fallback = Cursor::fallback_ptr(fallback);
        // SAFETY: texture is live and fallback null or a live cursor; the cursor references both.
        Cursor::own(unsafe {
            sys::gdk_cursor_new_from_texture(texture.as_ptr(), hotspot_x, hotspot_y, fallback)
        })
    }

    pub fn as_ptr(&self) -> *mut sys::GdkCursor {
        self.0.as_ptr().cast()
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }
}
