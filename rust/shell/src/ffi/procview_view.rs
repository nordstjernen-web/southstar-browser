//! Southstar — the tab view's lifetime: building its widgets, the worker it starts, its reference count and its teardown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;
use core::sync::atomic::Ordering;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk::{self as gtk, Overlay, Widget};
use southstar_gtk_ffi::{self as sys, GtkWidget};

use super::pagelayers::{ns_page_layers_free, ns_page_layers_new};
use super::procview_chrome::{build_perm_bar, build_search_bar, ctx_install_actions};
use super::procview_draw::{
    disarm_anim, ns_pv_area_detach, ns_pv_area_new, ns_pv_install_area_signals,
    ns_pv_install_scroll, ns_pv_install_view_signals,
};
use super::procview_input::{
    new_req, ns_pv_input_install_keys, ns_pv_input_install_pointer, ns_pv_input_install_secondary,
    push_req, view,
};
use super::procview_media::{ns_pv_media_free, ns_pv_media_new};
use super::procview_state::{NotifyFn, NsProcView, Req};
use super::procview_worker::{
    REQ_QUIT, free_req, join_worker, lock_proc, ns_proc_renderer_path, start_worker, unlock_proc,
};

const ACT_HOVER: c_int = 0;

unsafe extern "C" {
    fn g_mutex_init(mutex: *mut c_void);
    fn g_mutex_clear(mutex: *mut c_void);
    fn g_async_queue_new() -> *mut c_void;
    fn g_async_queue_unref(queue: *mut c_void);
    fn g_async_queue_try_pop(queue: *mut c_void) -> *mut c_void;
}

fn unref_object(object: *mut c_void) {
    if !object.is_null() {
        // SAFETY: object is a reference the view owns and drops once, here.
        unsafe { sys::g_object_unref(object) };
    }
}

fn free_view(v: *mut NsProcView) {
    let state = view(v);
    // SAFETY: media is null or the view's own Media, freed once here.
    unsafe { ns_pv_media_free(state.media.cast()) };
    state.media = ptr::null_mut();
    if !state.queue.is_null() {
        loop {
            // SAFETY: the queue is the view's own and only the main loop is left using it.
            let req = unsafe { g_async_queue_try_pop(state.queue) }.cast::<Req>();
            if req.is_null() {
                break;
            }
            free_req(req);
        }
        // SAFETY: drops the view's reference to its queue.
        unsafe { g_async_queue_unref(state.queue) };
    }
    unref_object(state.frame);
    state.frame = ptr::null_mut();
    // SAFETY: layers is null or the view's own PageLayers, freed once here.
    unsafe { ns_page_layers_free(state.layers.cast()) };
    unref_object(state.favicon);
    state.favicon = ptr::null_mut();
    if !state.ctx_popover.is_null() {
        // SAFETY: ctx_popover is the live popover parented to the area.
        unsafe { sys::gtk_widget_unparent(state.ctx_popover) };
    }
    unref_object(state.ctx_actions);
    state.ui = None;
    // SAFETY: renderer_path is null or the view's own GLib string.
    unsafe { glib::g_free(state.renderer_path.cast()) };
    unref_object(state.hourglass_cursor);
    // SAFETY: the worker has been joined, so nothing holds the lock any more.
    unsafe { g_mutex_clear((&raw mut state.proc_lock).cast()) };
    // SAFETY: the view was allocated with g_malloc0 and nothing refers to it any more.
    unsafe { glib::g_free(v.cast()) };
}

fn on_area_destroy(v: *mut NsProcView) {
    ns_pv_area_detach(v);
    let state = view(v);
    state.closed = 1;
    unref_object(state.im);
    state.im = ptr::null_mut();
    disarm_anim(v);
    let state = view(v);
    if state.hover_after_scroll_id != 0 {
        // SAFETY: the id is the view's pending hover source, still attached.
        unsafe { sys::g_source_remove(state.hover_after_scroll_id) };
        state.hover_after_scroll_id = 0;
    }
    let ui = state.ui();
    if let Some(poll) = ui.console_poll.take() {
        poll.remove();
    }
    let console = ui.console.take();
    ui.perm = None;
    ui.find = None;
    if let Some(console) = console {
        console.window.destroy();
    }
    let state = view(v);
    if !state.ctx_popover.is_null() {
        // SAFETY: ctx_popover is the live popover parented to the area.
        unsafe { sys::gtk_widget_unparent(state.ctx_popover) };
        state.ctx_popover = ptr::null_mut();
    }
    state.area = ptr::null_mut();
    push_req(v, new_req(REQ_QUIT, |_| {}));
    lock_proc(v);
    if let Some(remote) = view(v).remote {
        remote.interrupt();
    }
    unlock_proc(v);
    join_worker(v);
    ns_proc_view_unref(v);
}

