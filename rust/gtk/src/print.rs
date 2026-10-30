//! Southstar — GtkPrintOperation, its page setup and print context, with the begin-print and draw-page signals taking Rust closures.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_cairo::ContextRef;
use southstar_gtk_ffi::{self as sys, GtkPrintContext, GtkPrintOperation};

use crate::Object;
use crate::Widget;
use crate::event::{borrow_object, connect_boxed, gcallback};
use crate::signal::SignalHandlerId;

pub use sys::{
    PAGE_ORIENTATION_LANDSCAPE, PAGE_ORIENTATION_PORTRAIT, PRINT_OPERATION_ACTION_PRINT_DIALOG,
    UNIT_POINTS,
};

#[repr(C)]
pub(crate) struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}

unsafe extern "C" {
    pub(crate) fn g_error_free(error: *mut GError);
}

pub(crate) fn take_error(error: *mut GError) -> Option<CString> {
    // SAFETY: error is null or the GError a GLib call just reported, owned by the caller.
    let message = unsafe { error.as_ref() }.map(|error| {
        if error.message.is_null() {
            CString::default()
        } else {
            // SAFETY: a GError's message is a NUL-terminated string the error owns.
            unsafe { CStr::from_ptr(error.message) }.to_owned()
        }
    });
    if !error.is_null() {
        // SAFETY: the caller owns the error and does not use it again.
        unsafe { g_error_free(error) };
    }
    message
}

pub struct PageSetup(Object);

impl PageSetup {
    pub fn new() -> PageSetup {
        // SAFETY: gtk_page_setup_new has no preconditions and returns a new, non-floating GObject.
        let setup = unsafe { sys::gtk_page_setup_new() };
        // SAFETY: setup is live and its one reference passes to the handle.
        PageSetup(unsafe { Object::from_full(setup.cast()) })
    }

    pub fn set_custom_paper_size(
        &self,
        name: &CStr,
        display_name: &CStr,
        width: f64,
        height: f64,
        unit: c_int,
    ) {
        // SAFETY: both names are NUL-terminated; GTK copies them and returns null for an invalid unit.
        let paper = unsafe {
            sys::gtk_paper_size_new_custom(
                name.as_ptr(),
                display_name.as_ptr(),
                width,
                height,
                unit,
            )
        };
        if paper.is_null() {
            return;
        }
        // SAFETY: self is a live GtkPageSetup and paper a live paper size, which the setup copies.
        unsafe { sys::gtk_page_setup_set_paper_size(self.as_ptr(), paper) };
        // SAFETY: paper was created above for this call and is not used again.
        unsafe { sys::gtk_paper_size_free(paper) };
    }

    pub fn set_orientation(&self, orientation: c_int) {
        // SAFETY: self is a live GtkPageSetup; GTK stores the enum value.
        unsafe { sys::gtk_page_setup_set_orientation(self.as_ptr(), orientation) };
    }

    fn as_ptr(&self) -> *mut sys::GtkPageSetup {
        self.0.as_ptr().cast()
    }
}

impl Default for PageSetup {
    fn default() -> PageSetup {
        PageSetup::new()
    }
}

#[derive(Clone)]
pub struct PrintOperation(Object);

pub struct PrintContext<'a> {
    raw: NonNull<GtkPrintContext>,
    _call: PhantomData<&'a GtkPrintContext>,
}

impl PrintContext<'_> {
    pub fn width(&self) -> f64 {
        // SAFETY: the context is the live one GTK passed to the signal handler.
        unsafe { sys::gtk_print_context_get_width(self.raw.as_ptr()) }
    }

    pub fn cairo_context(&self) -> Option<ContextRef<'_>> {
        // SAFETY: the context is live; the cairo context belongs to it.
        let cr = unsafe { sys::gtk_print_context_get_cairo_context(self.raw.as_ptr()) };
        // SAFETY: cr is null or the print context's cairo_t, alive while the context is, on the main thread.
        unsafe { ContextRef::from_raw(cr.cast()) }
    }
}

