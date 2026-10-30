//! Southstar — revealers, frames and overlays, plus the widget's root, clipboard and accessible label and the window's close request.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};

use southstar_gtk_ffi::{self as sys, GBoolean, GType};

use crate::event::{borrow_object, connect_boxed, gcallback};
use crate::signal::SignalHandlerId;
use crate::widgets::widget_class;
use crate::{Widget, Window};

pub use sys::REVEALER_TRANSITION_TYPE_SLIDE_DOWN;

unsafe extern "C" {
    fn gtk_revealer_get_type() -> GType;
    fn gtk_frame_get_type() -> GType;
}

widget_class!(
    Revealer => gtk_revealer_get_type,
    Frame => gtk_frame_get_type,
);

impl Revealer {
    pub fn new() -> Revealer {
        // SAFETY: gtk_revealer_new returns a new floating widget.
        Revealer(Widget::adopt(unsafe { sys::gtk_revealer_new() }))
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkRevealer and child a live widget; the revealer takes its own reference.
        unsafe { sys::gtk_revealer_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn set_transition_type(&self, transition: c_int) {
        // SAFETY: self is a live GtkRevealer; GTK checks the enum value.
        unsafe { sys::gtk_revealer_set_transition_type(self.as_ptr(), transition) };
    }

    pub fn set_reveal_child(&self, reveal: bool) {
        // SAFETY: self is a live GtkRevealer.
        unsafe { sys::gtk_revealer_set_reveal_child(self.as_ptr(), reveal.into()) };
    }

    pub fn reveals_child(&self) -> bool {
        // SAFETY: self is a live GtkRevealer.
        unsafe { sys::gtk_revealer_get_reveal_child(self.as_ptr()) != 0 }
    }
}

impl Default for Revealer {
    fn default() -> Revealer {
        Revealer::new()
    }
}

impl Frame {
    pub fn new() -> Frame {
        // SAFETY: a null label asks for a frame without one; the constructor returns a new floating widget.
        Frame(Widget::adopt(unsafe {
            sys::gtk_frame_new(core::ptr::null())
        }))
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkFrame and child a live widget; the frame takes its own reference.
        unsafe { sys::gtk_frame_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }
}

impl Default for Frame {
    fn default() -> Frame {
        Frame::new()
    }
}

impl Widget {
    pub fn root(&self) -> Option<Widget> {
        // SAFETY: self is a live widget.
        let root = unsafe { sys::gtk_widget_get_root(self.as_ptr()) };
        // SAFETY: GTK returned null or the live toplevel above self.
        (!root.is_null()).then(|| Widget(unsafe { borrow_object(root.cast()) }))
    }

    pub fn set_clipboard_text(&self, text: &CStr) {
        // SAFETY: self is a live widget; the clipboard belongs to its display.
        let clipboard = unsafe { sys::gtk_widget_get_clipboard(self.as_ptr()) };
        // SAFETY: clipboard is live and text is NUL-terminated; GDK copies it.
        unsafe { sys::gdk_clipboard_set_text(clipboard, text.as_ptr()) };
    }
}

unsafe extern "C" fn on_close_request<F: Fn() -> bool + 'static>(
    _window: *mut c_void,
    data: *mut c_void,
) -> GBoolean {
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    GBoolean::from(f())
}

impl Window {
    pub fn set_destroy_with_parent(&self, destroy: bool) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_set_destroy_with_parent(self.as_ptr(), destroy.into()) };
    }

    pub fn connect_close_request<F: Fn() -> bool + 'static>(&self, f: F) -> SignalHandlerId {
        let callback =
            gcallback!(on_close_request::<F> => fn(*mut c_void, *mut c_void) -> GBoolean);
        // SAFETY: close-request takes (window, data) and returns a gboolean, as on_close_request does.
        unsafe { connect_boxed(self.object(), c"close-request", callback, f) }
    }
}
