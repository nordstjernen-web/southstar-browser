//! Southstar — popovers attached to a widget, the scrolled list inside one, and parenting a widget by hand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_gtk_ffi as sys;

use crate::signal::{self, SignalHandlerId};
use crate::{ScrolledWindow, Widget};

crate::widgets::widget_class!(Popover => sys::gtk_popover_get_type);

impl Popover {
    pub fn new() -> Popover {
        // SAFETY: gtk_popover_new returns a new floating widget.
        Popover(Widget::adopt(unsafe { sys::gtk_popover_new() }))
    }

    pub fn static_type() -> sys::GType {
        // SAFETY: the type getter registers the class if needed and has no preconditions.
        unsafe { sys::gtk_popover_get_type() }
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkPopover and child a live widget; the popover takes its own reference.
        unsafe { sys::gtk_popover_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn set_position(&self, position: c_int) {
        // SAFETY: self is a live GtkPopover; GTK checks the position value.
        unsafe { sys::gtk_popover_set_position(self.as_ptr(), position) };
    }

    pub fn popup(&self) {
        // SAFETY: self is a live GtkPopover.
        unsafe { sys::gtk_popover_popup(self.as_ptr()) };
    }

    pub fn popdown(&self) {
        // SAFETY: self is a live GtkPopover.
        unsafe { sys::gtk_popover_popdown(self.as_ptr()) };
    }

    pub fn connect_closed(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"closed", f)
    }
}

impl Default for Popover {
    fn default() -> Popover {
        Popover::new()
    }
}

impl ScrolledWindow {
    pub fn set_policy(&self, horizontal: c_int, vertical: c_int) {
        // SAFETY: self is a live GtkScrolledWindow; GTK checks the policy values.
        unsafe { sys::gtk_scrolled_window_set_policy(self.as_ptr(), horizontal, vertical) };
    }

    pub fn set_propagate_natural_height(&self, propagate: bool) {
        // SAFETY: self is a live GtkScrolledWindow.
        unsafe {
            sys::gtk_scrolled_window_set_propagate_natural_height(self.as_ptr(), propagate.into())
        };
    }

    pub fn set_max_content_height(&self, height: c_int) {
        // SAFETY: self is a live GtkScrolledWindow.
        unsafe { sys::gtk_scrolled_window_set_max_content_height(self.as_ptr(), height) };
    }
}

impl Widget {
    pub fn parent(&self) -> Option<Widget> {
        // SAFETY: self is a live widget.
        let parent = unsafe { sys::gtk_widget_get_parent(self.as_ptr()) };
        // SAFETY: GTK returned null or the live parent holding self.
        unsafe { Widget::from_ptr(parent) }
    }

    pub fn unparent(&self) {
        // SAFETY: self is a live widget with a parent; this handle's reference keeps it valid once the parent lets go.
        unsafe { sys::gtk_widget_unparent(self.as_ptr()) };
    }
}
