//! Southstar — a tab's renderer view as the procview module exposes it, and the notebook pages that carry one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_gtk_ffi::{self as gtk, GdkPaintable, GtkWidget};

use super::procview_chrome::{
    ns_proc_view_find_close, ns_proc_view_find_open, ns_proc_view_print, ns_proc_view_save_image,
    ns_proc_view_save_pdf, ns_proc_view_set_color_scheme,
};
use super::procview_result::{
    ns_proc_view_audio_pid, ns_proc_view_back, ns_proc_view_can_back, ns_proc_view_can_forward,
    ns_proc_view_end_task, ns_proc_view_exit_fullscreen, ns_proc_view_favicon, ns_proc_view_focus,
    ns_proc_view_forward, ns_proc_view_is_loading, ns_proc_view_load, ns_proc_view_reload,
    ns_proc_view_remote_ip, ns_proc_view_renderer_pid, ns_proc_view_security, ns_proc_view_stop,
    ns_proc_view_stop_audio, ns_proc_view_stop_video, ns_proc_view_title,
    ns_proc_view_toggle_console, ns_proc_view_url, ns_proc_view_video_pid, ns_proc_view_zoom_in,
    ns_proc_view_zoom_out, ns_proc_view_zoom_percent, ns_proc_view_zoom_reset,
};
pub use super::procview_state::NsProcView;
use super::procview_view::{
    ns_proc_view_is_private, ns_proc_view_new, ns_proc_view_set_notify, ns_proc_view_set_private,
    ns_proc_view_widget,
};

pub type Notify =
    unsafe extern "C" fn(view: *mut NsProcView, evt: c_int, text: *const c_char, data: *mut c_void);

pub const PAGE_VIEW_KEY: &CStr = c"ns-proc-view";

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct View(NonNull<NsProcView>);

fn owned(p: *const c_char) -> Option<CString> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_owned())
}

impl View {
    pub unsafe fn from_raw(p: *mut c_void) -> Option<View> {
        NonNull::new(p.cast()).map(View)
    }

    pub fn create() -> Option<View> {
        NonNull::new(ns_proc_view_new()).map(View)
    }

    pub fn raw(self) -> *mut NsProcView {
        self.0.as_ptr()
    }

    fn p(self) -> *mut NsProcView {
        self.0.as_ptr()
    }

    pub fn widget(self) -> *mut GtkWidget {
        ns_proc_view_widget(self.p())
    }

    pub unsafe fn set_notify(self, cb: Notify, data: *mut c_void) {
        ns_proc_view_set_notify(self.p(), Some(cb), data);
    }

    pub fn set_private(self, private_mode: bool) {
        ns_proc_view_set_private(self.p(), private_mode);
    }

    pub fn is_private(self) -> bool {
        ns_proc_view_is_private(self.p())
    }

    pub fn load(self, url: &CStr) {
        ns_proc_view_load(self.p(), url.as_ptr());
    }

    pub fn back(self) {
        ns_proc_view_back(self.p());
    }

    pub fn forward(self) {
        ns_proc_view_forward(self.p());
    }

    pub fn reload(self) {
        ns_proc_view_reload(self.p());
    }

    pub fn stop(self) {
        ns_proc_view_stop(self.p());
    }

    pub fn toggle_console(self) {
        ns_proc_view_toggle_console(self.p());
    }

    pub fn exit_fullscreen(self) {
        ns_proc_view_exit_fullscreen(self.p());
    }

    pub fn can_back(self) -> bool {
        ns_proc_view_can_back(self.p())
    }

    pub fn can_forward(self) -> bool {
        ns_proc_view_can_forward(self.p())
    }

    pub fn url(self) -> Option<CString> {
        owned(ns_proc_view_url(self.p()))
    }

    pub fn title(self) -> Option<CString> {
        owned(ns_proc_view_title(self.p()))
    }

    pub fn is_loading(self) -> bool {
        ns_proc_view_is_loading(self.p())
    }

    pub fn security(self) -> c_int {
        ns_proc_view_security(self.p())
    }

    pub fn remote_ip(self) -> Option<CString> {
        owned(ns_proc_view_remote_ip(self.p()))
    }

    pub fn favicon(self) -> *mut GdkPaintable {
        ns_proc_view_favicon(self.p()).cast()
    }

    pub fn renderer_pid(self) -> c_int {
        ns_proc_view_renderer_pid(self.p())
    }

    pub fn audio_pid(self) -> c_int {
        ns_proc_view_audio_pid(self.p())
    }

    pub fn video_pid(self) -> c_int {
        ns_proc_view_video_pid(self.p())
    }

    pub fn end_task(self) {
        ns_proc_view_end_task(self.p());
    }

    pub fn stop_video(self) {
        ns_proc_view_stop_video(self.p());
    }

    pub fn stop_audio(self) {
        ns_proc_view_stop_audio(self.p());
    }

    pub fn zoom_in(self) {
        ns_proc_view_zoom_in(self.p());
    }

    pub fn zoom_out(self) {
        ns_proc_view_zoom_out(self.p());
    }

    pub fn zoom_reset(self) {
        ns_proc_view_zoom_reset(self.p());
    }

    pub fn zoom_percent(self) -> c_int {
        ns_proc_view_zoom_percent(self.p())
    }

    pub fn focus(self) {
        ns_proc_view_focus(self.p());
    }

    pub fn find_open(self) {
        ns_proc_view_find_open(self.p());
    }

    pub fn set_color_scheme(self, dark: bool) {
        ns_proc_view_set_color_scheme(self.p(), dark);
    }
}

pub fn find_close(v: Option<View>) -> bool {
    ns_proc_view_find_close(v.map_or(core::ptr::null_mut(), View::raw))
}

pub fn print(v: Option<View>) {
    ns_proc_view_print(v.map_or(core::ptr::null_mut(), View::raw));
}

pub fn save_pdf(v: Option<View>) {
    ns_proc_view_save_pdf(v.map_or(core::ptr::null_mut(), View::raw));
}

pub fn save_image(v: Option<View>) {
    ns_proc_view_save_image(v.map_or(core::ptr::null_mut(), View::raw));
}

pub unsafe fn for_page(page: *mut GtkWidget) -> Option<View> {
    if page.is_null() {
        return None;
    }
    unsafe { View::from_raw(gtk::g_object_get_data(page.cast(), PAGE_VIEW_KEY.as_ptr())) }
}

pub unsafe fn views(notebook: *mut GtkWidget) -> Vec<View> {
    unsafe {
        let n = gtk::gtk_notebook_get_n_pages(notebook);
        (0..n)
            .filter_map(|i| for_page(gtk::gtk_notebook_get_nth_page(notebook, i)))
            .collect()
    }
}
