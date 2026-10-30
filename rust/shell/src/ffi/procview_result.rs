//! Southstar — the tab view's main-loop side: renderer replies, loading and history, zoom, the permission bar and the view's accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk_ffi as gtk;

use super::gtk::{cstring, tr};
use super::pagelayers::{ns_layer_update_free, ns_page_layers_reset};
use super::procview_chrome::{
    ns_pv_console_append, ns_pv_console_set_open, ns_pv_console_text, ns_pv_show_context_menu,
    print_run,
};
use super::procview_draw::{
    arm_anim, clear_busy_cursor, configure_adjustments, disarm_anim, layers,
    ns_pv_stop_wheel_animation, request_render, request_tick, scroll_view_to, set_busy_cursor,
    set_named_cursor, start_render, tiles_apply, tiles_leave,
};
use super::procview_input::{
    area, cur_scale, hadj, maybe_update_viewport, new_req, ns_pv_css_viewport_h,
    ns_pv_css_viewport_w, page_dpr, push_req, push_req_front, start_hover, start_link,
    start_select, vadj, view,
};
use super::procview_media::{
    ns_pv_media_audio_pid, ns_pv_media_audio_shutdown, ns_pv_media_pump, ns_pv_media_stats,
    ns_pv_media_video_pid, ns_pv_media_video_playing, ns_pv_media_video_shutdown,
};
use super::procview_state::{NsProcView, Res};
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

fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    // SAFETY: every string a reply carries is null or NUL-terminated and owned by the reply, which outlives its dispatch.
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

fn filled<'a>(s: *const c_char) -> Option<&'a CStr> {
    text(s).filter(|s| !s.is_empty())
}

fn is_local(url: &CStr) -> bool {
    nav::is_local_url(url.to_bytes())
}

fn page_may_open(v: *mut NsProcView, url: &CStr) -> bool {
    if !is_local(url) || view(v).local_page != 0 {
        return true;
    }
    post_emit(
        v,
        EVT_STATUS,
        Some(tr(c"Blocked a web page from opening a local file")),
    );
    false
}

pub(super) fn ns_pv_page_may_open(v: *mut NsProcView, url: &CStr) -> bool {
    page_may_open(v, url)
}

fn page_load(v: *mut NsProcView, url: &CStr) {
    if page_may_open(v, url) {
        load(v, url);
    }
}

fn clear_object(slot: &mut *mut c_void) {
    if !slot.is_null() {
        // SAFETY: slot holds the one reference the view owns to this object, dropped once here.
        unsafe { gtk::g_object_unref(*slot) };
        *slot = ptr::null_mut();
    }
}

fn history_len(v: *mut NsProcView) -> c_int {
    c_int::try_from(view(v).ui().history.len()).unwrap_or(c_int::MAX)
}

fn history_at(v: *mut NsProcView, index: c_int) -> Option<CString> {
    let index = usize::try_from(index).ok()?;
    view(v).ui().history.get(index).cloned()
}

fn current_url(v: *mut NsProcView) -> Option<CString> {
    view(v).ui().current_url.clone()
}

pub(super) fn post_emit(v: *mut NsProcView, evt: c_int, text: Option<&CStr>) {
    let state = view(v);
    if let Some(notify) = state.notify {
        let text = text.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: the window registered notify for this view with notify_ud as its data; text is null or NUL-terminated for the call.
        unsafe { notify(v, evt, text, state.notify_ud) };
    }
}

fn finish_loading(v: *mut NsProcView) {
    let state = view(v);
    if state.loading != 0 {
        state.loading = 0;
        post_emit(v, EVT_LOADING, Some(c"0"));
    }
}

fn request_favicon(v: *mut NsProcView) {
    let seq = view(v).load_seq;
    push_req(v, new_req(REQ_FAVICON, |r| r.seq = seq));
}

