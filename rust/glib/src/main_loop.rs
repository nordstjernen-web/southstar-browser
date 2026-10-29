//! Southstar — a counted reference to a GLib main loop on the default context, to run until something quits it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr::{self, NonNull};

unsafe extern "C" {
    fn g_main_loop_new(context: *mut c_void, is_running: c_int) -> *mut c_void;
    fn g_main_loop_ref(main_loop: *mut c_void) -> *mut c_void;
    fn g_main_loop_unref(main_loop: *mut c_void);
    fn g_main_loop_run(main_loop: *mut c_void);
    fn g_main_loop_quit(main_loop: *mut c_void);
    fn g_main_loop_is_running(main_loop: *mut c_void) -> c_int;
    fn g_main_context_pending(context: *mut c_void) -> c_int;
    fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
}

pub fn default_context_pending() -> bool {
    // SAFETY: a null context is the default one, which lives for the whole process.
    unsafe { g_main_context_pending(ptr::null_mut()) != 0 }
}

pub fn default_context_iteration(may_block: bool) -> bool {
    // SAFETY: a null context is the default one, which lives for the whole process.
    unsafe { g_main_context_iteration(ptr::null_mut(), c_int::from(may_block)) != 0 }
}

pub struct MainLoop(NonNull<c_void>);

// SAFETY: a GMainLoop is reference counted atomically, and running, quitting and querying it take its context's lock.
unsafe impl Send for MainLoop {}
// SAFETY: as above; every method only passes the loop to a thread-safe GLib entry point.
unsafe impl Sync for MainLoop {}

impl MainLoop {
    pub fn new() -> MainLoop {
        // SAFETY: a null context selects the default one; the call returns a new loop, never null.
        let raw = unsafe { g_main_loop_new(ptr::null_mut(), 0) };
        MainLoop(NonNull::new(raw).expect("g_main_loop_new"))
    }

    pub fn run(&self) {
        // SAFETY: the loop is live while this reference holds it.
        unsafe { g_main_loop_run(self.0.as_ptr()) };
    }

    pub fn quit(&self) {
        // SAFETY: the loop is live while this reference holds it.
        unsafe { g_main_loop_quit(self.0.as_ptr()) };
    }

    pub fn is_running(&self) -> bool {
        // SAFETY: the loop is live while this reference holds it.
        unsafe { g_main_loop_is_running(self.0.as_ptr()) != 0 }
    }
}

impl Default for MainLoop {
    fn default() -> MainLoop {
        MainLoop::new()
    }
}

impl Clone for MainLoop {
    fn clone(&self) -> MainLoop {
        // SAFETY: the loop is live while this reference holds it; the call adds a reference for the clone.
        unsafe { g_main_loop_ref(self.0.as_ptr()) };
        MainLoop(self.0)
    }
}

impl Drop for MainLoop {
    fn drop(&mut self) {
        // SAFETY: this gives up the one reference this value held.
        unsafe { g_main_loop_unref(self.0.as_ptr()) };
    }
}
