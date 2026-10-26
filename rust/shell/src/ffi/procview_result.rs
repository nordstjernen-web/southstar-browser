//! Southstar — the tab view's main-loop side: renderer replies, loading and history, zoom, the permission bar and the view's accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_gtk_ffi as gtk;

use super::pagelayers::{ns_layer_update_free, ns_page_layers_reset};
use super::procview_chrome::{
    ns_pv_console_append, ns_pv_console_set_open, ns_pv_show_context_menu, print_run,
};
use super::procview_draw::{
    arm_anim, clear_busy_cursor, configure_adjustments, disarm_anim, layers,
    ns_pv_stop_wheel_animation, request_render, request_tick, scroll_view_to, set_busy_cursor,
    set_named_cursor, start_render, tiles_apply, tiles_leave,
};
use super::procview_input::{
    cur_scale, maybe_update_viewport, ns_pv_css_viewport_h, ns_pv_css_viewport_w, page_dpr,
    push_req, start_hover, start_link, start_select,
};
use super::procview_media::{
    ns_pv_media_append_stats, ns_pv_media_audio_pid, ns_pv_media_audio_shutdown, ns_pv_media_pump,
    ns_pv_media_video_pid, ns_pv_media_video_playing, ns_pv_media_video_shutdown,
};
use super::procview_state::{NsProcView, Req, Res};
use super::procview_view::ns_proc_view_unref;
use super::procview_worker::{
    RES_CLICK, RES_CONSOLE, RES_COPY, RES_DROPFILES, RES_DUMP, RES_EVAL, RES_EXPORT, RES_FAVICON,
    RES_FIND, RES_FRAME, RES_HOVER, RES_KEY, RES_LINK, RES_PAGE, RES_PRINT, RES_RELEASE,
    RES_SCROLLBAR, RES_SELECT, RES_TICK, RES_VIEWPORT, lock_proc, unlock_proc,
};
use crate::procview::nav;

const EVT_TITLE: c_int = 0;
const EVT_URL: c_int = 1;
const EVT_STATUS: c_int = 2;
const EVT_NEWTAB: c_int = 3;
const EVT_HISTORY: c_int = 4;
const EVT_LOADING: c_int = 5;
const EVT_DOWNLOAD: c_int = 6;
const EVT_FAVICON: c_int = 7;
const EVT_WEBGL: c_int = 8;
const EVT_FULLSCREEN: c_int = 9;
const EVT_ZOOM: c_int = 10;

const REQ_LOAD: c_int = 0;
const REQ_SCROLLBAR: c_int = 15;
const REQ_EVAL: c_int = 12;
const REQ_WEBGL: c_int = 16;
const REQ_CAMERA: c_int = 17;
const REQ_FAVICON: c_int = 19;
const REQ_VIDEO_EVENT: c_int = 20;

const ACT_HOVER: c_int = 0;
const ACT_NAVIGATE: c_int = 1;
const ACT_NEWTAB: c_int = 2;
const ACT_CONTEXT: c_int = 3;

const DEV_TAB_NETWORK: c_int = 1;
const DEV_TAB_PERFORMANCE: c_int = 2;
const DEV_TAB_LAYOUT: c_int = 3;
const DEV_TAB_ELEMENTS: c_int = 4;

#[repr(C)]
struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

unsafe extern "C" {
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_async_queue_push_front(queue: *mut c_void, data: *mut c_void);
    fn g_strcmp0(a: *const c_char, b: *const c_char) -> c_int;
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(string: *mut GString, free_segment: GBoolean) -> *mut c_char;
}

fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    unsafe { &mut *v }
}

fn tr(text: &CStr) -> *const c_char {
    southstar_i18n::translate(text).unwrap_or(text).as_ptr()
}

fn nonempty(s: *const c_char) -> bool {
    !s.is_null() && unsafe { *s } != 0
}

fn same(a: *const c_char, b: *const c_char) -> bool {
    unsafe { g_strcmp0(a, b) == 0 }
}

fn dup(s: *const c_char) -> *mut c_char {
    unsafe { glib::g_strdup(s) }
}

fn url_bytes<'a>(url: *const c_char) -> Option<&'a [u8]> {
    // SAFETY: every URL the view holds or receives is null or a NUL-terminated string it owns.
    unsafe { glib::bytes(url) }
}

fn is_local(url: *const c_char) -> bool {
    url_bytes(url).is_some_and(nav::is_local_url)
}

fn page_may_open(v: *mut NsProcView, url: *const c_char) -> bool {
    if !is_local(url) || view(v).local_page != 0 {
        return true;
    }
    post_emit(
        v,
        EVT_STATUS,
        tr(c"Blocked a web page from opening a local file"),
    );
    false
}

pub(super) fn ns_pv_page_may_open(v: *mut NsProcView, url: *const c_char) -> bool {
    page_may_open(v, url)
}

