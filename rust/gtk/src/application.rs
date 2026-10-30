//! Southstar — the GtkApplication handle: construction, its activate signal and the main run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};

use southstar_gtk_ffi as sys;

use crate::Object;
use crate::signal;

pub use sys::APPLICATION_NON_UNIQUE;

#[derive(Clone, PartialEq, Eq)]
pub struct Application(Object);

impl Application {
    pub fn new(id: &CStr, flags: c_uint) -> Application {
        // SAFETY: id is NUL-terminated; GTK checks the id and flags and returns a new reference.
        let app = unsafe { sys::gtk_application_new(id.as_ptr(), flags) };
        // SAFETY: a GtkApplication is not floating and the constructor's reference is handed over.
        Application(unsafe { Object::from_full(app) })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn object(&self) -> &Object {
        &self.0
    }

    pub fn connect_activate(&self, f: impl Fn(&Application) + 'static) {
        let weak = self.0.downgrade();
        signal::connect_plain(&self.0, c"activate", move || {
            if let Some(app) = weak.upgrade() {
                f(&Application(app));
            }
        });
    }

    pub fn run(&self) -> c_int {
        // SAFETY: self is a live GApplication; an empty argument vector is passed as argc 0 and a null argv.
        unsafe { sys::g_application_run(self.as_ptr(), 0, core::ptr::null_mut()) }
    }
}
