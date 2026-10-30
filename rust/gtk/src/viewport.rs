//! Southstar — what a scrolling view needs from GTK: adjustments, frame-clock ticks, surface scale, focus, property notifications and clipboard text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_gtk_ffi::{self as sys, GBoolean, GdkFrameClock, GtkWidget};

use crate::event::sealed::Wrap;
use crate::event::{borrow_object, connect_boxed, from_borrowed, gcallback, on_pair};
use crate::signal::{self, SignalHandlerId};
use crate::{Object, Widget};

unsafe extern "C" fn on_notify<F: Fn() + 'static>(
    _object: *mut c_void,
    _pspec: *mut c_void,
    data: *mut c_void,
) {
    // SAFETY: data is the boxed F connect_notify registered; GLib keeps it while the handler lives.
    let f = unsafe { &*data.cast::<F>() };
    f();
}

impl Object {
    pub fn connect_notify<F: Fn() + 'static>(&self, property: &CStr, f: F) -> SignalHandlerId {
        let mut signal = b"notify::".to_vec();
        signal.extend_from_slice(property.to_bytes());
        let signal = CString::new(signal).unwrap_or_default();
        let callback = gcallback!(on_notify::<F> => fn(*mut c_void, *mut c_void, *mut c_void));
        // SAFETY: GObject::notify takes (object, pspec, data) and returns nothing, as on_notify does.
        unsafe { connect_boxed(self, &signal, callback, f) }
    }
}

unsafe extern "C" fn on_tick<F: Fn(&Widget, i64) -> bool + 'static>(
    widget: *mut GtkWidget,
    clock: *mut GdkFrameClock,
    data: *mut c_void,
) -> GBoolean {
    // SAFETY: GTK runs tick callbacks on the live widget they were added to.
    let widget = Widget(unsafe { borrow_object(widget.cast()) });
    let now = if clock.is_null() {
        0
    } else {
        // SAFETY: GTK passes the widget's live frame clock for the call.
        unsafe { sys::gdk_frame_clock_get_frame_time(clock) }
    };
    // SAFETY: data is the boxed F add_tick_callback registered; GTK keeps it until drop_box runs.
    let f = unsafe { &*data.cast::<F>() };
    GBoolean::from(f(&widget, now))
}

pub(crate) unsafe extern "C" fn drop_box<T>(data: *mut c_void) {
    // SAFETY: data came from Box::into_raw of a T and its owner calls this once.
    drop(unsafe { Box::from_raw(data.cast::<T>()) });
}

unsafe extern "C" fn on_text_read<F: FnOnce(Option<Vec<u8>>) + 'static>(
    source: *mut c_void,
    result: *mut c_void,
    data: *mut c_void,
) {
    // SAFETY: data came from Box::into_raw in read_clipboard_text and GIO calls this once.
    let f = unsafe { Box::from_raw(data.cast::<F>()) };
    // SAFETY: GIO passes the clipboard and the result of its text read; a null error asks for none.
    let text = unsafe {
        sys::gdk_clipboard_read_text_finish(source.cast(), result, core::ptr::null_mut())
    };
    let bytes = (!text.is_null()).then(|| {
        // SAFETY: text is a NUL-terminated string the caller owns.
        let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
        // SAFETY: the string was allocated by GLib for the caller and is not used again.
        unsafe { sys::g_free(text.cast()) };
        bytes
    });
    f(bytes);
}