fn page_load(v: *mut NsProcView, url: *const c_char) {
    if page_may_open(v, url) {
        load(v, url);
    }
}

fn replace(slot: &mut *mut c_char, value: *mut c_char) {
    unsafe { glib::g_free((*slot).cast()) };
    *slot = value;
}

fn clear_object(slot: &mut *mut c_void) {
    if !slot.is_null() {
        unsafe { gtk::g_object_unref(*slot) };
        *slot = ptr::null_mut();
    }
}

fn history(v: *mut NsProcView) -> *mut GPtrArray {
    view(v).history.cast()
}

fn history_len(v: *mut NsProcView) -> c_int {
    unsafe { (*history(v)).len as c_int }
}

fn history_at(v: *mut NsProcView, index: c_int) -> *mut c_char {
    unsafe { *(*history(v)).pdata.add(index as usize) }.cast()
}

fn new_req(kind: c_int) -> *mut Req {
    let req = unsafe { g_malloc0(core::mem::size_of::<Req>()) }.cast::<Req>();
    unsafe { (*req).r#type = kind };
    req
}

pub(super) fn post_emit(v: *mut NsProcView, evt: c_int, text: *const c_char) {
    let state = view(v);
    if let Some(notify) = state.notify {
        unsafe { notify(v, evt, text, state.notify_ud) };
    }
}

fn finish_loading(v: *mut NsProcView) {
    let state = view(v);
    if state.loading != 0 {
        state.loading = 0;
        post_emit(v, EVT_LOADING, c"0".as_ptr());
    }
}

fn request_favicon(v: *mut NsProcView) {
    let req = new_req(REQ_FAVICON);
    unsafe { (*req).seq = view(v).load_seq };
    push_req(v, req);
}

fn push_history(v: *mut NsProcView, url: *const c_char) {
    if !nonempty(url) {
        return;
    }
    let index = view(v).hist_index;
    if index >= 0 && same(history_at(v, index), url) {
        return;
    }
    while history_len(v) > index + 1 {
        unsafe { glib::g_ptr_array_remove_index(history(v), (history_len(v) - 1) as u32) };
    }
    unsafe { glib::g_ptr_array_add(history(v), dup(url).cast()) };
    view(v).hist_index = history_len(v) - 1;
    post_emit(v, EVT_HISTORY, ptr::null());
}

fn follow_same_document_state(
    v: *mut NsProcView,
    url: *const c_char,
    pushed: bool,
    title: *const c_char,
) {
    let state = view(v);
    let accepted = url_bytes(url).is_some_and(|next| {
        nav::same_document_url_ok(url_bytes(state.current_url), next, state.local_page != 0)
    });
    if nonempty(url) && accepted && !same(url, view(v).current_url) {
        let index = view(v).hist_index;
        if pushed || index < 0 {
            push_history(v, url);
        } else {
            unsafe {
                let slot = (*history(v)).pdata.add(index as usize);
                glib::g_free(*slot);
                *slot = dup(url).cast();
            }
        }
        let state = view(v);
        replace(&mut state.current_url, dup(url));
        post_emit(v, EVT_URL, state.current_url);
    }
    if !title.is_null() && !same(title, view(v).current_title) {
        let state = view(v);
        replace(&mut state.current_title, dup(title));
        post_emit(v, EVT_TITLE, state.current_title);
    }
}

pub(super) fn do_load(
    v: *mut NsProcView,
    url: *const c_char,
    record: bool,
    history: bool,
    user_activated: bool,
) {
    if !nonempty(url) {
        return;
    }
    perm_resolve(v, false);
    ns_pv_media_audio_shutdown(v);
    ns_pv_media_video_shutdown(v);
    let state = view(v);
    state.pending_record = glib::boolean(record);
    state.pending_local = glib::boolean(is_local(url));
    state.load_seq += 1;
    let seq = state.load_seq;
    state.render_seq += 1;
    state.link_seq += 1;
    state.click_seq += 1;
    state.viewport_seq += 1;
    state.key_seq += 1;
    state.select_seq += 1;
    state.hover_seq += 1;
    state.render_pending = 0;
    state.render_inflight = 0;
    state.tick_pending = 0;
    state.tick_inflight = 0;
    state.link_inflight = 0;
    state.link_pending = 0;
    state.link_pending_action = ACT_HOVER;
    state.hover_inflight = 0;
    state.hover_pending = 0;
    state.has_selection = 0;
    state.find_seq += 1;
    unsafe {
        if !state.search_revealer.is_null() {
            gtk::gtk_revealer_set_reveal_child(state.search_revealer, 0);
        }
        if !state.search_label.is_null() {
            gtk::gtk_label_set_text(state.search_label, c"".as_ptr());
        }
    }
    state.opened = 0;
    state.page_animating = 0;
    state.caret_blinking = 0;
    ns_pv_stop_wheel_animation(v);
    disarm_anim(v);
    clear_object(&mut state.frame);
    ns_page_layers_reset(layers(v));
    state.tiles_mode = 0;
    unsafe { gtk::gtk_widget_queue_draw(state.area) };
    if state.loading == 0 {
        state.loading = 1;
        post_emit(v, EVT_LOADING, c"1".as_ptr());
    }
    set_busy_cursor(v);
    post_emit(v, EVT_STATUS, tr(c"Loading…"));

    let state = view(v);
    let vw = unsafe { gtk::gtk_widget_get_width(state.area) };
    let vh = unsafe { gtk::gtk_widget_get_height(state.area) };
    if vw <= 1 || vh <= 1 {
        replace(&mut state.deferred_url, dup(url));
        state.deferred_record = glib::boolean(record);
        state.deferred_history = glib::boolean(history);
        state.deferred_user_activated = glib::boolean(user_activated);
        return;
    }
    state.last_vp_w = ns_pv_css_viewport_w(v);
    state.last_vp_h = ns_pv_css_viewport_h(v);
    state.last_vp_dpr = page_dpr(v);

    let req = new_req(REQ_LOAD);
    unsafe {
        let r = &mut *req;
        r.seq = seq;
        r.url = dup(url);
        r.vw = state.last_vp_w;
        r.vh = state.last_vp_h;
        r.dpr = state.last_vp_dpr;
        r.history = glib::boolean(history);
        r.user_activated = glib::boolean(user_activated);
    }
    push_req(v, req);
}

fn load(v: *mut NsProcView, url: *const c_char) {
    view(v).render_restarts = 0;
    do_load(v, url, true, false, true);
}

fn can_back(v: *mut NsProcView) -> bool {
    view(v).hist_index > 0
}

fn can_forward(v: *mut NsProcView) -> bool {
    let index = view(v).hist_index;
    index >= 0 && index < history_len(v) - 1
}

fn step_history(v: *mut NsProcView, delta: c_int) {
    let state = view(v);
    state.hist_index += delta;
    state.render_restarts = 0;
    post_emit(v, EVT_HISTORY, ptr::null());
    do_load(v, history_at(v, view(v).hist_index), false, true, true);
}

fn set_zoom(v: *mut NsProcView, scale: f64) {
    let permille = nav::zoom_permille(scale);
    let clamped = f64::from(permille) / 1000.0;
    if clamped == cur_scale(v) {
        return;
    }
    view(v).scale = clamped;
    let percent = CString::new((permille / 10).to_string()).unwrap_or_default();
    post_emit(v, EVT_ZOOM, percent.as_ptr());
    if view(v).opened != 0 && !maybe_update_viewport(v) {
        configure_adjustments(v);
        request_render(v);
    }
}

pub(super) fn perm_resolve(v: *mut NsProcView, allow: bool) {
    let state = view(v);
    if state.perm_pending == 0 {
        return;
    }
    state.perm_pending = 0;
    let kind = state.perm_kind;
    let origin = state.perm_origin;
    state.perm_origin = ptr::null_mut();
    if !state.perm_revealer.is_null() {
        unsafe { gtk::gtk_revealer_set_reveal_child(state.perm_revealer, 0) };
    }
    if state.closed == 0 {
        let req = new_req(kind);
        unsafe {
            (*req).url = dup(origin);
            (*req).mods = c_int::from(allow);
        }
        push_req(v, req);
        if kind == REQ_WEBGL && allow && !view(v).current_url.is_null() {
            do_load(v, view(v).current_url, false, false, true);
        }
    }
    unsafe { glib::g_free(origin.cast()) };
}

fn perm_bar_show(v: *mut NsProcView, kind: c_int, origin: *const c_char) {
    let state = view(v);
    if state.perm_pending != 0 && state.perm_kind == kind && same(state.perm_origin, origin) {
        return;
    }
    if state.perm_pending != 0 {
        perm_resolve(v, false);
    }
    let state = view(v);
    state.perm_pending = 1;
    state.perm_kind = kind;
    state.perm_origin = dup(origin);
    let what = if kind == REQ_WEBGL {
        tr(c"This site wants to use WebGL (3D graphics)")
    } else {
        tr(c"This site wants to use your camera and microphone")
    };
    let mut text = unsafe { CStr::from_ptr(what) }.to_bytes().to_vec();
    text.extend_from_slice(" — ".as_bytes());
    text.extend_from_slice(unsafe { glib::bytes(origin) }.unwrap_or(b"(null)"));
    let text = CString::new(text).unwrap_or_default();
    unsafe {
        gtk::gtk_label_set_text(state.perm_label, text.as_ptr());
        gtk::gtk_revealer_set_reveal_child(state.perm_revealer, 1);
    }
}

fn apply_window_action(v: *mut NsProcView, action: *const c_char) {
    if v.is_null() || action.is_null() {
        return;
    }
    if unsafe { glib::bytes(action) }.is_some_and(nav::is_fullscreen_action) {
        post_emit(v, EVT_FULLSCREEN, action);
    }
}

fn adopt_page_size(v: *mut NsProcView, res: &Res) -> bool {
    let state = view(v);
    if res.ph > 0 && res.ph != state.page_h {
        state.page_h = res.ph;
        if res.pw > 0 {
            state.page_w = res.pw;
        }
        return true;
    }
    false
}

fn set_anim(v: *mut NsProcView) {
    let state = view(v);
    if state.page_animating != 0 || state.caret_blinking != 0 {
        arm_anim(v);
    } else {
        disarm_anim(v);
    }
}

fn copy_to_clipboard(v: *mut NsProcView, text: *const c_char, status: &CStr) {
    unsafe { gtk::gdk_clipboard_set_text(gtk::gtk_widget_get_clipboard(view(v).area), text) };
    post_emit(v, EVT_STATUS, tr(status));
}

fn side_channels(v: *mut NsProcView, res: &Res) {
    if nonempty(res.webgl) {
        post_emit(v, EVT_WEBGL, c"1".as_ptr());
    }
    if nonempty(res.camera) {
        perm_bar_show(v, REQ_CAMERA, res.camera);
    }
    if nonempty(res.download) {
        post_emit(v, EVT_DOWNLOAD, res.download);
    }
    if nonempty(res.audio) {
        ns_pv_media_pump(v, res.audio);
    }
    if nonempty(res.window_action) {
        apply_window_action(v, res.window_action);
    }
}

fn redirect(v: *mut NsProcView, nav_url: *const c_char) -> bool {
    let state = view(v);
    if nonempty(nav_url) && state.js_redirects < nav::MAX_JS_REDIRECTS && page_may_open(v, nav_url)
    {
        state.js_redirects += 1;
        do_load(v, nav_url, false, false, false);
        return true;
    }
    false
}

fn on_page(v: *mut NsProcView, res: &Res) {
    if res.seq != view(v).load_seq {
        return;
    }
    if res.ok == 0 {
        post_emit(v, EVT_STATUS, tr(c"Failed to load page"));
        finish_loading(v);
        clear_busy_cursor(v);
        return;
    }
    if nonempty(res.nav) && page_may_open(v, res.nav) {
        let state = view(v);
        if state.js_redirects < nav::MAX_JS_REDIRECTS {
            state.js_redirects += 1;
            let record = state.pending_record != 0;
            do_load(v, res.nav, record, false, false);
            return;
        }
        post_emit(v, EVT_STATUS, tr(c"Stopped after too many redirects"));
    }
    let state = view(v);
    if is_local(res.url) && state.pending_local == 0 {
        post_emit(
            v,
            EVT_STATUS,
            tr(c"Blocked a web page from opening a local file"),
        );
        finish_loading(v);
        clear_busy_cursor(v);
        return;
    }
    state.local_page = glib::boolean(is_local(res.url));
    state.js_redirects = 0;
    replace(&mut state.current_url, dup(res.url));
    replace(&mut state.current_title, dup(res.title));
    state.security = res.security;
    replace(
        &mut state.remote_ip,
        if res.remote_ip.is_null() {
            ptr::null_mut()
        } else {
            dup(res.remote_ip)
        },
    );
    state.page_w = res.pw;
    state.page_h = res.ph;
    state.scroll_x = 0;
    state.scroll_y = 0;
    state.opened = 1;
    configure_adjustments(v);
    let state = view(v);
    if state.pending_record != 0 {
        push_history(v, state.current_url);
    }
    let state = view(v);
    post_emit(v, EVT_URL, state.current_url);
    post_emit(v, EVT_TITLE, state.current_title);
    post_emit(v, EVT_STATUS, c"".as_ptr());
    finish_loading(v);
    request_render(v);
    request_favicon(v);
}

fn on_tick(v: *mut NsProcView, res: &Res) {
    let current = res.seq == view(v).load_seq && res.ok != 0;
    let mut navigated = false;
    if current {
        navigated = redirect(v, res.nav);
        side_channels(v, res);
        if !navigated {
            follow_same_document_state(v, res.url, res.url_pushed != 0, res.title);
        }
        view(v).page_animating = res.animating;
        set_anim(v);
        if adopt_page_size(v, res) {
            configure_adjustments(v);
        }
        if res.kind != 0 && !ns_pv_media_video_playing(v) {
            request_render(v);
        }
    }
    let state = view(v);
    state.tick_inflight = 0;
    if !navigated && state.tick_pending != 0 && state.opened != 0 {
        state.tick_pending = 0;
        request_tick(v);
    }
}

fn on_frame(v: *mut NsProcView, res: &mut Res) {
    let current = res.seq == view(v).render_seq;
    if res.ok != 0 && res.wheel_snapped != 0 {
        ns_pv_stop_wheel_animation(v);
    }
    if current && res.ok != 0 {
        let state = view(v);
        state.page_animating = res.animating;
        state.caret_blinking = res.caret_blinking;
        set_anim(v);
        if adopt_page_size(v, res) {
            unsafe { gtk::gtk_widget_queue_draw(view(v).area) };
        }
        if res.requested_scroll_y >= 0 || res.requested_scroll_x >= 0 {
            view(v).adopting_scroll = 1;
            configure_adjustments(v);
            let state = view(v);
            unsafe {
                if res.requested_scroll_y >= 0 {
                    gtk::gtk_adjustment_set_value(
                        state.vadj.cast(),
                        f64::from(res.requested_scroll_y),
                    );
                }
                if res.requested_scroll_x >= 0 {
                    gtk::gtk_adjustment_set_value(
                        state.hadj.cast(),
                        f64::from(res.requested_scroll_x),
                    );
                }
            }
            let state = view(v);
            state.adopting_scroll = 0;
            unsafe {
                state.scroll_x = gtk::gtk_adjustment_get_value(state.hadj.cast()) as c_int;
                state.scroll_y = gtk::gtk_adjustment_get_value(state.vadj.cast()) as c_int;
            }
        }
    }
    if current && res.ok != 0 && !res.layers.is_null() {
        tiles_apply(v, res);
    } else if current && res.ok != 0 && !res.texture.is_null() {
        tiles_leave(v);
        let state = view(v);
        clear_object(&mut state.frame);
        state.frame = core::mem::replace(&mut res.texture, ptr::null_mut());
        state.frame_scale = res.texture_scale;
        state.render_restarts = 0;
        unsafe { gtk::gtk_widget_queue_draw(state.area) };
        clear_busy_cursor(v);
    } else if current && res.ok != 0 && res.frame_unchanged != 0 {
        view(v).render_restarts = 0;
    }
    if current && res.ok != 0 {
        redirect(v, res.nav);
    }
    if res.ok != 0 {
        side_channels(v, res);
        if !res.clipboard.is_null() && !view(v).area.is_null() {
            copy_to_clipboard(v, res.clipboard, c"Copied to clipboard");
        }
    }
    let state = view(v);
    state.render_inflight = 0;
    if state.render_pending != 0 {
        state.render_pending = 0;
        start_render(v);
    } else if current && res.ok == 0 && !state.current_url.is_null() {
        if state.render_restarts < nav::MAX_RESTARTS {
            state.render_restarts += 1;
            post_emit(v, EVT_STATUS, tr(c"Renderer restarted"));
            do_load(v, view(v).current_url, false, false, false);
        } else {
            post_emit(
                v,
                EVT_STATUS,
                tr(c"This tab's renderer keeps failing — reload to retry"),
            );
            finish_loading(v);
            clear_busy_cursor(v);
        }
    }
}

fn pending_link(v: *mut NsProcView) {
    let state = view(v);
    if state.link_pending != 0 {
        state.link_pending = 0;
        let action = state.link_pending_action;
        state.link_pending_action = ACT_HOVER;
        start_link(v, state.link_pending_x, state.link_pending_y, action);
    }
}

fn show_link_status(v: *mut NsProcView, href: *const c_char, cursor: *const c_char) {
    let cursor = unsafe { glib::bytes(cursor) };
    if nonempty(href) {
        post_emit(v, EVT_STATUS, href);
        if view(v).busy_cursor == 0 {
            set_named_cursor(view(v).area, cursor.unwrap_or(b"pointer"));
        }
    } else {
        post_emit(v, EVT_STATUS, c"".as_ptr());
        if view(v).busy_cursor == 0 {
            set_named_cursor(view(v).area, cursor.unwrap_or_default());
        }
    }
}

fn on_link(v: *mut NsProcView, res: &Res) {
    if res.seq != view(v).link_seq {
        return;
    }
    view(v).link_inflight = 0;
    if res.action == ACT_CONTEXT {
        if res.prevented != 0 || res.edit != 0 {
            request_render(v);
        }
        if res.prevented == 0 {
            ns_pv_show_context_menu(v, res.href, res.edit);
        }
        pending_link(v);
        return;
    }
    let mut navigated = false;
    show_link_status(v, res.href, res.cursor);
    if nonempty(res.href) {
        if res.action == ACT_NAVIGATE {
            navigated = true;
            page_load(v, res.href);
        } else if res.action == ACT_NEWTAB && page_may_open(v, res.href) {
            post_emit(v, EVT_NEWTAB, res.href);
        }
    }
    if !navigated {
        pending_link(v);
    }
}

fn on_hover(v: *mut NsProcView, res: &Res) {
    if res.seq != view(v).hover_seq {
        return;
    }
    view(v).hover_inflight = 0;
    show_link_status(v, res.href, res.cursor);
    if res.ok != 0 {
        request_render(v);
    }
    let state = view(v);
    if state.hover_pending != 0 {
        state.hover_pending = 0;
        start_hover(v, state.hover_pending_x, state.hover_pending_y);
    }
}

fn on_scrollbar(v: *mut NsProcView, res: &Res) {
    if res.kind == 0 {
        let state = view(v);
        let still = state.sb_probe != 0;
        state.sb_probe = 0;
        if res.ok != 0 {
            request_render(v);
            let state = view(v);
            if still {
                state.sb_dragging = 1;
                if state.sb_have_last != 0 {
                    let s = cur_scale(v);
                    let req = new_req(REQ_SCROLLBAR);
                    unsafe {
                        (*req).kind = 1;
                        (*req).x = state.scroll_x + (state.sb_last_x / s) as c_int;
                        (*req).y = state.scroll_y + (state.sb_last_y / s) as c_int;
                    }
                    push_req(v, req);
                }
            }
        } else if still && state.sb_have_last != 0 {
            let s = cur_scale(v);
            if state.multi_click == 0 {
                start_select(
                    v,
                    0,
                    state.scroll_x + (state.drag_start_x / s) as c_int,
                    state.scroll_y + (state.drag_start_y / s) as c_int,
                );
                view(v).drag_anchored = 1;
            }
            let state = view(v);
            start_select(
                v,
                1,
                state.scroll_x + (state.sb_last_x / s) as c_int,
                state.scroll_y + (state.sb_last_y / s) as c_int,
            );
            view(v).has_selection = 1;
        }
    } else if res.kind == 1 && res.ok != 0 {
        request_render(v);
    }
}

fn on_find(v: *mut NsProcView, res: &Res) {
    let state = view(v);
    if res.seq != state.find_seq {
        return;
    }
    if !state.search_label.is_null() {
        let query = if state.search_entry.is_null() {
            ptr::null()
        } else {
            unsafe { gtk::gtk_editable_get_text(state.search_entry) }
        };
        if res.find_total > 0 {
            let label =
                CString::new(nav::find_count(res.find_current, res.find_total)).unwrap_or_default();
            unsafe { gtk::gtk_label_set_text(state.search_label, label.as_ptr()) };
        } else {
            let text = if nonempty(query) {
                tr(c"No results")
            } else {
                c"".as_ptr()
            };
            unsafe { gtk::gtk_label_set_text(state.search_label, text) };
        }
    }
    if res.find_total > 0 {
        unsafe {
            gtk::gtk_adjustment_set_value(
                state.vadj.cast(),
                nav::find_scroll_target(res.find_scroll_y),
            )
        };
    }
    request_render(v);
}

fn on_eval(v: *mut NsProcView, res: &Res) {
    let state = view(v);
    if res.inspect != 0 {
        if !state.elements_buffer.is_null() {
            let text = if nonempty(res.href) {
                res.href.cast_const()
            } else {
                tr(c"No matching element")
            };
            unsafe { gtk::gtk_text_buffer_set_text(state.elements_buffer, text, -1) };
        }
    } else if nonempty(res.href) {
        ns_pv_console_append(v, res.href);
        ns_pv_console_append(v, c"\n".as_ptr());
    } else {
        ns_pv_console_append(v, c"undefined\n".as_ptr());
    }
    request_render(v);
}

fn on_dump(v: *mut NsProcView, res: &Res) {
    let state = view(v);
    let buf = match res.dump_tab {
        DEV_TAB_NETWORK => state.net_buffer,
        DEV_TAB_PERFORMANCE => state.perf_buffer,
        DEV_TAB_LAYOUT => state.layout_buffer,
        DEV_TAB_ELEMENTS => state.elements_buffer,
        _ => ptr::null_mut(),
    };
    let text = if nonempty(res.href) {
        res.href.cast_const()
    } else {
        tr(c"(empty)")
    };
    if buf == state.perf_buffer {
        unsafe {
            let out = g_string_new(text);
            ns_pv_media_append_stats(v, out.cast());
            gtk::gtk_text_buffer_set_text(buf, (*out).str, -1);
            g_string_free(out, 1);
        }
    } else if !buf.is_null() {
        unsafe { gtk::gtk_text_buffer_set_text(buf, text, -1) };
    }
}

fn on_favicon(v: *mut NsProcView, res: &Res) {
    let state = view(v);
    if res.seq != state.load_seq {
        return;
    }
    clear_object(&mut state.favicon);
    if !res.favicon_bytes.is_null() && res.favicon_w > 0 && res.favicon_h > 0 {
        // SAFETY: favicon_bytes is the reply's own GBytes holding at least stride * height bytes, which the texture takes a reference to.
        let tex = unsafe {
            gtk::gdk_memory_texture_new(
                res.favicon_w,
                res.favicon_h,
                gtk::MEMORY_DEFAULT,
                res.favicon_bytes,
                res.favicon_stride as usize,
            )
        };
        state.favicon = tex.cast();
    }
    post_emit(v, EVT_FAVICON, ptr::null());
}

fn dispatch(v: *mut NsProcView, res: &mut Res) {
    match res.r#type {
        RES_PAGE => on_page(v, res),
        RES_TICK => on_tick(v, res),
        RES_FRAME => on_frame(v, res),
        RES_VIEWPORT => {
            if res.seq == view(v).viewport_seq && res.ok != 0 {
                let state = view(v);
                state.page_w = res.pw;
                state.page_h = res.ph;
                configure_adjustments(v);
                request_render(v);
            }
        }
        RES_SELECT => {
            if res.seq == view(v).select_seq {
                request_render(v);
            }
        }
        RES_COPY => {
            if nonempty(res.href) && !view(v).area.is_null() {
                copy_to_clipboard(v, res.href, c"Copied selection");
            }
            if res.kind == 7 {
                request_render(v);
            }
        }
        RES_KEY => {
            if res.seq != view(v).key_seq {
                return;
            }
            if res.kind == 0 && nonempty(res.href) {
                post_emit(v, EVT_STATUS, res.href);
                page_load(v, res.href);
            } else {
                if res.fallback_scroll != 0 && res.prevented == 0 {
                    scroll_view_to(v, res.fallback_x, res.fallback_y);
                }
                request_render(v);
            }
        }
        RES_CLICK => {
            if res.seq != view(v).click_seq {
                return;
            }
            if nonempty(res.href) {
                post_emit(v, EVT_STATUS, res.href);
                page_load(v, res.href);
            } else {
                request_render(v);
            }
        }
        RES_LINK => on_link(v, res),
        RES_HOVER => on_hover(v, res),
        RES_DROPFILES => {
            if res.ok != 0 {
                request_render(v);
            }
        }
        RES_SCROLLBAR => on_scrollbar(v, res),
        RES_RELEASE => {
            if nonempty(res.href) {
                post_emit(v, EVT_STATUS, res.href);
                page_load(v, res.href);
            } else if res.ok != 0 {
                request_render(v);
            }
        }
        RES_FIND => on_find(v, res),
        RES_EXPORT => {
            if res.ok != 0 && !res.url.is_null() {
                post_emit(v, EVT_STATUS, res.url);
            } else {
                post_emit(v, EVT_STATUS, tr(c"Could not save page"));
            }
        }
        RES_PRINT => {
            let pages = res.print_pages.cast::<GPtrArray>();
            if !pages.is_null() && unsafe { (*pages).len } > 0 {
                print_run(v, pages, &res.print_setup, res.print_scale);
                res.print_pages = ptr::null_mut();
            } else {
                post_emit(v, EVT_STATUS, tr(c"Nothing to print"));
            }
            request_render(v);
        }
        RES_CONSOLE => {
            if nonempty(res.href) {
                ns_pv_console_append(v, res.href);
            }
        }
        RES_EVAL => on_eval(v, res),
        RES_DUMP => on_dump(v, res),
        RES_FAVICON => on_favicon(v, res),
        _ => {}
    }
}

