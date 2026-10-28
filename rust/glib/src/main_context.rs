//! Southstar — a reference to a GLib main context and the deferred, always-asynchronous hand-off of a closure to the thread that iterates it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr::NonNull;

use crate::GDestroyNotify;

type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> c_int;
type Deferred = Box<dyn FnOnce() + Send>;

const PRIORITY_DEFAULT: c_int = 0;

unsafe extern "C" {
    fn g_main_context_ref_thread_default() -> *mut c_void;
    fn g_main_context_ref(context: *mut c_void) -> *mut c_void;
    fn g_main_context_unref(context: *mut c_void);
    fn g_idle_source_new() -> *mut c_void;
    fn g_source_set_priority(source: *mut c_void, priority: c_int);
    fn g_source_set_callback(
        source: *mut c_void,
        func: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    );
    fn g_source_attach(source: *mut c_void, context: *mut c_void) -> u32;
    fn g_source_unref(source: *mut c_void);
}

pub struct MainContext(NonNull<c_void>);

// SAFETY: a GMainContext is reference counted atomically and every one of its entry points takes its own lock.
unsafe impl Send for MainContext {}
// SAFETY: as above; a shared reference only ever refs, unrefs or attaches a source, all of which are thread-safe.
unsafe impl Sync for MainContext {}

unsafe extern "C" fn run_deferred(data: *mut c_void) -> c_int {
    // SAFETY: data is the slot defer boxed for this source, and the source dispatches once.
    let slot = unsafe { &mut *data.cast::<Option<Deferred>>() };
    if let Some(task) = slot.take() {
        task();
    }
    0
}

unsafe extern "C" fn drop_deferred(data: *mut c_void) {
    // SAFETY: GLib hands back the slot defer boxed, exactly once, when the source is destroyed.
    drop(unsafe { Box::from_raw(data.cast::<Option<Deferred>>()) });
}

impl MainContext {
    pub fn thread_default() -> MainContext {
        // SAFETY: g_main_context_ref_thread_default has no preconditions and returns a new reference, never null.
        let raw = unsafe { g_main_context_ref_thread_default() };
        MainContext(NonNull::new(raw).expect("g_main_context_ref_thread_default"))
    }

    pub fn defer(&self, task: impl FnOnce() + Send + 'static) {
        let slot: Box<Option<Deferred>> = Box::new(Some(Box::new(task)));
        // SAFETY: g_idle_source_new has no preconditions and returns a new source.
        let source = unsafe { g_idle_source_new() };
        // SAFETY: the source is live and owned here.
        unsafe { g_source_set_priority(source, PRIORITY_DEFAULT) };
        // SAFETY: the source takes ownership of the leaked slot and frees it with drop_deferred.
        unsafe {
            g_source_set_callback(
                source,
                run_deferred,
                Box::into_raw(slot).cast(),
                Some(drop_deferred),
            )
        };
        // SAFETY: the context is live, and attaching takes the context's own reference to the source.
        unsafe { g_source_attach(source, self.0.as_ptr()) };
        // SAFETY: this gives up the creating reference; the context keeps its own until the source dispatched.
        unsafe { g_source_unref(source) };
    }
}

impl Clone for MainContext {
    fn clone(&self) -> MainContext {
        // SAFETY: the handle holds a reference to a live context, and the copy owns the new one.
        unsafe { g_main_context_ref(self.0.as_ptr()) };
        MainContext(self.0)
    }
}

impl Drop for MainContext {
    fn drop(&mut self) {
        // SAFETY: the handle owns one reference, given up here.
        unsafe { g_main_context_unref(self.0.as_ptr()) };
    }
}