impl Widget {
    pub fn has_focus(&self) -> bool {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_has_focus(self.as_ptr()) != 0 }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_contains(self.as_ptr(), x, y) != 0 }
    }

    pub fn scale_factor(&self) -> c_int {
        // SAFETY: self is a live widget.
        unsafe { sys::gtk_widget_get_scale_factor(self.as_ptr()) }
    }

    pub fn surface(&self) -> Option<Surface> {
        // SAFETY: self is a live widget.
        let native = unsafe { sys::gtk_widget_get_native(self.as_ptr()) };
        if native.is_null() {
            return None;
        }
        // SAFETY: native is the live GtkNative that holds self.
        let surface = unsafe { sys::gtk_native_get_surface(native) };
        // SAFETY: surface is null or the native's live GdkSurface.
        unsafe { from_borrowed(surface.cast()) }
    }

    pub fn connect_realize(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"realize", f)
    }

    pub fn connect_unrealize(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"unrealize", f)
    }

    pub fn connect_resize<F: Fn(&Widget, c_int, c_int) + 'static>(
        &self,
        f: F,
    ) -> Option<SignalHandlerId> {
        // SAFETY: the GTK type getter has no preconditions.
        let drawing_area = unsafe { sys::gtk_drawing_area_get_type() };
        if !self.is_a(drawing_area) {
            return None;
        }
        let callback = gcallback!(on_pair::<Widget, c_int, (), F> => fn(*mut c_void, c_int, c_int, *mut c_void));
        // SAFETY: self is a GtkDrawingArea, whose ::resize takes (area, width, height, data) and returns nothing, as on_pair does for ().
        Some(unsafe { connect_boxed(self.object(), c"resize", callback, f) })
    }

    pub fn add_tick_callback<F: Fn(&Widget, i64) -> bool + 'static>(&self, f: F) -> c_uint {
        let data = Box::into_raw(Box::new(f)).cast::<c_void>();
        // SAFETY: self is a live widget; GTK passes data to on_tick, which reads it as F, and frees it with drop_box.
        unsafe {
            sys::gtk_widget_add_tick_callback(
                self.as_ptr(),
                Some(on_tick::<F>),
                data,
                Some(drop_box::<F>),
            )
        }
    }

    pub fn remove_tick_callback(&self, id: c_uint) {
        // SAFETY: self is a live widget; GTK ignores an id it does not know.
        unsafe { sys::gtk_widget_remove_tick_callback(self.as_ptr(), id) };
    }

    pub fn read_clipboard_text<F: FnOnce(Option<Vec<u8>>) + 'static>(&self, f: F) {
        // SAFETY: self is a live widget; its clipboard belongs to its display.
        let clipboard = unsafe { sys::gtk_widget_get_clipboard(self.as_ptr()) };
        let data = Box::into_raw(Box::new(f)).cast::<c_void>();
        // SAFETY: clipboard is live and the read keeps it referenced; on_text_read takes data back exactly once.
        unsafe {
            sys::gdk_clipboard_read_text_async(
                clipboard,
                core::ptr::null_mut(),
                Some(on_text_read::<F>),
                data,
            )
        };
    }
}

impl Wrap for Widget {
    fn wrap(object: Object) -> Widget {
        Widget(object)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Surface(Object);

impl Wrap for Surface {
    fn wrap(object: Object) -> Surface {
        Surface(object)
    }
}

impl Surface {
    pub fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn object(&self) -> &Object {
        &self.0
    }

    pub fn scale(&self) -> f64 {
        // SAFETY: self is a live surface.
        unsafe { sys::gdk_surface_get_scale(self.as_ptr().cast()) }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Adjustment(Object);

impl Wrap for Adjustment {
    fn wrap(object: Object) -> Adjustment {
        Adjustment(object)
    }
}

impl Adjustment {
    fn as_ptr(&self) -> *mut sys::GtkAdjustment {
        self.0.as_ptr().cast()
    }

    pub fn configure(
        &self,
        value: f64,
        lower: f64,
        upper: f64,
        step_increment: f64,
        page_increment: f64,
        page_size: f64,
    ) {
        // SAFETY: self is a live adjustment.
        unsafe {
            sys::gtk_adjustment_configure(
                self.as_ptr(),
                value,
                lower,
                upper,
                step_increment,
                page_increment,
                page_size,
            )
        };
    }

    pub fn value(&self) -> f64 {
        // SAFETY: self is a live adjustment.
        unsafe { sys::gtk_adjustment_get_value(self.as_ptr()) }
    }

    pub fn set_value(&self, value: f64) {
        // SAFETY: self is a live adjustment.
        unsafe { sys::gtk_adjustment_set_value(self.as_ptr(), value) };
    }

    pub fn upper(&self) -> f64 {
        // SAFETY: self is a live adjustment.
        unsafe { sys::gtk_adjustment_get_upper(self.as_ptr()) }
    }

    pub fn page_size(&self) -> f64 {
        // SAFETY: self is a live adjustment.
        unsafe { sys::gtk_adjustment_get_page_size(self.as_ptr()) }
    }

    pub fn connect_value_changed(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(&self.0, c"value-changed", f)
    }
}
