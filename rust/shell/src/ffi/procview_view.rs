//! Southstar — the tab view's lifetime: building its widgets, the worker it starts, its reference count and its teardown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk_ffi::{self as gtk, GtkWidget};

use super::gtk::{connect, handler};
use super::pagelayers::{ns_page_layers_free, ns_page_layers_new};
use super::procview_chrome::{build_perm_bar, build_search_bar, ctx_install_actions};
use super::procview_draw::{
    disarm_anim, ns_pv_area_new, ns_pv_area_set_view, ns_pv_install_area_signals,
    ns_pv_install_scroll, ns_pv_install_view_signals,
};
use super::procview_input::{
    ns_pv_input_install_keys, ns_pv_input_install_pointer, ns_pv_input_install_secondary, push_req,
};
use super::procview_media::{ns_pv_media_free, ns_pv_media_new};
use super::procview_state::{NotifyFn, NsProcView, Req};
use super::procview_worker::{
    REQ_QUIT, free_req, lock_proc, ns_proc_renderer_path, ns_rproc_http_interrupt, start_worker,
    unlock_proc,
};

const ACT_HOVER: c_int = 0;

unsafe extern "C" {
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_ref_count_init(rc: *mut c_int);
    fn g_ref_count_inc(rc: *mut c_int);
    fn g_ref_count_dec(rc: *mut c_int) -> GBoolean;
    fn g_mutex_init(mutex: *mut c_void);
    fn g_mutex_clear(mutex: *mut c_void);
    fn g_async_queue_new() -> *mut c_void;
    fn g_async_queue_unref(queue: *mut c_void);
    fn g_async_queue_try_pop(queue: *mut c_void) -> *mut c_void;
    fn g_thread_join(thread: *mut c_void) -> *mut c_void;
}

fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    unsafe { &mut *v }
}

fn unref_object(object: *mut c_void) {
    if !object.is_null() {
        unsafe { gtk::g_object_unref(object) };
    }
}

fn free_view(v: *mut NsProcView) {
    let state = view(v);
    unsafe { ns_pv_media_free(state.media.cast()) };
    state.media = ptr::null_mut();
    if !state.queue.is_null() {
        loop {
            let req = unsafe { g_async_queue_try_pop(state.queue) }.cast::<Req>();
            if req.is_null() {
                break;
            }
            free_req(req);
        }
        unsafe { g_async_queue_unref(state.queue) };
    }
    unref_object(state.frame);
    state.frame = ptr::null_mut();
    unsafe { ns_page_layers_free(state.layers.cast()) };
    unref_object(state.favicon);
    state.favicon = ptr::null_mut();
    unsafe {
        if !state.ctx_popover.is_null() {
            gtk::gtk_widget_unparent(state.ctx_popover);
        }
        unref_object(state.ctx_actions);
        glib::g_free(state.ctx_link.cast());
        if !state.history.is_null() {
            glib::g_ptr_array_unref(state.history.cast());
        }
        for s in [
            state.renderer_path,
            state.current_url,
            state.current_title,
            state.remote_ip,
            state.deferred_url,
            state.perm_origin,
        ] {
            glib::g_free(s.cast());
        }
        unref_object(state.hourglass_cursor);
        g_mutex_clear((&raw mut state.proc_lock).cast());
        glib::g_free(v.cast());
    }
}

