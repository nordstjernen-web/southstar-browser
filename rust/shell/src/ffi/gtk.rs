//! Southstar — small helpers over the GTK calls: signal connection, translated strings, C string conversion.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_ulong, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_gtk_ffi::{self as gtk, GCallback, GtkWidget};

unsafe extern "C" {}

macro_rules! handler {
    ($f:expr) => {
        #[allow(unused_unsafe)]
        unsafe {
            core::mem::transmute::<*const (), southstar_gtk_ffi::GCallback>($f as *const ())
        }
    };
}

pub(crate) use handler;

pub unsafe fn connect<T>(
    instance: *mut T,
    signal: &CStr,
    handler: GCallback,
    data: *mut c_void,
) -> c_ulong {
    unsafe { gtk::g_signal_connect_data(instance.cast(), signal.as_ptr(), handler, data, None, 0) }
}

pub fn tr(text: &CStr) -> &CStr {
    southstar_i18n::translate(text).unwrap_or(text)
}

pub fn tr_string(text: &CStr) -> String {
    tr(text).to_string_lossy().into_owned()
}

pub fn cstring(text: impl Into<Vec<u8>>) -> CString {
    let mut bytes = text.into();
    bytes.retain(|&b| b != 0);
    CString::new(bytes).unwrap_or_default()
}

pub unsafe fn borrowed<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub unsafe fn take_gstring(p: *mut c_char) -> Option<CString> {
    if p.is_null() {
        return None;
    }
    let out = unsafe { CStr::from_ptr(p) }.to_owned();
    unsafe { gtk::g_free(p.cast()) };
    Some(out)
}

pub fn build_filename(parts: &[&CStr]) -> CString {
    let mut argv: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    argv.push(ptr::null_mut());
    unsafe { take_gstring(gtk::g_build_filenamev(argv.as_mut_ptr())) }.unwrap_or_default()
}

pub fn basename(path: &CStr) -> CString {
    unsafe { take_gstring(gtk::g_path_get_basename(path.as_ptr())) }.unwrap_or_default()
}

pub fn file_test(path: &CStr, test: core::ffi::c_uint) -> bool {
    unsafe { gtk::g_file_test(path.as_ptr(), test) != 0 }
}

pub unsafe fn set_accessible_label(w: *mut GtkWidget, label: &CStr) {
    unsafe {
        gtk::gtk_accessible_update_property(
            w,
            gtk::ACCESSIBLE_PROPERTY_LABEL,
            label.as_ptr(),
            -1 as c_int,
        )
    };
}

pub unsafe fn toggle_css_class(w: *mut GtkWidget, class: &CStr, on: bool) {
    unsafe {
        if on {
            gtk::gtk_widget_add_css_class(w, class.as_ptr());
        } else {
            gtk::gtk_widget_remove_css_class(w, class.as_ptr());
        }
    }
}

pub unsafe fn set_margins(w: *mut GtkWidget, start: c_int, end: c_int, top: c_int, bottom: c_int) {
    unsafe {
        gtk::gtk_widget_set_margin_start(w, start);
        gtk::gtk_widget_set_margin_end(w, end);
        gtk::gtk_widget_set_margin_top(w, top);
        gtk::gtk_widget_set_margin_bottom(w, bottom);
    }
}

pub unsafe fn label(text: &CStr) -> *mut GtkWidget {
    unsafe { gtk::gtk_label_new(text.as_ptr()) }
}

pub unsafe fn hexpand_spacer() -> *mut GtkWidget {
    unsafe {
        let spacer = gtk::gtk_label_new(c"".as_ptr());
        gtk::gtk_widget_set_hexpand(spacer, 1);
        spacer
    }
}

pub fn open_path(path: &CStr) {
    unsafe {
        if let Some(uri) = take_gstring(gtk::g_filename_to_uri(
            path.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
        )) {
            gtk::g_app_info_launch_default_for_uri(uri.as_ptr(), ptr::null_mut(), ptr::null_mut());
        }
    }
}
