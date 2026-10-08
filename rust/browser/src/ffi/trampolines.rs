//! Southstar — the C callbacks a page hands the script engine, the video cache, image sessions and GLib timeouts, each forwarding to the lifecycle.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable};
use southstar_layout::NsBox;

use super::engine::{
    ns_video_cache_mse_append, ns_video_cache_mse_buffered, ns_video_cache_mse_bytes,
    ns_video_cache_mse_eos, ns_video_cache_mse_remove,
};
use super::glib::{
    g_main_loop_new, g_main_loop_quit, g_main_loop_run, g_main_loop_unref, g_timeout_add,
};
use super::js::{NavigationTiming, ns_js_video_event};
use super::{Js, NsBrowser, Session, Videos, browser_for, c_str, monotonic_us, source_remove};
use crate::{build, callbacks, images, page, scroll, settle};

type TextCb = unsafe extern "C" fn(text: *const c_char, ud: *mut c_void);
type TextBoolCb = unsafe extern "C" fn(text: *const c_char, ud: *mut c_void) -> GBoolean;
type UdCb = unsafe extern "C" fn(ud: *mut c_void);
type UdBoolCb = unsafe extern "C" fn(ud: *mut c_void) -> GBoolean;
type NavigateCb = unsafe extern "C" fn(url: *const c_char, flag: GBoolean, ud: *mut c_void);
type FormSubmitCb =
    unsafe extern "C" fn(form: *const NsNode, submitter: *const NsNode, ud: *mut c_void);
type ViewportScrollCb = unsafe extern "C" fn(x: *mut f64, y: *mut f64, ud: *mut c_void);
type ScrollToCb = unsafe extern "C" fn(target: *const NsNode, ud: *mut c_void);
type DownloadCb =
    unsafe extern "C" fn(url: *const c_char, filename: *const c_char, ud: *mut c_void);
type MediaSeekCb =
    unsafe extern "C" fn(node: *const c_void, seconds: f64, ud: *mut c_void) -> GBoolean;
type MediaFlagCb = unsafe extern "C" fn(node: *const c_void, on: GBoolean, ud: *mut c_void);
type MediaVolumeCb = unsafe extern "C" fn(node: *const c_void, volume: f64, ud: *mut c_void);
type MseCb = unsafe extern "C" fn(
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
    eos: GBoolean,
    ud: *mut c_void,
) -> GBoolean;
type MseBufferedCb =
    unsafe extern "C" fn(stream_id: c_uint, kind: c_char, start: *mut f64, ud: *mut c_void) -> f64;
type MseRemoveCb = unsafe extern "C" fn(
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    ud: *mut c_void,
) -> GBoolean;
type MseBytesCb = unsafe extern "C" fn(stream_id: c_uint, kind: c_char, ud: *mut c_void) -> usize;
type VideoJsCb =
    unsafe extern "C" fn(node: *const c_void, kind: *const c_char, value: f64, ud: *mut c_void);

