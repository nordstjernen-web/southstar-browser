//! Southstar — the GtkFileDialog save chooser, reporting the chosen path to a Rust closure.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};

use southstar_gtk_ffi::{self as sys, GtkFileDialog};

use crate::event::take_path;
use crate::print::{GError, take_error};
use crate::{Object, Widget};

pub struct FileDialog(Object);

unsafe extern "C" fn on_save_done<F: FnOnce(Option<Vec<u8>>) + 'static>(
    source: *mut c_void,
    result: *mut c_void,
    data: *mut c_void,
) {
    // SAFETY: data came from Box::into_raw in save and GIO calls the ready callback once.
    let f = unsafe { Box::from_raw(data.cast::<F>()) };
    let mut error: *mut GError = core::ptr::null_mut();
    // SAFETY: GIO passes the dialog and the result of its save; error receives a GError the caller owns.
    let file =
        unsafe { sys::gtk_file_dialog_save_finish(source.cast(), result, (&raw mut error).cast()) };
    drop(take_error(error));
    let path = (!file.is_null()).then(|| take_path(file)).flatten();
    if !file.is_null() {
        // SAFETY: the finish call returned a new reference to the chosen GFile, released once here.
        unsafe { sys::g_object_unref(file) };
    }
    f(path);
}

impl FileDialog {
    pub fn new() -> FileDialog {
        // SAFETY: gtk_file_dialog_new has no preconditions and returns a new, non-floating GObject.
        let dialog = unsafe { sys::gtk_file_dialog_new() };
        // SAFETY: dialog is live and its one reference passes to the handle.
        FileDialog(unsafe { Object::from_full(dialog.cast()) })
    }

    fn as_ptr(&self) -> *mut GtkFileDialog {
        self.0.as_ptr().cast()
    }

    pub fn set_title(&self, title: &CStr) {
        // SAFETY: self is a live GtkFileDialog and title is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_file_dialog_set_title(self.as_ptr(), title.as_ptr()) };
    }

    pub fn set_initial_name(&self, name: &CStr) {
        // SAFETY: self is a live GtkFileDialog and name is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_file_dialog_set_initial_name(self.as_ptr(), name.as_ptr()) };
    }

    pub fn save<F: FnOnce(Option<Vec<u8>>) + 'static>(&self, parent: Option<&Widget>, f: F) {
        let parent = parent.map_or(core::ptr::null_mut(), Widget::as_ptr);
        let data = Box::into_raw(Box::new(f)).cast::<c_void>();
        // SAFETY: self is live, parent is null or a live window GTK checks, and on_save_done reclaims data exactly once.
        unsafe {
            sys::gtk_file_dialog_save(
                self.as_ptr(),
                parent,
                core::ptr::null_mut(),
                Some(on_save_done::<F>),
                data,
            )
        };
    }
}

impl Default for FileDialog {
    fn default() -> FileDialog {
        FileDialog::new()
    }
}
