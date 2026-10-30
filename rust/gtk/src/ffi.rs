//! Southstar — the GObject and GTK declarations the safe handles need beyond southstar-gtk-ffi.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};

use southstar_gtk_ffi::{GDestroyNotify, GType};

pub type Quark = u32;

#[repr(C)]
pub struct GWeakRef {
    pub priv_: *mut c_void,
}

unsafe extern "C" {
    pub fn g_object_ref_sink(object: *mut c_void) -> *mut c_void;
    pub fn g_quark_from_string(string: *const c_char) -> Quark;
    pub fn g_object_set_qdata_full(
        object: *mut c_void,
        quark: Quark,
        data: *mut c_void,
        destroy: GDestroyNotify,
    );
    pub fn g_object_get_qdata(object: *mut c_void, quark: Quark) -> *mut c_void;
    pub fn g_weak_ref_init(weak_ref: *mut GWeakRef, object: *mut c_void);
    pub fn g_weak_ref_get(weak_ref: *mut GWeakRef) -> *mut c_void;
    pub fn g_weak_ref_clear(weak_ref: *mut GWeakRef);
    pub fn gtk_box_get_type() -> GType;
    pub fn gtk_label_get_type() -> GType;
    pub fn gtk_button_get_type() -> GType;
    pub fn gtk_list_box_get_type() -> GType;
    pub fn gtk_list_box_row_get_type() -> GType;
    pub fn gtk_scrolled_window_get_type() -> GType;
    pub fn gtk_text_view_get_type() -> GType;
    pub fn gtk_progress_bar_get_type() -> GType;
    pub fn gtk_notebook_get_type() -> GType;
    pub fn gtk_window_set_hide_on_close(
        window: *mut southstar_gtk_ffi::GtkWidget,
        setting: southstar_gtk_ffi::GBoolean,
    );
}