unsafe extern "C" {
    fn ns_js_new(
        log_cb: TextCb,
        log_ud: *mut c_void,
        mut_cb: UdCb,
        mut_ud: *mut c_void,
        nav_cb: NavigateCb,
        nav_ud: *mut c_void,
        timing: *const NavigationTiming,
    ) -> *mut c_void;
    fn ns_js_set_load_delay_cb(js: *mut c_void, cb: UdBoolCb, ud: *mut c_void);
    fn ns_js_set_form_submit_cb(js: *mut c_void, cb: FormSubmitCb, ud: *mut c_void);
    fn ns_js_set_layout_flush_cb(js: *mut c_void, cb: UdCb, ud: *mut c_void);
    fn ns_js_set_viewport_scroll_cb(js: *mut c_void, cb: ViewportScrollCb, ud: *mut c_void);
    fn ns_js_set_scroll_to_cb(js: *mut c_void, cb: ScrollToCb, ud: *mut c_void);
    fn ns_js_set_fragment_nav_cb(js: *mut c_void, cb: TextCb, ud: *mut c_void);
    fn ns_js_set_soft_nav_cb(js: *mut c_void, cb: NavigateCb, ud: *mut c_void);
    fn ns_js_set_download_cb(js: *mut c_void, cb: DownloadCb, ud: *mut c_void);
    fn ns_js_set_clipboard_write_cb(js: *mut c_void, cb: TextBoolCb, ud: *mut c_void);
    fn ns_js_set_selection_cmd_cb(js: *mut c_void, cb: TextBoolCb, ud: *mut c_void);
    fn ns_js_set_audio_cb(js: *mut c_void, cb: TextCb, ud: *mut c_void);
    fn ns_js_set_media_seek_cb(js: *mut c_void, cb: MediaSeekCb, ud: *mut c_void);
    fn ns_js_set_media_play_cb(js: *mut c_void, cb: MediaFlagCb, ud: *mut c_void);
    fn ns_js_set_media_muted_cb(js: *mut c_void, cb: MediaFlagCb, ud: *mut c_void);
    fn ns_js_set_mse_cb(js: *mut c_void, cb: MseCb, ud: *mut c_void);
    fn ns_js_set_mse_buffered_cb(js: *mut c_void, cb: MseBufferedCb, ud: *mut c_void);
    fn ns_js_set_mse_remove_cb(js: *mut c_void, cb: MseRemoveCb, ud: *mut c_void);
    fn ns_js_set_mse_bytes_cb(js: *mut c_void, cb: MseBytesCb, ud: *mut c_void);
    fn ns_js_set_media_volume_cb(js: *mut c_void, cb: MediaVolumeCb, ud: *mut c_void);
    fn ns_js_set_window_action_cb(js: *mut c_void, cb: TextCb, ud: *mut c_void);
    fn ns_video_cache_set_js_cb(cache: *mut c_void, cb: VideoJsCb, ud: *mut c_void);
    fn ns_video_cache_set_audio_cb(cache: *mut c_void, cb: TextCb, ud: *mut c_void);
    fn ns_engine_fetch_images_start(
        root: *mut NsBox,
        base_url: *const c_char,
        cache: *mut c_void,
        requested: *mut GHashTable,
        scroll_y: f64,
        viewport_h: f64,
        deferred_any: *mut GBoolean,
        arrived_cb: UdCb,
        ud: *mut c_void,
    ) -> *mut c_void;
}

unsafe extern "C" fn on_log(line: *const c_char, ud: *mut c_void) {
    if let (Some(b), Some(line)) = (browser_for(ud), c_str(line)) {
        callbacks::js_log(b, line);
    }
}

unsafe extern "C" fn on_mutated(ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        b.dirty.set(true);
    }
}

unsafe extern "C" fn on_navigate(url: *const c_char, _reload: GBoolean, ud: *mut c_void) {
    if let (Some(b), Some(url)) = (browser_for(ud), c_str(url)) {
        callbacks::js_navigate(b, url);
    }
}

unsafe extern "C" fn on_load_delay(ud: *mut c_void) -> GBoolean {
    browser_for(ud).map_or(0, |b| {
        southstar_glib::boolean(images::load_waits_for_images(b))
    })
}

unsafe extern "C" fn on_form_submit(
    form: *const NsNode,
    submitter: *const NsNode,
    ud: *mut c_void,
) {
    let (Some(b), Some(form)) = (browser_for(ud), unsafe { Node::from_ptr(form) }) else {
        return;
    };
    build::js_form_submit(b, form, unsafe { Node::from_ptr(submitter) });
}

unsafe extern "C" fn on_flush(ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        page::flush(b);
    }
}

unsafe extern "C" fn on_viewport_scroll(x: *mut f64, y: *mut f64, ud: *mut c_void) {
    let Some(b) = browser_for(ud) else {
        return;
    };
    let (sx, sy) = scroll::js_viewport_scroll(b, unsafe { *x }, unsafe { *y });
    unsafe {
        *x = sx;
        *y = sy;
    }
}

unsafe extern "C" fn on_scroll_to(target: *const NsNode, ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        scroll::js_scroll_to(b, unsafe { Node::from_ptr(target) });
    }
}

unsafe extern "C" fn on_fragment_nav(url: *const c_char, ud: *mut c_void) {
    if let (Some(b), Some(url)) = (browser_for(ud), c_str(url)) {
        scroll::js_fragment_navigate(b, url);
    }
}

unsafe extern "C" fn on_soft_nav(url: *const c_char, replace: GBoolean, ud: *mut c_void) {
    if let (Some(b), Some(url)) = (browser_for(ud), c_str(url)) {
        scroll::js_soft_navigate(b, url, replace != 0);
    }
}

unsafe extern "C" fn on_download(url: *const c_char, filename: *const c_char, ud: *mut c_void) {
    if let (Some(b), Some(url)) = (browser_for(ud), c_str(url)) {
        callbacks::js_download(b, url, c_str(filename));
    }
}

