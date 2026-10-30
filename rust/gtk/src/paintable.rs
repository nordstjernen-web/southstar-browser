//! Southstar — a GdkPaintable (and GtkSymbolicPaintable) whose drawing is a Rust value, plus the icon setters and resource lookups that take one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_double, c_int, c_uint, c_void};
use std::sync::OnceLock;

use southstar_cairo::{Context, ContextRef};
use southstar_gtk_ffi::{self as sys, GdkPaintable, GdkRgba, GtkSnapshot};

use crate::{Button, Image, Object, Widget, Window};

pub trait PaintableImpl: 'static {
    fn intrinsic_width(&self) -> c_int;
    fn intrinsic_height(&self) -> c_int;
    fn snapshot(&self, cr: ContextRef<'_>, width: f64, height: f64, colors: &[GdkRgba]);
}

#[repr(C)]
struct Instance {
    parent: sys::GObject,
    imp: *mut Box<dyn PaintableImpl>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Paintable(Object);

impl Paintable {
    pub fn new(imp: impl PaintableImpl) -> Paintable {
        // SAFETY: the type is a registered GObject subclass with no construct properties; the list ends at the null name.
        let raw = unsafe { sys::g_object_new(paintable_type(), core::ptr::null()) };
        // SAFETY: g_object_new returned a live instance of the paintable type.
        let object = unsafe { Object::from_ptr(raw) }.expect("g_object_new never returns null");
        // SAFETY: the handle holds its own reference, so the constructor's reference is released here.
        unsafe { sys::g_object_unref(raw) };
        let imp: Box<dyn PaintableImpl> = Box::new(imp);
        let instance = raw.cast::<Instance>();
        // SAFETY: instance is the live, fresh Instance; finalize frees the box this field takes.
        unsafe { (*instance).imp = Box::into_raw(Box::new(imp)) };
        Paintable(object)
    }