fn push_history(v: *mut NsProcView, url: &CStr) {
    if url.is_empty() {
        return;
    }
    let index = view(v).hist_index;
    let ui = view(v).ui();
    if index >= 0 && ui.history.get(index as usize).map(CString::as_c_str) == Some(url) {
        return;
    }
    ui.history.truncate(usize::try_from(index + 1).unwrap_or(0));
    ui.history.push(url.to_owned());
    view(v).hist_index = history_len(v) - 1;
    post_emit(v, EVT_HISTORY, None);
}

fn follow_same_document_state(
    v: *mut NsProcView,
    url: Option<&CStr>,
    pushed: bool,
    title: Option<&CStr>,
) {
    let local_page = view(v).local_page != 0;
    let current = current_url(v);
    let accepted = url.is_some_and(|next| {
        nav::same_document_url_ok(
            current.as_deref().map(CStr::to_bytes),
            next.to_bytes(),
            local_page,
        )
    });
    if let Some(url) = url.filter(|url| !url.is_empty())
        && accepted
        && current.as_deref() != Some(url)
    {
        let index = view(v).hist_index;
        if pushed || index < 0 {
            push_history(v, url);
        } else if let Some(slot) = view(v).ui().history.get_mut(index as usize) {
            *slot = url.to_owned();
        }
        view(v).ui().current_url = Some(url.to_owned());
        post_emit(v, EVT_URL, Some(url));
    }
    if let Some(title) = title
        && view(v).ui().current_title.as_deref() != Some(title)
    {
        view(v).ui().current_title = Some(title.to_owned());
        post_emit(v, EVT_TITLE, Some(title));
    }
}