fn free_res(res: *mut Res) {
    unsafe {
        let r = &mut *res;
        clear_object(&mut r.texture);
        ns_layer_update_free(r.layers.cast());
        for s in [
            r.title,
            r.url,
            r.nav,
            r.remote_ip,
            r.webgl,
            r.camera,
            r.download,
            r.audio,
        ] {
            glib::g_free(s.cast());
        }
        glib::g_free(r.clipboard.cast());
        glib::g_free(r.window_action.cast());
        glib::g_free(r.href.cast());
        glib::g_free(r.cursor.cast());
        glib::g_free(r.media_url.cast());
        if !r.favicon_bytes.is_null() {
            gtk::g_bytes_unref(r.favicon_bytes);
        }
        ns_proc_view_unref(r.view);
        glib::g_free(res.cast());
    }
}

pub(super) unsafe extern "C" fn on_result(data: *mut c_void) -> GBoolean {
    let res = data.cast::<Res>();
    let v = unsafe { (*res).view };
    if view(v).closed == 0 {
        dispatch(v, unsafe { &mut *res });
    }
    free_res(res);
    gtk::SOURCE_REMOVE
}

pub(super) fn ns_proc_view_stop(v: *mut NsProcView) {
    if v.is_null() {
        return;
    }
    let state = view(v);
    state.render_seq += 1;
    state.render_inflight = 0;
    state.render_pending = 0;
    finish_loading(v);
    clear_busy_cursor(v);
}

