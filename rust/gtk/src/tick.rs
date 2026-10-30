//! Southstar — Rust closures called on every frame of a widget's frame clock.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_void};
use core::ops::ControlFlow;

use southstar_gtk_ffi::{self as sys, GBoolean, GdkFrameClock, GtkWidget};

use crate::Widget;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TickCallbackId(c_uint);

impl TickCallbackId {
    pub fn remove(self, widget: &Widget) {
        // SAFETY: widget is live; GTK ignores an id it no longer knows.
        unsafe { sys::gtk_widget_remove_tick_callback(widget.as_ptr(), self.0) };
    }
}

unsafe extern "C" fn tick<F: FnMut(&Widget) -> ControlFlow<()> + 'static>(
    widget: *mut GtkWidget,
    _clock: *mut GdkFrameClock,
    data: *mut c_void,
) -> GBoolean {
    // SAFETY: GTK passes the live widget the callback was added to.
    let Some(widget) = (unsafe { Widget::from_ptr(widget) }) else {
        return 0;
    };
    // SAFETY: data is the boxed F add_tick_callback registered; GTK holds the callback across this call, so drop_tick cannot run until it returns, and never calls it reentrantly.
    let f = unsafe { &mut *data.cast::<F>() };
    GBoolean::from(f(&widget).is_continue())
}

unsafe extern "C" fn drop_tick<F>(data: *mut c_void) {
    // SAFETY: data came from Box::into_raw in add_tick_callback and GTK calls this once.
    drop(unsafe { Box::from_raw(data.cast::<F>()) });
}

impl Widget {
    pub fn add_tick_callback<F: FnMut(&Widget) -> ControlFlow<()> + 'static>(
        &self,
        f: F,
    ) -> TickCallbackId {
        let data = Box::into_raw(Box::new(f)).cast::<c_void>();
        // SAFETY: self is live; tick matches GtkTickCallback and drop_tick frees data when the callback is removed.
        let id = unsafe {
            sys::gtk_widget_add_tick_callback(
                self.as_ptr(),
                Some(tick::<F>),
                data,
                Some(drop_tick::<F>),
            )
        };
        TickCallbackId(id)
    }
}