    fn as_ptr(&self) -> *mut GdkPaintable {
        self.0.as_ptr().cast()
    }
}

fn paintable_type() -> sys::GType {
    static TYPE: OnceLock<sys::GType> = OnceLock::new();
    *TYPE.get_or_init(register)
}

fn register() -> sys::GType {
    // SAFETY: the GObject type getter has no preconditions.
    let parent = unsafe { sys::g_object_get_type() };
    // SAFETY: the sizes are those of the C class and of Instance, whose first field is the parent instance.
    let ty = unsafe {
        sys::g_type_register_static_simple(
            parent,
            c"SouthstarPaintable".as_ptr(),
            size_of::<sys::GObjectClass>() as c_uint,
            Some(class_init),
            size_of::<Instance>() as c_uint,
            None,
            0,
        )
    };
    let paintable = sys::GInterfaceInfo {
        interface_init: Some(paintable_init),
        interface_finalize: None,
        interface_data: core::ptr::null_mut(),
    };
    // SAFETY: the GDK type getter has no preconditions.
    let paintable_iface = unsafe { sys::gdk_paintable_get_type() };
    // SAFETY: ty is the new type and paintable_init fills a GdkPaintableInterface.
    unsafe { sys::g_type_add_interface_static(ty, paintable_iface, &paintable) };
    let symbolic = sys::GInterfaceInfo {
        interface_init: Some(symbolic_init),
        interface_finalize: None,
        interface_data: core::ptr::null_mut(),
    };
    // SAFETY: the GTK type getter has no preconditions.
    let symbolic_iface = unsafe { sys::gtk_symbolic_paintable_get_type() };
    // SAFETY: ty is the new type and symbolic_init fills a GtkSymbolicPaintableInterface.
    unsafe { sys::g_type_add_interface_static(ty, symbolic_iface, &symbolic) };
    ty
}

unsafe extern "C" fn class_init(klass: *mut c_void, _data: *mut c_void) {
    // SAFETY: GLib passes the class of a GObject subclass, which starts with GObjectClass.
    unsafe { (*klass.cast::<sys::GObjectClass>()).finalize = Some(finalize) };
}

unsafe extern "C" fn finalize(object: *mut sys::GObject) {
    let instance = object.cast::<Instance>();
    // SAFETY: GLib finalizes an instance of this type exactly once, while it is still valid.
    let imp = unsafe { (*instance).imp };
    if !imp.is_null() {
        // SAFETY: imp came from Box::into_raw in Paintable::new and is released only here.
        drop(unsafe { Box::from_raw(imp) });
    }
    // SAFETY: the GObject type getter has no preconditions.
    let object_type = unsafe { sys::g_object_get_type() };
    // SAFETY: the GObject class is always registered and referenced while one of its subclasses lives.
    let parent = unsafe { sys::g_type_class_peek(object_type) };
    // SAFETY: parent is the GObjectClass, which GLib never frees.
    if let Some(parent_finalize) = unsafe { (*parent.cast::<sys::GObjectClass>()).finalize } {
        // SAFETY: chaining up to the parent finalizer with the same live object.
        unsafe { parent_finalize(object) };
    }
}

unsafe extern "C" fn paintable_init(iface: *mut c_void, _data: *mut c_void) {
    let iface = iface.cast::<sys::GdkPaintableInterface>();
    // SAFETY: GLib passes the GdkPaintableInterface vtable being initialized for this type.
    let iface = unsafe { &mut *iface };
    iface.snapshot = Some(snapshot);
    iface.get_flags = Some(flags);
    iface.get_intrinsic_width = Some(intrinsic_width);
    iface.get_intrinsic_height = Some(intrinsic_height);
}

unsafe extern "C" fn symbolic_init(iface: *mut c_void, _data: *mut c_void) {
    let iface = iface.cast::<sys::GtkSymbolicPaintableInterface>();
    // SAFETY: GLib passes the GtkSymbolicPaintableInterface vtable being initialized for this type.
    unsafe { (*iface).snapshot_symbolic = Some(snapshot_symbolic) };
}

unsafe fn imp<'a>(paintable: *mut GdkPaintable) -> Option<&'a dyn PaintableImpl> {
    // SAFETY: the caller passes a live instance of this type, whose imp is null or set by Paintable::new.
    let imp = unsafe { (*paintable.cast::<Instance>()).imp };
    // SAFETY: imp is null or the boxed implementation, alive until the instance is finalized.
    unsafe { imp.as_ref() }.map(|imp| &**imp)
}

fn draw(
    imp: &dyn PaintableImpl,
    snapshot: *mut GtkSnapshot,
    width: f64,
    height: f64,
    colors: &[GdkRgba],
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let bounds = sys::GrapheneRect::new(0.0, 0.0, width as f32, height as f32);
    // SAFETY: snapshot is the live GtkSnapshot GTK passed to the vfunc.
    let cr = unsafe { sys::gtk_snapshot_append_cairo(snapshot, &bounds) };
    // SAFETY: gtk_snapshot_append_cairo returns a new cairo_t reference the Context takes over on this thread.
    let Some(cr) = (unsafe { Context::from_raw_full(cr.cast()) }) else {
        return;
    };
    imp.snapshot(cr.as_ref(), width, height, colors);
}

unsafe extern "C" fn snapshot(
    paintable: *mut GdkPaintable,
    snapshot: *mut GtkSnapshot,
    width: c_double,
    height: c_double,
) {
    // SAFETY: GTK calls the vfunc on a live instance of this type.
    if let Some(imp) = unsafe { imp(paintable) } {
        draw(imp, snapshot, width, height, &[]);
    }
}

unsafe extern "C" fn snapshot_symbolic(
    paintable: *mut GdkPaintable,
    snapshot: *mut GtkSnapshot,
    width: c_double,
    height: c_double,
    colors: *const GdkRgba,
    n_colors: usize,
) {
    let colors = if colors.is_null() || n_colors == 0 {
        &[]
    } else {
        // SAFETY: GTK passes n_colors colors that stay valid for the call.
        unsafe { core::slice::from_raw_parts(colors, n_colors) }
    };
    // SAFETY: GTK calls the vfunc on a live instance of this type.
    if let Some(imp) = unsafe { imp(paintable) } {
        draw(imp, snapshot, width, height, colors);
    }
}

unsafe extern "C" fn flags(_paintable: *mut GdkPaintable) -> sys::GdkPaintableFlags {
    sys::PAINTABLE_STATIC_SIZE | sys::PAINTABLE_STATIC_CONTENTS
}

unsafe extern "C" fn intrinsic_width(paintable: *mut GdkPaintable) -> c_int {
    // SAFETY: GTK calls the vfunc on a live instance of this type.
    unsafe { imp(paintable) }.map_or(0, PaintableImpl::intrinsic_width)
}

unsafe extern "C" fn intrinsic_height(paintable: *mut GdkPaintable) -> c_int {
    // SAFETY: GTK calls the vfunc on a live instance of this type.
    unsafe { imp(paintable) }.map_or(0, PaintableImpl::intrinsic_height)
}

unsafe extern "C" {
    fn gtk_menu_button_get_type() -> sys::GType;
    fn gtk_entry_get_type() -> sys::GType;
}

crate::widgets::widget_class!(
    MenuButton => gtk_menu_button_get_type,
    Entry => gtk_entry_get_type,
);

impl Image {
    pub fn from_paintable(paintable: &Paintable) -> Image {
        // SAFETY: paintable is live; the constructor returns a new floating widget.
        let image = Widget::adopt(unsafe { sys::gtk_image_new_from_paintable(paintable.as_ptr()) });
        Image::cast(&image).expect("gtk_image_new_from_paintable returns a GtkImage")
    }