pub(super) fn ns_proc_view_push_video_event(v: *mut NsProcView, token: &CStr, kind: &CStr) {
    let req = new_req(REQ_VIDEO_EVENT);
    unsafe {
        (*req).seq = view(v).load_seq;
        (*req).key = dup(token.as_ptr());
        (*req).query = dup(kind.as_ptr());
        g_async_queue_push_front(view(v).queue, req.cast());
    }
}

pub(super) fn ns_proc_view_load(v: *mut NsProcView, url: *const c_char) {
    load(v, url);
}

pub(super) fn ns_proc_view_can_back(v: *mut NsProcView) -> bool {
    can_back(v)
}

pub(super) fn ns_proc_view_can_forward(v: *mut NsProcView) -> bool {
    can_forward(v)
}

pub(super) fn ns_proc_view_back(v: *mut NsProcView) {
    if can_back(v) {
        step_history(v, -1);
    }
}

pub(super) fn ns_proc_view_forward(v: *mut NsProcView) {
    if can_forward(v) {
        step_history(v, 1);
    }
}

pub(super) fn ns_proc_view_reload(v: *mut NsProcView) {
    let state = view(v);
    state.render_restarts = 0;
    if state.hist_index >= 0 && state.hist_index < history_len(v) {
        do_load(v, history_at(v, state.hist_index), false, false, true);
    } else if !state.current_url.is_null() {
        do_load(v, state.current_url, false, false, true);
    }
}