unsafe extern "C" fn on_area_destroy(widget: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    unsafe { ns_pv_area_set_view(widget, ptr::null_mut()) };
    let state = view(v);
    state.closed = 1;
    unref_object(state.im);
    state.im = ptr::null_mut();
    disarm_anim(v);
    let state = view(v);
    unsafe {
        if state.hover_after_scroll_id != 0 {
            gtk::g_source_remove(state.hover_after_scroll_id);
            state.hover_after_scroll_id = 0;
        }
        if state.console_poll_id != 0 {
            gtk::g_source_remove(state.console_poll_id);
            state.console_poll_id = 0;
        }
        if !state.console_window.is_null() {
            let win = state.console_window;
            state.console_window = ptr::null_mut();
            state.console_notebook = ptr::null_mut();
            state.console_entry = ptr::null_mut();
            state.console_view = ptr::null_mut();
            state.console_buffer = ptr::null_mut();
            state.net_view = ptr::null_mut();
            state.net_buffer = ptr::null_mut();
            state.perf_view = ptr::null_mut();
            state.perf_buffer = ptr::null_mut();
            state.layout_view = ptr::null_mut();
            state.layout_buffer = ptr::null_mut();
            state.elements_view = ptr::null_mut();
            state.elements_buffer = ptr::null_mut();
            state.inspect_entry = ptr::null_mut();
            gtk::gtk_window_destroy(win);
        }
        if !state.ctx_popover.is_null() {
            gtk::gtk_widget_unparent(state.ctx_popover);
            state.ctx_popover = ptr::null_mut();
        }
    }
    state.area = ptr::null_mut();
    state.perm_revealer = ptr::null_mut();
    state.perm_label = ptr::null_mut();
    let req = unsafe { g_malloc0(core::mem::size_of::<Req>()) }.cast::<Req>();
    unsafe { (*req).r#type = REQ_QUIT };
    push_req(v, req);
    lock_proc(v);
    let p = view(v).proc;
    if !p.is_null() {
        unsafe { ns_rproc_http_interrupt(p) };
    }
    unlock_proc(v);
    let state = view(v);
    if !state.thread.is_null() {
        unsafe { g_thread_join(state.thread) };
        state.thread = ptr::null_mut();
    }
    ns_proc_view_unref(v);
}

pub(super) fn ns_proc_view_ref(v: *mut NsProcView) -> *mut NsProcView {
    unsafe { g_ref_count_inc(&raw mut (*v).rc) };
    v
}

pub(super) fn ns_proc_view_unref(v: *mut NsProcView) {
    if unsafe { g_ref_count_dec(&raw mut (*v).rc) } != 0 {
        free_view(v);
    }
}

pub(super) fn ns_proc_view_area(v: *mut NsProcView) -> *mut GtkWidget {
    view(v).area
}

pub(super) fn ns_proc_view_media(v: *mut NsProcView) -> *mut c_void {
    if v.is_null() {
        ptr::null_mut()
    } else {
        view(v).media
    }
}

pub(super) fn ns_proc_view_new() -> *mut NsProcView {
    let v = unsafe { g_malloc0(core::mem::size_of::<NsProcView>()) }.cast::<NsProcView>();
    let state = view(v);
    unsafe {
        g_ref_count_init(&raw mut state.rc);
        g_mutex_init((&raw mut state.proc_lock).cast());
    }
    state.media = ns_pv_media_new().cast();
    state.renderer_path = ns_proc_renderer_path();
    state.queue = unsafe { g_async_queue_new() };
    state.history = unsafe { glib::g_ptr_array_new_with_free_func(Some(glib::g_free)) }.cast();
    state.hist_index = -1;
    state.link_pending_action = ACT_HOVER;
    state.pending_record = 1;
    state.scale = 1.0;

    unsafe {
        state.hadj = gtk::gtk_adjustment_new(0.0, 0.0, 1.0, 60.0, 60.0, 1.0).cast();
        state.vadj = gtk::gtk_adjustment_new(0.0, 0.0, 1.0, 60.0, 60.0, 1.0).cast();
        ns_pv_install_view_signals(v);

        state.layers = ns_page_layers_new().cast();
        state.area = ns_pv_area_new(v);
        gtk::gtk_widget_set_hexpand(state.area, 1);
        gtk::gtk_widget_set_vexpand(state.area, 1);
        gtk::gtk_widget_set_focusable(state.area, 1);
        ns_pv_install_area_signals(v);

        let grid = gtk::gtk_grid_new();
        state.vscroll = gtk::gtk_scrollbar_new(gtk::ORIENTATION_VERTICAL, state.vadj.cast());
        state.hscroll = gtk::gtk_scrollbar_new(gtk::ORIENTATION_HORIZONTAL, state.hadj.cast());
        gtk::gtk_grid_attach(grid, state.area, 0, 0, 1, 1);
        gtk::gtk_grid_attach(grid, state.vscroll, 1, 0, 1, 1);
        gtk::gtk_grid_attach(grid, state.hscroll, 0, 1, 1, 1);
        gtk::gtk_widget_set_visible(state.vscroll, 0);
        gtk::gtk_widget_set_visible(state.hscroll, 0);

        state.overlay = gtk::gtk_overlay_new();
        gtk::gtk_overlay_set_child(state.overlay, grid);
        gtk::gtk_widget_set_vexpand(state.overlay, 1);
    }
    build_perm_bar(v);
    let state = view(v);
    unsafe {
        state.root = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        gtk::gtk_box_append(state.root, state.perm_revealer);
        gtk::gtk_box_append(state.root, state.overlay);
        gtk::gtk_widget_set_vexpand(state.root, 1);
    }
    build_search_bar(v);
    ns_pv_input_install_pointer(v);
    ctx_install_actions(v);
    ns_pv_input_install_secondary(v);
    ns_pv_install_scroll(v);
    ns_pv_input_install_keys(v);
    unsafe {
        connect(
            view(v).area,
            c"destroy",
            handler!(on_area_destroy),
            v.cast(),
        );
    }
    view(v).thread = start_worker(v);
    v
}

pub(super) fn ns_proc_view_widget(v: *mut NsProcView) -> *mut GtkWidget {
    view(v).root
}

pub(super) fn ns_proc_view_set_notify(v: *mut NsProcView, cb: NotifyFn, ud: *mut c_void) {
    let state = view(v);
    state.notify = cb;
    state.notify_ud = ud;
}

pub(super) fn ns_proc_view_set_private(v: *mut NsProcView, private_mode: bool) {
    if !v.is_null() {
        view(v).private_mode = GBoolean::from(private_mode);
    }
}

pub(super) fn ns_proc_view_is_private(v: *mut NsProcView) -> bool {
    !v.is_null() && view(v).private_mode != 0
}