unsafe extern "C" fn on_clipboard_write(text: *const c_char, ud: *mut c_void) -> GBoolean {
    match (browser_for(ud), c_str(text)) {
        (Some(b), Some(text)) => southstar_glib::boolean(callbacks::clipboard_write(b, text)),
        _ => 0,
    }
}

unsafe extern "C" fn on_selection_cmd(command: *const c_char, ud: *mut c_void) -> GBoolean {
    match (browser_for(ud), c_str(command)) {
        (Some(b), Some(command)) => southstar_glib::boolean(callbacks::selection_cmd(b, command)),
        _ => 0,
    }
}

unsafe extern "C" fn on_audio(command: *const c_char, ud: *mut c_void) {
    if let (Some(b), Some(command)) = (browser_for(ud), c_str(command)) {
        callbacks::js_audio(b, command);
    }
}

unsafe extern "C" fn on_media_seek(node: *const c_void, seconds: f64, ud: *mut c_void) -> GBoolean {
    southstar_glib::boolean(callbacks::media_seek(browser_for(ud), node, seconds))
}

unsafe extern "C" fn on_media_play(node: *const c_void, play: GBoolean, ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        callbacks::media_play(b, node, play != 0);
    }
}

unsafe extern "C" fn on_media_muted(node: *const c_void, muted: GBoolean, ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        callbacks::media_muted(b, node, muted != 0);
    }
}

unsafe extern "C" fn on_media_volume(node: *const c_void, volume: f64, ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        callbacks::media_volume(b, node, volume);
    }
}

fn videos_of(ud: *mut c_void) -> Option<Videos> {
    browser_for(ud).and_then(NsBrowser::videos)
}

unsafe extern "C" fn on_mse_data(
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
    eos: GBoolean,
    ud: *mut c_void,
) -> GBoolean {
    let Some(videos) = videos_of(ud) else {
        return 0;
    };
    if eos != 0 {
        unsafe { ns_video_cache_mse_eos(videos.raw(), stream_id) };
        return 1;
    }
    unsafe { ns_video_cache_mse_append(videos.raw(), stream_id, kind, data, len) }
}

unsafe extern "C" fn on_mse_buffered(
    stream_id: c_uint,
    kind: c_char,
    start: *mut f64,
    ud: *mut c_void,
) -> f64 {
    let Some(videos) = videos_of(ud) else {
        if let Some(start) = unsafe { start.as_mut() } {
            *start = 0.0;
        }
        return 0.0;
    };
    unsafe { ns_video_cache_mse_buffered(videos.raw(), stream_id, kind, start) }
}

unsafe extern "C" fn on_mse_remove(
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    ud: *mut c_void,
) -> GBoolean {
    match videos_of(ud) {
        Some(videos) => unsafe {
            ns_video_cache_mse_remove(videos.raw(), stream_id, kind, start, end)
        },
        None => 0,
    }
}

unsafe extern "C" fn on_mse_bytes(stream_id: c_uint, kind: c_char, ud: *mut c_void) -> usize {
    match videos_of(ud) {
        Some(videos) => unsafe { ns_video_cache_mse_bytes(videos.raw(), stream_id, kind) },
        None => 0,
    }
}

unsafe extern "C" fn on_window_action(action: *const c_char, ud: *mut c_void) {
    if let (Some(b), Some(action)) = (browser_for(ud), c_str(action)) {
        callbacks::window_action(b, action);
    }
}

unsafe extern "C" fn on_video_event(
    node: *const c_void,
    kind: *const c_char,
    value: f64,
    ud: *mut c_void,
) {
    if let Some(js) = browser_for(ud).and_then(NsBrowser::js) {
        unsafe { ns_js_video_event(js.raw(), node, kind, value) };
    }
}

unsafe extern "C" fn on_fire_media_events(ud: *mut c_void) -> GBoolean {
    if let Some(b) = browser_for(ud) {
        images::fire_media_events(b);
    }
    0
}

unsafe extern "C" fn on_image_arrived(ud: *mut c_void) {
    if let Some(b) = browser_for(ud) {
        images::image_arrived(b);
    }
}

pub fn new_js(b: &NsBrowser, timing: *const NavigationTiming) -> Option<Js> {
    let ud = b.as_ptr().cast::<c_void>();
    unsafe {
        Js::from_ptr(ns_js_new(
            on_log,
            ud,
            on_mutated,
            ud,
            on_navigate,
            ud,
            timing,
        ))
    }
}