    pub fn set_from_paintable(&self, paintable: &Paintable) {
        // SAFETY: self is a live GtkImage and paintable a live paintable the image references.
        unsafe { sys::gtk_image_set_from_paintable(self.as_ptr().cast(), paintable.as_ptr()) };
    }

    pub fn set_from_icon_name(&self, icon: &CStr) {
        // SAFETY: self is a live GtkImage and icon is NUL-terminated.
        unsafe { sys::gtk_image_set_from_icon_name(self.as_ptr().cast(), icon.as_ptr()) };
    }
}

impl Button {
    pub fn new() -> Button {
        // SAFETY: gtk_button_new returns a new floating widget.
        let button = Widget::adopt(unsafe { sys::gtk_button_new() });
        Button::cast(&button).expect("gtk_button_new returns a GtkButton")
    }

    pub fn set_icon_name(&self, icon: &CStr) {
        // SAFETY: self is a live GtkButton and icon is NUL-terminated.
        unsafe { sys::gtk_button_set_icon_name(self.as_ptr().cast(), icon.as_ptr()) };
    }

    pub fn child(&self) -> Option<Widget> {
        // SAFETY: self is a live GtkButton.
        let child = unsafe { sys::gtk_button_get_child(self.as_ptr().cast()) };
        // SAFETY: GTK returned null or the live child the button holds.
        unsafe { Widget::from_ptr(child) }
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkButton and child a live widget; the button takes its own reference.
        unsafe { sys::gtk_button_set_child(self.as_ptr().cast(), child.as_ref().as_ptr()) };
    }
}

impl Default for Button {
    fn default() -> Button {
        Button::new()
    }
}

impl MenuButton {
    pub fn set_icon_name(&self, icon: &CStr) {
        // SAFETY: self is a live GtkMenuButton and icon is NUL-terminated.
        unsafe { sys::gtk_menu_button_set_icon_name(self.as_ptr().cast(), icon.as_ptr()) };
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkMenuButton and child a live widget; the button takes its own reference.
        unsafe { sys::gtk_menu_button_set_child(self.as_ptr().cast(), child.as_ref().as_ptr()) };
    }
}

impl Entry {
    pub fn set_icon_from_icon_name(&self, pos: sys::GtkEntryIconPosition, icon: &CStr) {
        // SAFETY: self is a live GtkEntry, icon is NUL-terminated and GTK checks the position.
        unsafe { sys::gtk_entry_set_icon_from_icon_name(self.as_ptr().cast(), pos, icon.as_ptr()) };
    }

    pub fn set_icon_from_paintable(&self, pos: sys::GtkEntryIconPosition, paintable: &Paintable) {
        // SAFETY: self is a live GtkEntry, paintable is live and GTK checks the position.
        unsafe {
            sys::gtk_entry_set_icon_from_paintable(self.as_ptr().cast(), pos, paintable.as_ptr())
        };
    }
}

impl Window {
    pub fn set_default_icon_name(icon: &CStr) {
        // SAFETY: icon is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_window_set_default_icon_name(icon.as_ptr()) };
    }
}

pub fn resource_bytes(path: &CStr) -> Option<Vec<u8>> {
    // SAFETY: path is NUL-terminated; a null error pointer asks GLib not to report one.
    let bytes = unsafe { sys::g_resources_lookup_data(path.as_ptr(), 0, core::ptr::null_mut()) };
    if bytes.is_null() {
        return None;
    }
    let mut len = 0usize;
    // SAFETY: bytes is the live GBytes the lookup returned.
    let data = unsafe { sys::g_bytes_get_data(bytes, &mut len) };
    let copy = (!data.is_null()).then(|| {
        // SAFETY: GLib returned len readable bytes at data, alive while bytes is referenced.
        unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec()
    });
    // SAFETY: releases the reference the lookup returned.
    unsafe { sys::g_bytes_unref(bytes) };
    copy
}

pub fn pixbuf_loads_format(name: &[u8]) -> bool {
    let mut found = false;
    // SAFETY: gdk_pixbuf_get_formats has no preconditions and returns a list the caller frees.
    let formats = unsafe { sys::gdk_pixbuf_get_formats() };
    let mut l = formats;
    while !l.is_null() && !found {
        // SAFETY: l is a live node of the list GdkPixbuf returned.
        let node = unsafe { &*l };
        // SAFETY: node.data is a GdkPixbufFormat GdkPixbuf owns; the name is a new string.
        let format = unsafe { sys::gdk_pixbuf_format_get_name(node.data.cast()) };
        // SAFETY: format is null or a NUL-terminated string owned here.
        found = !format.is_null() && unsafe { CStr::from_ptr(format) }.to_bytes() == name;
        // SAFETY: format was allocated by GLib for the caller (g_free accepts null).
        unsafe { sys::g_free(format.cast()) };
        l = node.next;
    }
    // SAFETY: frees the list itself; its formats belong to GdkPixbuf.
    unsafe { sys::g_slist_free(formats) };
    found
}
