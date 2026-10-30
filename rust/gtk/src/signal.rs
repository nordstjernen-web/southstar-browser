//! Southstar — connecting Rust closures to GObject signals through typed trampolines.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_ulong, c_void};

use southstar_gtk_ffi as sys;

use crate::Object;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignalHandlerId(c_ulong);

impl SignalHandlerId {
    pub fn disconnect(self, object: &Object) {
        // SAFETY: object is a live GObject; GLib warns rather than misbehaves on a stale id.
        unsafe { sys::g_signal_handler_disconnect(object.as_ptr(), self.0) };
    }
}

unsafe extern "C" fn plain<F: Fn() + 'static>(_instance: *mut c_void, data: *mut c_void) {
    // SAFETY: data is the boxed F connect_plain registered; GLib keeps it until drop_closure runs.
    let f = unsafe { &*data.cast::<F>() };
    f();
}

unsafe extern "C" fn drop_closure<F>(data: *mut c_void, _closure: *mut c_void) {
    // SAFETY: data came from Box::into_raw in connect_plain and GLib calls this once.
    drop(unsafe { Box::from_raw(data.cast::<F>()) });
}

pub(crate) fn connect_plain<F: Fn() + 'static>(
    object: &Object,
    signal: &CStr,
    f: F,
) -> SignalHandlerId {
    let data = Box::into_raw(Box::new(f)).cast::<c_void>();
    let callback = plain::<F> as unsafe extern "C" fn(*mut c_void, *mut c_void);
    // SAFETY: the signal's handlers take (instance, user_data) and return nothing, matching `plain`; GLib casts it back before calling.
    let callback = unsafe {
        core::mem::transmute::<unsafe extern "C" fn(*mut c_void, *mut c_void), sys::GCallback>(
            callback,
        )
    };
    // SAFETY: object is live, signal is NUL-terminated and drop_closure frees data when the handler goes away.
    let id = unsafe {
        sys::g_signal_connect_data(
            object.as_ptr(),
            signal.as_ptr(),
            callback,
            data,
            Some(drop_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}