pub fn wire_js(b: &NsBrowser, js: Js) {
    let ud = b.as_ptr().cast::<c_void>();
    let raw = js.raw();
    unsafe {
        ns_js_set_load_delay_cb(raw, on_load_delay, ud);
        js.set_style_table(Some(b.styles_raw()));
        js.set_image_cache(b.images());
        js.set_anim(b.anim());
        ns_js_set_form_submit_cb(raw, on_form_submit, ud);
        ns_js_set_layout_flush_cb(raw, on_flush, ud);
        ns_js_set_viewport_scroll_cb(raw, on_viewport_scroll, ud);
        ns_js_set_scroll_to_cb(raw, on_scroll_to, ud);
        ns_js_set_fragment_nav_cb(raw, on_fragment_nav, ud);
        ns_js_set_soft_nav_cb(raw, on_soft_nav, ud);
        ns_js_set_download_cb(raw, on_download, ud);
        ns_js_set_clipboard_write_cb(raw, on_clipboard_write, ud);
        ns_js_set_selection_cmd_cb(raw, on_selection_cmd, ud);
        ns_js_set_audio_cb(raw, on_audio, ud);
        ns_js_set_media_seek_cb(raw, on_media_seek, ud);
        ns_js_set_media_play_cb(raw, on_media_play, ud);
        ns_js_set_media_muted_cb(raw, on_media_muted, ud);
        ns_js_set_mse_cb(raw, on_mse_data, ud);
        ns_js_set_mse_buffered_cb(raw, on_mse_buffered, ud);
        ns_js_set_mse_remove_cb(raw, on_mse_remove, ud);
        ns_js_set_mse_bytes_cb(raw, on_mse_bytes, ud);
        ns_js_set_media_volume_cb(raw, on_media_volume, ud);
        ns_js_set_window_action_cb(raw, on_window_action, ud);
    }
}

pub fn wire_videos(b: &NsBrowser, videos: Videos) {
    let ud = b.as_ptr().cast::<c_void>();
    unsafe {
        ns_video_cache_set_js_cb(videos.raw(), on_video_event, ud);
        ns_video_cache_set_audio_cb(videos.raw(), on_audio, ud);
    }
}

pub fn add_media_events_timeout(b: &NsBrowser) -> c_uint {
    unsafe { g_timeout_add(0, on_fire_media_events, b.as_ptr().cast()) }
}

pub fn start_image_session(b: &NsBrowser, viewport_h: f64) -> (Option<Session>, bool) {
    let mut deferred: GBoolean = 0;
    let session = unsafe {
        ns_engine_fetch_images_start(
            b.layout.get(),
            b.base_url.ptr(),
            b.images.get(),
            b.img_requested.get(),
            b.cur_scroll_y.get(),
            viewport_h,
            &mut deferred,
            on_image_arrived,
            b.as_ptr().cast(),
        )
    };
    (Session::from_raw(session), deferred != 0)
}

struct SettleCtx<'a> {
    b: &'a NsBrowser,
    main_loop: *mut c_void,
    state: settle::SettleState,
}

unsafe extern "C" fn on_settle_quit(main_loop: *mut c_void) -> GBoolean {
    unsafe { g_main_loop_quit(main_loop) };
    1
}

unsafe extern "C" fn on_settle_tick(ud: *mut c_void) -> GBoolean {
    let ctx = unsafe { &mut *ud.cast::<SettleCtx<'_>>() };
    if settle::settle_tick(ctx.b, &mut ctx.state) {
        unsafe { g_main_loop_quit(ctx.main_loop) };
    }
    1
}

pub fn run_settle_loop(b: &NsBrowser, settle_ms: c_int) {
    let main_loop = unsafe { g_main_loop_new(ptr::null_mut(), 0) };
    let mut ctx = SettleCtx {
        b,
        main_loop,
        state: settle::SettleState::new(monotonic_us() + i64::from(settle_ms) * 1000),
    };
    let quit = unsafe { g_timeout_add(settle_ms as c_uint, on_settle_quit, main_loop) };
    let tick = unsafe { g_timeout_add(16, on_settle_tick, ptr::from_mut(&mut ctx).cast()) };
    unsafe { g_main_loop_run(main_loop) };
    source_remove(tick);
    source_remove(quit);
    unsafe { g_main_loop_unref(main_loop) };
}
