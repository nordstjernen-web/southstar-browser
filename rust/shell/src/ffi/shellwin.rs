//! Southstar — the C entry points of shellwin.h: the downloads and task manager windows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_gtk_ffi::GtkWidget;

use super::gtk::borrowed;
use super::{downloads, taskmgr};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_downloads_show(parent: *mut GtkWidget) {
    unsafe { downloads::show(parent) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_downloads_start(
    parent: *mut GtkWidget,
    url: *const c_char,
    suggested: *const c_char,
) {
    unsafe {
        if let Some(url) = borrowed(url) {
            downloads::start(parent, url, borrowed(suggested));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_task_manager_show(parent: *mut GtkWidget, notebook: *mut GtkWidget) {
    unsafe { taskmgr::show(parent, notebook) };
}