pub(super) fn ns_proc_view_ref(v: *mut NsProcView) -> *mut NsProcView {
    view(v).rc.fetch_add(1, Ordering::Relaxed);
    v
}

pub(super) fn ns_proc_view_unref(v: *mut NsProcView) {
    if view(v).rc.fetch_sub(1, Ordering::AcqRel) == 1 {
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

fn build_grid(v: *mut NsProcView, area: &Widget) -> Widget {
    let state = view(v);
    // SAFETY: gtk_grid_new has no preconditions and returns a new floating widget.
    let grid = unsafe { sys::gtk_grid_new() };
    // SAFETY: grid is the live widget just created; the handle sinks its floating reference.
    let grid = unsafe { Widget::from_ptr(grid) }.expect("gtk_grid_new never returns null");
    // SAFETY: vadj is the view's live adjustment; the scrollbar sinks its floating reference.
    state.vscroll = unsafe { sys::gtk_scrollbar_new(gtk::ORIENTATION_VERTICAL, state.vadj.cast()) };
    // SAFETY: hadj is the view's live adjustment; the scrollbar sinks its floating reference.
    state.hscroll =
        unsafe { sys::gtk_scrollbar_new(gtk::ORIENTATION_HORIZONTAL, state.hadj.cast()) };
    for (child, column, row) in [
        (area.as_ptr(), 0, 0),
        (state.vscroll, 1, 0),
        (state.hscroll, 0, 1),
    ] {
        // SAFETY: grid and child are live widgets; the grid takes its own reference to child.
        unsafe { sys::gtk_grid_attach(grid.as_ptr(), child, column, row, 1, 1) };
    }
    for scrollbar in [state.vscroll, state.hscroll] {
        // SAFETY: scrollbar is a live widget the grid holds.
        unsafe { sys::gtk_widget_set_visible(scrollbar, 0) };
    }
    grid
}

pub(super) fn ns_proc_view_new() -> *mut NsProcView {
    // SAFETY: g_malloc0 aborts rather than return null; every field of NsProcView may be zero.
    let v = unsafe { glib::g_malloc0(core::mem::size_of::<NsProcView>()) }.cast::<NsProcView>();
    let state = view(v);
    state.rc.store(1, Ordering::Relaxed);
    // SAFETY: proc_lock is the zeroed mutex inside the new view, initialized once here.
    unsafe { g_mutex_init((&raw mut state.proc_lock).cast()) };
    state.remote = None;
    state.media = ns_pv_media_new().cast();
    state.renderer_path = ns_proc_renderer_path();
    // SAFETY: g_async_queue_new has no preconditions.
    state.queue = unsafe { g_async_queue_new() };
    state.hist_index = -1;
    state.link_pending_action = ACT_HOVER;
    state.pending_record = 1;
    state.scale = 1.0;

    // SAFETY: the adjustment constructor takes plain numbers and returns a new floating object.
    state.hadj = unsafe { sys::gtk_adjustment_new(0.0, 0.0, 1.0, 60.0, 60.0, 1.0) }.cast();
    // SAFETY: the adjustment constructor takes plain numbers and returns a new floating object.
    state.vadj = unsafe { sys::gtk_adjustment_new(0.0, 0.0, 1.0, 60.0, 60.0, 1.0) }.cast();
    ns_pv_install_view_signals(v);

    let state = view(v);
    state.layers = ns_page_layers_new().cast();
    let area = ns_pv_area_new(v);
    state.area = area.as_ptr();
    area.set_hexpand(true);
    area.set_vexpand(true);
    // SAFETY: area is a live widget.
    unsafe { sys::gtk_widget_set_focusable(area.as_ptr(), 1) };
    ns_pv_install_area_signals(v);

    let grid = build_grid(v, &area);
    let overlay = Overlay::new();
    overlay.set_child(&grid);
    overlay.set_vexpand(true);

    let perm = build_perm_bar(v);
    let root = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    root.append(&perm);
    root.append(&overlay);
    root.set_vexpand(true);
    view(v).root = Widget::from(root).into_floating();
    build_search_bar(v, &overlay);
    ns_pv_input_install_pointer(v);
    ctx_install_actions(v);
    ns_pv_input_install_secondary(v);
    ns_pv_install_scroll(v);
    ns_pv_input_install_keys(v);
    area.connect_destroy(move || on_area_destroy(v));
    start_worker(v);
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