pub(super) fn ns_proc_view_toggle_console(v: *mut NsProcView) {
    let state = view(v);
    if state.opened != 0 {
        ns_pv_console_set_open(v, state.console_open == 0);
    }
}

pub(super) fn ns_proc_view_exit_fullscreen(v: *mut NsProcView) {
    if v.is_null() || view(v).closed != 0 || view(v).opened == 0 {
        return;
    }
    let req = new_req(REQ_EVAL);
    unsafe { (*req).query = dup(c"document.exitFullscreen()".as_ptr()) };
    push_req(v, req);
}

pub(super) fn ns_proc_view_url(v: *mut NsProcView) -> *const c_char {
    view(v).current_url
}

pub(super) fn ns_proc_view_local_page(v: *mut NsProcView) -> bool {
    !v.is_null() && view(v).local_page != 0
}

pub(super) fn ns_proc_view_title(v: *mut NsProcView) -> *const c_char {
    view(v).current_title
}

pub(super) fn ns_proc_view_security(v: *mut NsProcView) -> c_int {
    if v.is_null() { 0 } else { view(v).security }
}

pub(super) fn ns_proc_view_remote_ip(v: *mut NsProcView) -> *const c_char {
    if v.is_null() {
        ptr::null()
    } else {
        view(v).remote_ip
    }
}