pub(super) fn do_load(
    v: *mut NsProcView,
    url: &CStr,
    record: bool,
    history: bool,
    user_activated: bool,
) {
    if url.is_empty() {
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
    if let Some(find) = state.ui().find.as_ref() {
        let (revealer, label) = (find.revealer.clone(), find.label.clone());
        revealer.set_reveal_child(false);
        label.set_text(c"");
    }
    let state = view(v);
    state.opened = 0;
    state.page_animating = 0;
    state.caret_blinking = 0;
    ns_pv_stop_wheel_animation(v);
    disarm_anim(v);
    let state = view(v);
    clear_object(&mut state.frame);
    ns_page_layers_reset(layers(v));
    view(v).tiles_mode = 0;
    let area = area(v);
    if let Some(area) = &area {
        area.queue_draw();
    }
    let state = view(v);
    if state.loading == 0 {
        state.loading = 1;
        post_emit(v, EVT_LOADING, Some(c"1"));
    }
    set_busy_cursor(v);
    post_emit(v, EVT_STATUS, Some(tr(c"Loading…")));

    let vw = area.as_ref().map_or(0, |area| area.width());
    let vh = area.as_ref().map_or(0, |area| area.height());
    let state = view(v);
    if vw <= 1 || vh <= 1 {
        state.ui().deferred_url = Some(url.to_owned());
        state.deferred_record = glib::boolean(record);
        state.deferred_history = glib::boolean(history);
        state.deferred_user_activated = glib::boolean(user_activated);
        return;
    }
    state.last_vp_w = ns_pv_css_viewport_w(v);
    state.last_vp_h = ns_pv_css_viewport_h(v);
    state.last_vp_dpr = page_dpr(v);

    let (vw, vh, dpr) = (state.last_vp_w, state.last_vp_h, state.last_vp_dpr);
    push_req(
        v,
        new_req(REQ_LOAD, |r| {
            r.seq = seq;
            r.url = glib::strdup(url.to_bytes());
            r.vw = vw;
            r.vh = vh;
            r.dpr = dpr;
            r.history = glib::boolean(history);
            r.user_activated = glib::boolean(user_activated);
        }),
    );
}

fn load(v: *mut NsProcView, url: &CStr) {
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
    post_emit(v, EVT_HISTORY, None);
    if let Some(url) = history_at(v, view(v).hist_index) {
        do_load(v, &url, false, true, true);
    }
}

fn set_zoom(v: *mut NsProcView, scale: f64) {
    let permille = nav::zoom_permille(scale);
    let clamped = f64::from(permille) / 1000.0;
    if clamped == cur_scale(v) {
        return;
    }
    view(v).scale = clamped;
    let percent = cstring((permille / 10).to_string());
    post_emit(v, EVT_ZOOM, Some(&percent));
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
    let origin = state.ui().perm_origin.take();
    if let Some(perm) = state.ui().perm.as_ref() {
        let revealer = perm.revealer.clone();
        revealer.set_reveal_child(false);
    }
    if view(v).closed == 0 {
        push_req(
            v,
            new_req(kind, |r| {
                r.url = origin
                    .as_deref()
                    .map_or(ptr::null_mut(), |o| glib::strdup(o.to_bytes()));
                r.mods = c_int::from(allow);
            }),
        );
        if kind == REQ_WEBGL
            && allow
            && let Some(url) = current_url(v)
        {
            do_load(v, &url, false, false, true);
        }
    }
}

fn perm_bar_show(v: *mut NsProcView, kind: c_int, origin: &CStr) {
    let state = view(v);
    if state.perm_pending != 0
        && state.perm_kind == kind
        && state.ui().perm_origin.as_deref() == Some(origin)
    {
        return;
    }
    if state.perm_pending != 0 {
        perm_resolve(v, false);
    }
    let state = view(v);
    state.perm_pending = 1;
    state.perm_kind = kind;
    state.ui().perm_origin = Some(origin.to_owned());
    let what = if kind == REQ_WEBGL {
        tr(c"This site wants to use WebGL (3D graphics)")
    } else {
        tr(c"This site wants to use your camera and microphone")
    };
    let mut message = what.to_bytes().to_vec();
    message.extend_from_slice(" — ".as_bytes());
    message.extend_from_slice(origin.to_bytes());
    let message = cstring(message);
    if let Some(perm) = view(v).ui().perm.as_ref() {
        let (revealer, label) = (perm.revealer.clone(), perm.label.clone());
        label.set_text(&message);
        revealer.set_reveal_child(true);
    }
}

fn apply_window_action(v: *mut NsProcView, action: &CStr) {
    if nav::is_fullscreen_action(action.to_bytes()) {
        post_emit(v, EVT_FULLSCREEN, Some(action));
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

fn copy_to_clipboard(v: *mut NsProcView, text: &CStr, status: &CStr) {
    if let Some(area) = area(v) {
        area.set_clipboard_text(text);
    }
    post_emit(v, EVT_STATUS, Some(tr(status)));
}

fn side_channels(v: *mut NsProcView, res: &Res) {
    if filled(res.webgl).is_some() {
        post_emit(v, EVT_WEBGL, Some(c"1"));
    }
    if let Some(camera) = filled(res.camera) {
        perm_bar_show(v, REQ_CAMERA, camera);
    }
    if let Some(download) = filled(res.download) {
        post_emit(v, EVT_DOWNLOAD, Some(download));
    }
    if filled(res.audio).is_some() {
        ns_pv_media_pump(v, res.audio);
    }
    if let Some(action) = filled(res.window_action) {
        apply_window_action(v, action);
    }
}

fn redirect(v: *mut NsProcView, nav_url: Option<&CStr>) -> bool {
    if let Some(nav_url) = nav_url.filter(|url| !url.is_empty())
        && view(v).js_redirects < nav::MAX_JS_REDIRECTS
        && page_may_open(v, nav_url)
    {
        view(v).js_redirects += 1;
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
        post_emit(v, EVT_STATUS, Some(tr(c"Failed to load page")));
        finish_loading(v);
        clear_busy_cursor(v);
        return;
    }
    if let Some(nav_url) = filled(res.nav)
        && page_may_open(v, nav_url)
    {
        let state = view(v);
        if state.js_redirects < nav::MAX_JS_REDIRECTS {
            state.js_redirects += 1;
            let record = state.pending_record != 0;
            do_load(v, nav_url, record, false, false);
            return;
        }
        post_emit(v, EVT_STATUS, Some(tr(c"Stopped after too many redirects")));
    }
    let local = text(res.url).is_some_and(is_local);
    let state = view(v);
    if local && state.pending_local == 0 {
        post_emit(
            v,
            EVT_STATUS,
            Some(tr(c"Blocked a web page from opening a local file")),
        );
        finish_loading(v);
        clear_busy_cursor(v);
        return;
    }
    state.local_page = glib::boolean(local);
    state.js_redirects = 0;
    let ui = state.ui();
    ui.current_url = text(res.url).map(CStr::to_owned);
    ui.current_title = text(res.title).map(CStr::to_owned);
    ui.remote_ip = text(res.remote_ip).map(CStr::to_owned);
    state.security = res.security;
    state.page_w = res.pw;
    state.page_h = res.ph;
    state.scroll_x = 0;
    state.scroll_y = 0;
    state.opened = 1;
    configure_adjustments(v);
    let url = current_url(v);
    if view(v).pending_record != 0
        && let Some(url) = &url
    {
        push_history(v, url);
    }
    let title = view(v).ui().current_title.clone();
    post_emit(v, EVT_URL, url.as_deref());
    post_emit(v, EVT_TITLE, title.as_deref());
    post_emit(v, EVT_STATUS, Some(c""));
    finish_loading(v);
    request_render(v);
    request_favicon(v);
}

fn on_tick(v: *mut NsProcView, res: &Res) {
    let current = res.seq == view(v).load_seq && res.ok != 0;
    let mut navigated = false;
    if current {
        navigated = redirect(v, filled(res.nav));
        side_channels(v, res);
        if !navigated {
            follow_same_document_state(v, text(res.url), res.url_pushed != 0, text(res.title));
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

fn adopt_requested_scroll(v: *mut NsProcView, res: &Res) {
    view(v).adopting_scroll = 1;
    configure_adjustments(v);
    let (hadj, vadj) = (hadj(v), vadj(v));
    if res.requested_scroll_y >= 0
        && let Some(vadj) = &vadj
    {
        vadj.set_value(f64::from(res.requested_scroll_y));
    }
    if res.requested_scroll_x >= 0
        && let Some(hadj) = &hadj
    {
        hadj.set_value(f64::from(res.requested_scroll_x));
    }
    let state = view(v);
    state.adopting_scroll = 0;
    state.scroll_x = hadj.map_or(0.0, |adj| adj.value()) as c_int;
    state.scroll_y = vadj.map_or(0.0, |adj| adj.value()) as c_int;
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
        if adopt_page_size(v, res)
            && let Some(area) = area(v)
        {
            area.queue_draw();
        }
        if res.requested_scroll_y >= 0 || res.requested_scroll_x >= 0 {
            adopt_requested_scroll(v, res);
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
        if let Some(area) = area(v) {
            area.queue_draw();
        }
        clear_busy_cursor(v);
    } else if current && res.ok != 0 && res.frame_unchanged != 0 {
        view(v).render_restarts = 0;
    }
    if current && res.ok != 0 {
        redirect(v, filled(res.nav));
    }
    if res.ok != 0 {
        side_channels(v, res);
        if let Some(clipboard) = text(res.clipboard)
            && area(v).is_some()
        {
            copy_to_clipboard(v, clipboard, c"Copied to clipboard");
        }
    }
    let state = view(v);
    state.render_inflight = 0;
    if state.render_pending != 0 {
        state.render_pending = 0;
        start_render(v);
    } else if current
        && res.ok == 0
        && let Some(url) = current_url(v)
    {
        let state = view(v);
        if state.render_restarts < nav::MAX_RESTARTS {
            state.render_restarts += 1;
            post_emit(v, EVT_STATUS, Some(tr(c"Renderer restarted")));
            do_load(v, &url, false, false, false);
        } else {
            post_emit(
                v,
                EVT_STATUS,
                Some(tr(c"This tab's renderer keeps failing — reload to retry")),
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

fn show_link_status(v: *mut NsProcView, href: Option<&CStr>, cursor: Option<&CStr>) {
    let cursor = cursor.map(CStr::to_bytes);
    if let Some(href) = href {
        post_emit(v, EVT_STATUS, Some(href));
        if view(v).busy_cursor == 0 {
            set_named_cursor(v, cursor.unwrap_or(b"pointer"));
        }
    } else {
        post_emit(v, EVT_STATUS, Some(c""));
        if view(v).busy_cursor == 0 {
            set_named_cursor(v, cursor.unwrap_or_default());
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
            ns_pv_show_context_menu(v, text(res.href), res.edit);
        }
        pending_link(v);
        return;
    }
    let mut navigated = false;
    let href = filled(res.href);
    show_link_status(v, href, text(res.cursor));
    if let Some(href) = href {
        if res.action == ACT_NAVIGATE {
            navigated = true;
            page_load(v, href);
        } else if res.action == ACT_NEWTAB && page_may_open(v, href) {
            post_emit(v, EVT_NEWTAB, Some(href));
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
    show_link_status(v, filled(res.href), text(res.cursor));
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
                    let x = state.scroll_x + (state.sb_last_x / s) as c_int;
                    let y = state.scroll_y + (state.sb_last_y / s) as c_int;
                    push_req(
                        v,
                        new_req(REQ_SCROLLBAR, |r| {
                            r.kind = 1;
                            r.x = x;
                            r.y = y;
                        }),
                    );
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
    if res.seq != view(v).find_seq {
        return;
    }
    if let Some(find) = view(v).ui().find.as_ref() {
        let (entry, label) = (find.entry.clone(), find.label.clone());
        if res.find_total > 0 {
            label.set_text(&cstring(nav::find_count(res.find_current, res.find_total)));
        } else if entry.text().is_empty() {
            label.set_text(c"");
        } else {
            label.set_text(tr(c"No results"));
        }
    }
    if res.find_total > 0
        && let Some(vadj) = vadj(v)
    {
        vadj.set_value(nav::find_scroll_target(res.find_scroll_y));
    }
    request_render(v);
}

fn on_eval(v: *mut NsProcView, res: &Res) {
    let href = filled(res.href);
    if res.inspect != 0 {
        if let Some(elements) = ns_pv_console_text(v, DEV_TAB_ELEMENTS) {
            let shown = href.unwrap_or(tr(c"No matching element"));
            elements.set_text(&shown.to_string_lossy());
        }
    } else if let Some(href) = href {
        ns_pv_console_append(v, href.to_bytes());
        ns_pv_console_append(v, b"\n");
    } else {
        ns_pv_console_append(v, b"undefined\n");
    }
    request_render(v);
}

fn on_dump(v: *mut NsProcView, res: &Res) {
    if !matches!(
        res.dump_tab,
        DEV_TAB_NETWORK | DEV_TAB_PERFORMANCE | DEV_TAB_LAYOUT | DEV_TAB_ELEMENTS
    ) {
        return;
    }
    let Some(view_text) = ns_pv_console_text(v, res.dump_tab) else {
        return;
    };
    let shown = filled(res.href).unwrap_or(tr(c"(empty)"));
    let mut out = shown.to_bytes().to_vec();
    if res.dump_tab == DEV_TAB_PERFORMANCE {
        out.extend_from_slice(&ns_pv_media_stats(v));
    }
    view_text.set_text(&String::from_utf8_lossy(&out));
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
    post_emit(v, EVT_FAVICON, None);
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
            if let Some(href) = filled(res.href)
                && area(v).is_some()
            {
                copy_to_clipboard(v, href, c"Copied selection");
            }
            if res.kind == 7 {
                request_render(v);
            }
        }
        RES_KEY => {
            if res.seq != view(v).key_seq {
                return;
            }
            if res.kind == 0
                && let Some(href) = filled(res.href)
            {
                post_emit(v, EVT_STATUS, Some(href));
                page_load(v, href);
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
            if let Some(href) = filled(res.href) {
                post_emit(v, EVT_STATUS, Some(href));
                page_load(v, href);
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
            if let Some(href) = filled(res.href) {
                post_emit(v, EVT_STATUS, Some(href));
                page_load(v, href);
            } else if res.ok != 0 {
                request_render(v);
            }
        }
        RES_FIND => on_find(v, res),
        RES_EXPORT => {
            if res.ok != 0
                && let Some(url) = text(res.url)
            {
                post_emit(v, EVT_STATUS, Some(url));
            } else {
                post_emit(v, EVT_STATUS, Some(tr(c"Could not save page")));
            }
        }
        RES_PRINT => {
            let pages = res.print_pages.take().filter(|p| !p.sheets.is_empty());
            if let Some(pages) = pages {
                print_run(v, pages.sheets, &res.print_setup, pages.scale);
            } else {
                post_emit(v, EVT_STATUS, Some(tr(c"Nothing to print")));
            }
            request_render(v);
        }
        RES_CONSOLE => {
            if let Some(href) = filled(res.href) {
                ns_pv_console_append(v, href.to_bytes());
            }
        }
        RES_EVAL => on_eval(v, res),
        RES_DUMP => on_dump(v, res),
        RES_FAVICON => on_favicon(v, res),
        _ => {}
    }
}

fn free_res(res: *mut Res) {
    // SAFETY: res is the whole reply on_result took over; nothing else refers to it.
    let r = unsafe { &mut *res };
    clear_object(&mut r.texture);
    // SAFETY: layers is null or the reply's own LayerUpdate, freed once here.
    unsafe { ns_layer_update_free(r.layers.cast()) };
    for s in [
        r.title,
        r.url,
        r.nav,
        r.remote_ip,
        r.webgl,
        r.camera,
        r.download,
        r.audio,
        r.clipboard,
        r.window_action,
        r.href,
        r.cursor,
        r.media_url,
    ] {
        // SAFETY: each string is null or a GLib allocation the reply owns.
        unsafe { glib::g_free(s.cast()) };
    }
    if !r.favicon_bytes.is_null() {
        // SAFETY: drops the reply's reference to its favicon bytes.
        unsafe { gtk::g_bytes_unref(r.favicon_bytes) };
    }
    drop(r.print_pages.take());
    ns_proc_view_unref(r.view);
    // SAFETY: the reply was allocated with g_malloc0 and is freed exactly once.
    unsafe { glib::g_free(res.cast()) };
}

pub(super) unsafe extern "C" fn on_result(data: *mut c_void) -> GBoolean {
    // SAFETY: data is the reply the worker queued for this idle source, which hands it over once.
    let res = unsafe { &mut *data.cast::<Res>() };
    let v = res.view;
    if view(v).closed == 0 {
        dispatch(v, res);
    }
    free_res(data.cast());
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
    let seq = view(v).load_seq;
    push_req_front(
        v,
        new_req(REQ_VIDEO_EVENT, |r| {
            r.seq = seq;
            r.key = glib::strdup(token.to_bytes());
            r.query = glib::strdup(kind.to_bytes());
        }),
    );
}

pub(super) fn ns_proc_view_load(v: *mut NsProcView, url: &CStr) {
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
    view(v).render_restarts = 0;
    let url = history_at(v, view(v).hist_index).or_else(|| current_url(v));
    if let Some(url) = url {
        do_load(v, &url, false, false, true);
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
    push_req(
        v,
        new_req(REQ_EVAL, |r| {
            r.query = glib::strdup(b"document.exitFullscreen()");
        }),
    );
}

pub(super) fn ns_proc_view_url(v: *mut NsProcView) -> Option<CString> {
    current_url(v)
}

pub(super) fn ns_proc_view_local_page(v: *mut NsProcView) -> bool {
    !v.is_null() && view(v).local_page != 0
}

pub(super) fn ns_proc_view_title(v: *mut NsProcView) -> Option<CString> {
    view(v).ui().current_title.clone()
}

pub(super) fn ns_proc_view_security(v: *mut NsProcView) -> c_int {
    if v.is_null() { 0 } else { view(v).security }
}

pub(super) fn ns_proc_view_remote_ip(v: *mut NsProcView) -> Option<CString> {
    if v.is_null() {
        None
    } else {
        view(v).ui().remote_ip.clone()
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
    if let Some(area) = area(v) {
        area.grab_focus();
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