unsafe extern "C" fn on_begin_print<F: Fn(&PrintOperation) + 'static>(
    op: *mut c_void,
    _context: *mut c_void,
    data: *mut c_void,
) {
    // SAFETY: GTK emits the signal on the live operation the handler was connected to.
    let op = PrintOperation(unsafe { borrow_object(op) });
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(&op);
}

unsafe extern "C" fn on_draw_page<F: Fn(&PrintContext<'_>, c_int) + 'static>(
    _op: *mut c_void,
    context: *mut GtkPrintContext,
    page_nr: c_int,
    data: *mut c_void,
) {
    let Some(raw) = NonNull::new(context) else {
        return;
    };
    let context = PrintContext {
        raw,
        _call: PhantomData,
    };
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(&context, page_nr);
}

impl PrintOperation {
    pub fn new() -> PrintOperation {
        // SAFETY: gtk_print_operation_new has no preconditions and returns a new, non-floating GObject.
        let op = unsafe { sys::gtk_print_operation_new() };
        // SAFETY: op is live and its one reference passes to the handle.
        PrintOperation(unsafe { Object::from_full(op.cast()) })
    }

    fn as_ptr(&self) -> *mut GtkPrintOperation {
        self.0.as_ptr().cast()
    }

    pub fn set_default_page_setup(&self, setup: &PageSetup) {
        // SAFETY: both objects are live; the operation takes its own reference to the setup.
        unsafe { sys::gtk_print_operation_set_default_page_setup(self.as_ptr(), setup.as_ptr()) };
    }

    pub fn set_use_full_page(&self, full_page: bool) {
        // SAFETY: self is a live GtkPrintOperation.
        unsafe { sys::gtk_print_operation_set_use_full_page(self.as_ptr(), full_page.into()) };
    }

    pub fn set_unit(&self, unit: c_int) {
        // SAFETY: self is a live GtkPrintOperation; GTK stores the enum value.
        unsafe { sys::gtk_print_operation_set_unit(self.as_ptr(), unit) };
    }

    pub fn set_job_name(&self, name: &CStr) {
        // SAFETY: self is a live GtkPrintOperation and name is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_print_operation_set_job_name(self.as_ptr(), name.as_ptr()) };
    }

    pub fn set_embed_page_setup(&self, embed: bool) {
        // SAFETY: self is a live GtkPrintOperation.
        unsafe { sys::gtk_print_operation_set_embed_page_setup(self.as_ptr(), embed.into()) };
    }

    pub fn set_n_pages(&self, n_pages: c_int) {
        // SAFETY: self is a live GtkPrintOperation; GTK checks the count.
        unsafe { sys::gtk_print_operation_set_n_pages(self.as_ptr(), n_pages) };
    }

    pub fn connect_begin_print<F: Fn(&PrintOperation) + 'static>(&self, f: F) -> SignalHandlerId {
        let callback = gcallback!(on_begin_print::<F> => fn(*mut c_void, *mut c_void, *mut c_void));
        // SAFETY: begin-print takes (operation, context, data) and returns nothing, as on_begin_print does.
        unsafe { connect_boxed(&self.0, c"begin-print", callback, f) }
    }

    pub fn connect_draw_page<F: Fn(&PrintContext<'_>, c_int) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(
            on_draw_page::<F> => fn(*mut c_void, *mut GtkPrintContext, c_int, *mut c_void)
        );
        // SAFETY: draw-page takes (operation, context, page number, data) and returns nothing, as on_draw_page does.
        unsafe { connect_boxed(&self.0, c"draw-page", callback, f) }
    }

    pub fn run(&self, action: c_int, parent: Option<&Widget>) -> Result<c_int, CString> {
        let parent = parent.map_or(core::ptr::null_mut(), Widget::as_ptr);
        let mut error: *mut GError = core::ptr::null_mut();
        // SAFETY: self is live, parent is null or a live window GTK checks, and error receives a GError the caller owns.
        let result = unsafe {
            sys::gtk_print_operation_run(self.as_ptr(), action, parent, (&raw mut error).cast())
        };
        match take_error(error) {
            Some(message) => Err(message),
            None => Ok(result),
        }
    }
}

impl Default for PrintOperation {
    fn default() -> PrintOperation {
        PrintOperation::new()
    }
}