pub(super) fn ns_proc_view_is_loading(v: *mut NsProcView) -> bool {
    view(v).loading != 0
}

pub(super) fn ns_proc_view_favicon(v: *mut NsProcView) -> *mut c_void {
    if v.is_null() {
        ptr::null_mut()
    } else {
        view(v).favicon
    }
}

pub(super) fn ns_proc_view_renderer_pid(v: *mut NsProcView) -> c_int {
    if v.is_null() {
        return -1;
    }
    lock_proc(v);
    let pid = view(v).remote.map_or(-1, |remote| remote.pid());
    unlock_proc(v);
    pid
}

pub(super) fn ns_proc_view_audio_pid(v: *mut NsProcView) -> c_int {
    if v.is_null() {
        -1
    } else {
        ns_pv_media_audio_pid(v)
    }
}

pub(super) fn ns_proc_view_video_pid(v: *mut NsProcView) -> c_int {
    if v.is_null() {
        -1
    } else {
        ns_pv_media_video_pid(v)
    }
}

pub(super) fn ns_proc_view_end_task(v: *mut NsProcView) {
    if v.is_null() {
        return;
    }
    lock_proc(v);
    if let Some(remote) = view(v).remote {
        remote.interrupt();
        remote.terminate();
    }
    unlock_proc(v);
}

pub(super) fn ns_proc_view_stop_video(v: *mut NsProcView) {
    if !v.is_null() {
        ns_pv_media_video_shutdown(v);
    }
}

pub(super) fn ns_proc_view_stop_audio(v: *mut NsProcView) {
    if !v.is_null() {
        ns_pv_media_audio_shutdown(v);
    }
}

pub(super) fn ns_proc_view_focus(v: *mut NsProcView) {
    let area = view(v).area;
    if !area.is_null() {
        unsafe { gtk::gtk_widget_grab_focus(area) };
    }
}

pub(super) fn ns_proc_view_zoom_in(v: *mut NsProcView) {
    set_zoom(v, nav::zoom_step(cur_scale(v), 1));
}

pub(super) fn ns_proc_view_zoom_out(v: *mut NsProcView) {
    set_zoom(v, nav::zoom_step(cur_scale(v), -1));
}

pub(super) fn ns_proc_view_zoom_reset(v: *mut NsProcView) {
    set_zoom(v, 1.0);
}

pub(super) fn ns_proc_view_zoom_percent(v: *mut NsProcView) -> c_int {
    if v.is_null() {
        100
    } else {
        nav::zoom_percent(cur_scale(v))
    }
}
