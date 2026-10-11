//! Southstar — a tab's renderer view as procview.c exposes it, and the notebook pages that carry one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_gtk_ffi::{self as gtk, GBoolean, GdkPaintable, GtkWidget};

pub use super::procview_state::NsProcView;

pub type Notify =
    unsafe extern "C" fn(view: *mut NsProcView, evt: c_int, text: *const c_char, data: *mut c_void);

unsafe extern "C" {
    fn ns_proc_view_new() -> *mut NsProcView;
    fn ns_proc_view_widget(view: *mut NsProcView) -> *mut GtkWidget;
    fn ns_proc_view_set_notify(view: *mut NsProcView, cb: Notify, data: *mut c_void);
    fn ns_proc_view_set_private(view: *mut NsProcView, private_mode: GBoolean);
    fn ns_proc_view_is_private(view: *mut NsProcView) -> GBoolean;
    fn ns_proc_view_load(view: *mut NsProcView, url: *const c_char);
    fn ns_proc_view_back(view: *mut NsProcView);
    fn ns_proc_view_forward(view: *mut NsProcView);
    fn ns_proc_view_reload(view: *mut NsProcView);
    fn ns_proc_view_stop(view: *mut NsProcView);
    fn ns_proc_view_toggle_console(view: *mut NsProcView);
    fn ns_proc_view_exit_fullscreen(view: *mut NsProcView);
    fn ns_proc_view_can_back(view: *mut NsProcView) -> GBoolean;
    fn ns_proc_view_can_forward(view: *mut NsProcView) -> GBoolean;
    fn ns_proc_view_url(view: *mut NsProcView) -> *const c_char;
    fn ns_proc_view_title(view: *mut NsProcView) -> *const c_char;
    fn ns_proc_view_is_loading(view: *mut NsProcView) -> GBoolean;
    fn ns_proc_view_security(view: *mut NsProcView) -> c_int;
    fn ns_proc_view_remote_ip(view: *mut NsProcView) -> *const c_char;
    fn ns_proc_view_favicon(view: *mut NsProcView) -> *mut GdkPaintable;
    fn ns_proc_view_renderer_pid(view: *mut NsProcView) -> c_int;
    fn ns_proc_view_audio_pid(view: *mut NsProcView) -> c_int;
    fn ns_proc_view_video_pid(view: *mut NsProcView) -> c_int;
    fn ns_proc_view_end_task(view: *mut NsProcView);
    fn ns_proc_view_print(view: *mut NsProcView);
    fn ns_proc_view_save_pdf(view: *mut NsProcView);
    fn ns_proc_view_save_image(view: *mut NsProcView);
    fn ns_proc_view_stop_video(view: *mut NsProcView);
    fn ns_proc_view_stop_audio(view: *mut NsProcView);
    fn ns_proc_view_zoom_in(view: *mut NsProcView);
    fn ns_proc_view_zoom_out(view: *mut NsProcView);
    fn ns_proc_view_zoom_reset(view: *mut NsProcView);
    fn ns_proc_view_zoom_percent(view: *mut NsProcView) -> c_int;
    fn ns_proc_view_focus(view: *mut NsProcView);
    fn ns_proc_view_find_open(view: *mut NsProcView);
    fn ns_proc_view_find_close(view: *mut NsProcView) -> GBoolean;
    fn ns_proc_view_set_color_scheme(view: *mut NsProcView, dark: GBoolean);
}

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
        NonNull::new(unsafe { ns_proc_view_new() }).map(View)
    }

    pub fn raw(self) -> *mut NsProcView {
        self.0.as_ptr()
    }

    fn p(self) -> *mut NsProcView {
        self.0.as_ptr()
    }

    pub fn widget(self) -> *mut GtkWidget {
        unsafe { ns_proc_view_widget(self.p()) }
    }

    pub unsafe fn set_notify(self, cb: Notify, data: *mut c_void) {
        unsafe { ns_proc_view_set_notify(self.p(), cb, data) };
    }

    pub fn set_private(self, private_mode: bool) {
        unsafe { ns_proc_view_set_private(self.p(), GBoolean::from(private_mode)) };
    }

    pub fn is_private(self) -> bool {
        unsafe { ns_proc_view_is_private(self.p()) != 0 }
    }

    pub fn load(self, url: &CStr) {
        unsafe { ns_proc_view_load(self.p(), url.as_ptr()) };
    }

    pub fn back(self) {
        unsafe { ns_proc_view_back(self.p()) };
    }

    pub fn forward(self) {
        unsafe { ns_proc_view_forward(self.p()) };
    }

    pub fn reload(self) {
        unsafe { ns_proc_view_reload(self.p()) };
    }

    pub fn stop(self) {
        unsafe { ns_proc_view_stop(self.p()) };
    }

    pub fn toggle_console(self) {
        unsafe { ns_proc_view_toggle_console(self.p()) };
    }

    pub fn exit_fullscreen(self) {
        unsafe { ns_proc_view_exit_fullscreen(self.p()) };
    }

    pub fn can_back(self) -> bool {
        unsafe { ns_proc_view_can_back(self.p()) != 0 }
    }

    pub fn can_forward(self) -> bool {
        unsafe { ns_proc_view_can_forward(self.p()) != 0 }
    }

    pub fn url(self) -> Option<CString> {
        owned(unsafe { ns_proc_view_url(self.p()) })
    }

    pub fn title(self) -> Option<CString> {
        owned(unsafe { ns_proc_view_title(self.p()) })
    }

    pub fn is_loading(self) -> bool {
        unsafe { ns_proc_view_is_loading(self.p()) != 0 }
    }

    pub fn security(self) -> c_int {
        unsafe { ns_proc_view_security(self.p()) }
    }

    pub fn remote_ip(self) -> Option<CString> {
        owned(unsafe { ns_proc_view_remote_ip(self.p()) })
    }

    pub fn favicon(self) -> *mut GdkPaintable {
        unsafe { ns_proc_view_favicon(self.p()) }
    }

    pub fn renderer_pid(self) -> c_int {
        unsafe { ns_proc_view_renderer_pid(self.p()) }
    }

    pub fn audio_pid(self) -> c_int {
        unsafe { ns_proc_view_audio_pid(self.p()) }
    }

    pub fn video_pid(self) -> c_int {
        unsafe { ns_proc_view_video_pid(self.p()) }
    }

    pub fn end_task(self) {
        unsafe { ns_proc_view_end_task(self.p()) };
    }

    pub fn stop_video(self) {
        unsafe { ns_proc_view_stop_video(self.p()) };
    }

    pub fn stop_audio(self) {
        unsafe { ns_proc_view_stop_audio(self.p()) };
    }

    pub fn zoom_in(self) {
        unsafe { ns_proc_view_zoom_in(self.p()) };
    }

    pub fn zoom_out(self) {
        unsafe { ns_proc_view_zoom_out(self.p()) };
    }

    pub fn zoom_reset(self) {
        unsafe { ns_proc_view_zoom_reset(self.p()) };
    }

    pub fn zoom_percent(self) -> c_int {
        unsafe { ns_proc_view_zoom_percent(self.p()) }
    }

    pub fn focus(self) {
        unsafe { ns_proc_view_focus(self.p()) };
    }

    pub fn find_open(self) {
        unsafe { ns_proc_view_find_open(self.p()) };
    }

    pub fn set_color_scheme(self, dark: bool) {
        unsafe { ns_proc_view_set_color_scheme(self.p(), GBoolean::from(dark)) };
    }
}

pub fn find_close(v: Option<View>) -> bool {
    unsafe { ns_proc_view_find_close(v.map_or(core::ptr::null_mut(), View::raw)) != 0 }
}

pub fn print(v: Option<View>) {
    unsafe { ns_proc_view_print(v.map_or(core::ptr::null_mut(), View::raw)) };
}

pub fn save_pdf(v: Option<View>) {
    unsafe { ns_proc_view_save_pdf(v.map_or(core::ptr::null_mut(), View::raw)) };
}

pub fn save_image(v: Option<View>) {
    unsafe { ns_proc_view_save_image(v.map_or(core::ptr::null_mut(), View::raw)) };
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
