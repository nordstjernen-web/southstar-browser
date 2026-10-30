//! Southstar — reference-counted GTK widget handles whose methods are safe to call from shell code.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

pub use southstar_gtk_ffi as sys;
use southstar_gtk_ffi::GtkWidget;

mod action;
mod application;
mod entry;
mod event;
mod ffi;
mod layout;
mod menu;
mod notebook;
mod object;
mod paintable;
mod popover;
mod signal;
mod snapshot;
mod style;
mod viewport;
mod widgets;

pub use action::{ApplicationWindow, SimpleAction, default_settings};
pub use application::{APPLICATION_NON_UNIQUE, Application};
pub use event::{
    DropTarget, EventController, EventControllerFocus, EventControllerKey, EventControllerMotion,
    EventControllerScroll, GestureClick, GestureDrag, IMContext, ObjectType, from_borrowed,
    keyval_name, keyval_to_lower, keyval_to_unicode,
};
pub use layout::{CustomLayout, HeaderBar, WindowControls, set_custom_layout};
pub use menu::{Menu, MenuItem};
pub use notebook::{Notebook, Overlay, Separator, Spinner};
pub use object::{Object, WeakObject};
pub use paintable::{
    Entry, MenuButton, Paintable, PaintableImpl, pixbuf_loads_format, resource_bytes,
};
pub use popover::Popover;
pub use signal::SignalHandlerId;
pub use snapshot::{Cursor, SnapshotRef, Texture};
pub use style::{CssProvider, Display, IconTheme, STYLE_PROVIDER_PRIORITY_APPLICATION};
pub use viewport::{Adjustment, Surface};
pub use widgets::*;

pub use sys::{GtkOrientation, ORIENTATION_HORIZONTAL, ORIENTATION_VERTICAL};

#[derive(Clone, PartialEq, Eq)]
pub struct Widget(Object);

impl Widget {
    /// # Safety
    /// `widget` is null or a live GtkWidget; the new handle takes its own (sunk) reference.
    pub unsafe fn from_ptr(widget: *mut GtkWidget) -> Option<Widget> {
        // SAFETY: the caller passes null or a live GtkWidget, which is a GObject.
        unsafe { Object::from_ptr(widget.cast()) }.map(Widget)
    }

    fn adopt(widget: *mut GtkWidget) -> Widget {
        // SAFETY: GTK constructors return a new widget and never null.
        unsafe { Widget::from_ptr(widget) }.expect("GTK constructors never return null")
    }

    pub fn as_ptr(&self) -> *mut GtkWidget {
        self.0.as_ptr().cast()
    }

    pub fn object(&self) -> &Object {
        &self.0
    }

    pub fn downgrade(&self) -> WeakWidget {
        WeakWidget(self.0.downgrade())
    }

    fn is_a(&self, type_: sys::GType) -> bool {
        self.0.is_a(type_)
    }

    pub fn set_visible(&self, visible: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_visible(self.as_ptr(), visible.into()) };
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_get_visible(self.as_ptr()) != 0 }
    }

    pub fn set_sensitive(&self, sensitive: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_sensitive(self.as_ptr(), sensitive.into()) };
    }

    pub fn add_css_class(&self, class: &CStr) {
        // SAFETY: self is a live widget and class is NUL-terminated.
        unsafe { sys::gtk_widget_add_css_class(self.as_ptr(), class.as_ptr()) };
    }

    pub fn remove_css_class(&self, class: &CStr) {
        // SAFETY: self is a live widget and class is NUL-terminated.
        unsafe { sys::gtk_widget_remove_css_class(self.as_ptr(), class.as_ptr()) };
    }

    pub fn set_tooltip_text(&self, text: Option<&CStr>) {
        let text = text.map_or(core::ptr::null(), CStr::as_ptr);
        // SAFETY: self is a live widget and text is null or NUL-terminated.
        unsafe { sys::gtk_widget_set_tooltip_text(self.as_ptr(), text) };
    }

    pub fn set_hexpand(&self, expand: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_hexpand(self.as_ptr(), expand.into()) };
    }

    pub fn set_vexpand(&self, expand: bool) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_vexpand(self.as_ptr(), expand.into()) };
    }

    pub fn set_halign(&self, align: sys::GtkAlign) {
        // SAFETY: self is a live widget; GTK checks the enum value.
        unsafe { sys::gtk_widget_set_halign(self.as_ptr(), align) };
    }

    pub fn set_valign(&self, align: sys::GtkAlign) {
        // SAFETY: self is a live widget; GTK checks the enum value.
        unsafe { sys::gtk_widget_set_valign(self.as_ptr(), align) };
    }

    pub fn set_margins(&self, start: c_int, end: c_int, top: c_int, bottom: c_int) {
        let w = self.as_ptr();
        // SAFETY: w is a live widget.
        unsafe { sys::gtk_widget_set_margin_start(w, start) };
        // SAFETY: w is a live widget.
        unsafe { sys::gtk_widget_set_margin_end(w, end) };
        // SAFETY: w is a live widget.
        unsafe { sys::gtk_widget_set_margin_top(w, top) };
        // SAFETY: w is a live widget.
        unsafe { sys::gtk_widget_set_margin_bottom(w, bottom) };
    }

    pub fn set_size_request(&self, width: c_int, height: c_int) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_set_size_request(self.as_ptr(), width, height) };
    }

    pub fn queue_draw(&self) {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_queue_draw(self.as_ptr()) };
    }

    pub fn grab_focus(&self) -> bool {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_grab_focus(self.as_ptr()) != 0 }
    }

    pub fn width(&self) -> c_int {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_get_width(self.as_ptr()) }
    }

    pub fn height(&self) -> c_int {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_get_height(self.as_ptr()) }
    }

    pub fn first_child(&self) -> Option<Widget> {
        // SAFETY: self is a live widget.
        let widget = unsafe { sys::gtk_widget_get_first_child(self.as_ptr()) };
        // SAFETY: GTK returned null or a live widget in self's tree.
        unsafe { Widget::from_ptr(widget) }
    }

    pub fn next_sibling(&self) -> Option<Widget> {
        // SAFETY: self is a live widget.
        let widget = unsafe { sys::gtk_widget_get_next_sibling(self.as_ptr()) };
        // SAFETY: GTK returned null or a live widget in self's tree.
        unsafe { Widget::from_ptr(widget) }
    }

    pub fn children(&self) -> impl Iterator<Item = Widget> {
        core::iter::successors(self.first_child(), Widget::next_sibling)
    }

    pub fn connect_destroy(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(&self.0, c"destroy", f)
    }
}

impl From<Widget> for Object {
    fn from(widget: Widget) -> Object {
        widget.0
    }
}

#[derive(Clone)]
pub struct WeakWidget(WeakObject);

impl WeakWidget {
    pub fn upgrade(&self) -> Option<Widget> {
        self.0.upgrade().map(Widget)
    }
}
