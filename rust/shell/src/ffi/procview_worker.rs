//! Southstar — the tab view's worker thread, which carries each request to the renderer process and posts its reply to the main loop.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk_ffi as gtk;
use southstar_renderer_client::{Page, Renderer, TilesRequest, View};

use super::appmain::ns_app_self_exe;
use super::pagelayers::layer_update_parse;
use super::procview_draw::stage_fill;
use super::procview_result::on_result;
use super::procview_state::{NsProcView, Req, Res};
use super::procview_view::{ns_proc_view_ref, ns_proc_view_unref};
use crate::procview::nav;

const REQ_LOAD: c_int = 0;
const REQ_RENDER: c_int = 1;
const REQ_LINK: c_int = 2;
const REQ_CLICK: c_int = 3;
const REQ_VIEWPORT: c_int = 4;
const REQ_KEY: c_int = 5;
const REQ_SELECT: c_int = 6;
const REQ_HOVER: c_int = 7;
const REQ_RELEASE: c_int = 8;
const REQ_FIND: c_int = 9;
const REQ_EXPORT: c_int = 10;
const REQ_CONSOLE: c_int = 11;
const REQ_EVAL: c_int = 12;
const REQ_DUMP: c_int = 13;
const REQ_DROPFILES: c_int = 14;
const REQ_SCROLLBAR: c_int = 15;
const REQ_WEBGL: c_int = 16;
const REQ_CAMERA: c_int = 17;
const REQ_COLOR_SCHEME: c_int = 18;
const REQ_FAVICON: c_int = 19;
const REQ_VIDEO_EVENT: c_int = 20;
const REQ_TICK: c_int = 21;
const REQ_PRINT: c_int = 22;
pub(super) const REQ_QUIT: c_int = 23;

pub(super) const RES_PAGE: c_int = 0;
pub(super) const RES_FRAME: c_int = 1;
pub(super) const RES_LINK: c_int = 2;
pub(super) const RES_CLICK: c_int = 3;
pub(super) const RES_VIEWPORT: c_int = 4;
pub(super) const RES_KEY: c_int = 5;
pub(super) const RES_SELECT: c_int = 6;
pub(super) const RES_COPY: c_int = 7;
pub(super) const RES_HOVER: c_int = 8;
pub(super) const RES_RELEASE: c_int = 9;
pub(super) const RES_FIND: c_int = 10;
pub(super) const RES_EXPORT: c_int = 11;
pub(super) const RES_CONSOLE: c_int = 12;
pub(super) const RES_EVAL: c_int = 13;
pub(super) const RES_DUMP: c_int = 14;
pub(super) const RES_FAVICON: c_int = 15;
pub(super) const RES_DROPFILES: c_int = 16;
pub(super) const RES_SCROLLBAR: c_int = 17;
pub(super) const RES_TICK: c_int = 18;
pub(super) const RES_PRINT: c_int = 19;

const ACT_HOVER: c_int = 0;
const ACT_CONTEXT: c_int = 3;

const FILE_TEST_IS_EXECUTABLE: glib::GFileTest = 1 << 3;
const FILE_QUERY_INFO_NOFOLLOW_SYMLINKS: c_uint = 1;
const FILE_TYPE_REGULAR: c_int = 1;
const FILE_COPY_OVERWRITE: c_uint = 1;
const FILE_COPY_NOFOLLOW_SYMLINKS: c_uint = 1 << 2;

unsafe extern "C" {
    fn g_async_queue_pop(queue: *mut c_void) -> *mut c_void;
    fn g_mutex_lock(mutex: *mut c_void);
    fn g_mutex_unlock(mutex: *mut c_void);
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_file_new_for_path(path: *const c_char) -> *mut c_void;
    fn g_file_query_file_type(file: *mut c_void, flags: c_uint, cancellable: *mut c_void) -> c_int;
    fn g_file_copy(
        source: *mut c_void,
        destination: *mut c_void,
        flags: c_uint,
        cancellable: *mut c_void,
        progress_callback: *mut c_void,
        progress_callback_data: *mut c_void,
        error: *mut *mut c_void,
    ) -> GBoolean;
}

fn dup_or(s: *const c_char, fallback: *const c_char) -> *mut c_char {
    unsafe { glib::g_strdup(if s.is_null() { fallback } else { s }) }
}

fn dup(s: *const c_char) -> *mut c_char {
    if s.is_null() {
        ptr::null_mut()
    } else {
        unsafe { glib::g_strdup(s) }
    }
}

fn text(value: Option<Vec<u8>>) -> *mut c_char {
    value.map_or(ptr::null_mut(), |value| glib::strdup(&value))
}

fn arg<'a>(s: *const c_char) -> Option<&'a [u8]> {
    // SAFETY: a request's strings are null or NUL-terminated copies the request owns until free_req, after the handler returns.
    unsafe { glib::bytes(s) }
}

fn reply<'a>(res: *mut Res) -> &'a mut Res {
    // SAFETY: res comes from new_res, and this thread is its only user until post hands it to the main loop.
    unsafe { &mut *res }
}

fn proc_of(v: *mut NsProcView) -> *mut Renderer {
    // SAFETY: v is a live view, and `proc` is written only by this worker thread.
    unsafe { (*v).proc }
}

fn closed(v: *mut NsProcView) -> bool {
    // SAFETY: v is a live view; `closed` is a flag the main thread sets once.
    unsafe { (*v).closed != 0 }
}

fn with_renderer<T>(v: *mut NsProcView, ask: impl FnOnce(&mut Renderer) -> T) -> Option<T> {
    let p = proc_of(v);
    // SAFETY: only this worker thread dereferences or replaces `proc`, and the main thread reaches the renderer through `remote` alone, so the renderer is alive and borrowed by nobody else during the call.
    unsafe { p.as_mut() }.map(ask)
}

pub(super) fn lock_proc(v: *mut NsProcView) {
    unsafe { g_mutex_lock((&raw mut (*v).proc_lock).cast()) };
}

pub(super) fn unlock_proc(v: *mut NsProcView) {
    unsafe { g_mutex_unlock((&raw mut (*v).proc_lock).cast()) };
}

fn install(v: *mut NsProcView, next: Option<Renderer>) -> Option<Box<Renderer>> {
    let remote = next.as_ref().map(Renderer::remote);
    let next = next.map_or(ptr::null_mut(), |r| Box::into_raw(Box::new(r)));
    lock_proc(v);
    let old = proc_of(v);
    // SAFETY: v is a live view and proc_lock is held, so no other thread reads `proc` or `remote` meanwhile.
    unsafe { (*v).proc = next };
    // SAFETY: as above, under proc_lock.
    unsafe { (*v).remote = remote };
    unlock_proc(v);
    // SAFETY: old came from Box::into_raw in an earlier install and is no longer reachable from the view.
    (!old.is_null()).then(|| unsafe { Box::from_raw(old) })
}

fn spawn(v: *mut NsProcView) -> Option<Renderer> {
    // SAFETY: v is a live view; renderer_path is set when the view is built and freed only after this worker exits.
    let path = unsafe { (*v).renderer_path };
    // SAFETY: v is a live view; private_mode is set when the view is built.
    let private = unsafe { (*v).private_mode } != 0;
    Renderer::spawn(arg(path), nav::MAX_WIDTH, nav::MAX_HEIGHT, private)
}

fn settle_ms() -> c_int {
    nav::settle_ms(unsafe { glib::bytes(glib::g_getenv(c"NS_SETTLE_MS".as_ptr())) })
}

pub(super) fn free_req(req: *mut Req) {
    unsafe {
        let r = &*req;
        for s in [
            r.url,
            r.key,
            r.code,
            r.query,
            r.export_dest,
            r.paths,
            r.have,
            r.hold,
        ] {
            glib::g_free(s.cast());
        }
        glib::g_free(req.cast());
    }
}

fn new_res(v: *mut NsProcView, kind: c_int, seq: c_int) -> *mut Res {
    let res = unsafe { g_malloc0(core::mem::size_of::<Res>()) }.cast::<Res>();
    unsafe {
        (*res).view = ns_proc_view_ref(v);
        (*res).r#type = kind;
        (*res).seq = seq;
    }
    res
}

fn post(res: *mut Res) {
    unsafe { gtk::g_idle_add(on_result, res.cast()) };
}

unsafe extern "C" fn video_event_render(data: *mut c_void) -> GBoolean {
    let v = data.cast::<NsProcView>();
    super::procview_draw::request_tick(v);
    ns_proc_view_unref(v);
    gtk::SOURCE_REMOVE
}

fn open_page(v: *mut NsProcView, r: &Req, settle: c_int) -> Option<Page> {
    let url = arg(r.url)?;
    with_renderer(v, |p| {
        p.open(
            url,
            r.vw,
            r.vh,
            settle,
            r.history != 0,
            r.user_activated != 0,
        )
    })
    .flatten()
}

fn do_load(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_PAGE, r.seq);
    let settle = settle_ms();
    let mut page = open_page(v, r, settle);
    if page.is_none() && !proc_of(v).is_null() && !closed(v) {
        drop(install(v, None));
        drop(install(v, spawn(v)));
        with_renderer(v, |p| p.set_device_pixel_ratio(r.dpr));
        page = open_page(v, r, settle);
    }
    if let Some(page) = page.filter(|page| page.ok) {
        let out = reply(res);
        out.ok = 1;
        out.pw = page.page_width;
        out.ph = page.page_height;
        out.title = glib::strdup(page.title.as_deref().unwrap_or_default());
        out.url = page
            .url
            .map_or_else(|| dup(r.url), |url| glib::strdup(&url));
        out.nav = text(page.nav);
        out.security = page.security;
        out.remote_ip = text(page.remote_ip);
    }
    post(res);
}

fn render_into(p: &mut Renderer, r: &Req, out: &mut Res) -> bool {
    let view = View {
        width: r.w,
        height: r.h,
        scroll_x: r.sx,
        scroll_y: r.sy,
        scale: r.scale,
        caret: r.caret_active != 0,
    };
    let tiles = TilesRequest {
        tile_h: r.tiles.tile_h,
        want_y0: r.tiles.want_y0,
        want_y1: r.tiles.want_y1,
        generation: r.tiles.gen_,
        vp_held: r.tiles.vp_held,
        fill: r.tiles.fill,
        have: arg(r.have).unwrap_or_default(),
        hold: arg(r.hold).unwrap_or_default(),
    };
    let tiled = (r.tiles.tile_h > 0).then_some(&tiles);
    let Some(frame) = p.render(view, Some(&r.wheel), tiled) else {
        return false;
    };
    out.ok = 1;
    out.animating = glib::boolean(frame.animating);
    out.caret_blinking = glib::boolean(frame.caret_blinking);
    out.wheel_snapped = glib::boolean(frame.wheel_snapped);
    out.pw = frame.page_w;
    out.ph = frame.page_h;
    out.requested_scroll_y = frame.scroll_y;
    out.requested_scroll_x = frame.scroll_x;
    out.frame_unchanged = glib::boolean(frame.unchanged);
    if let Some(desc) = &frame.tiles {
        out.layers = layer_update_parse(desc, frame.pixels).cast();
        out.frame_unchanged = glib::boolean(out.layers.is_null());
    } else if !frame.unchanged {
        out.texture = stage_fill(frame.pixels, frame.width, frame.height, frame.stride).cast();
        out.texture_scale = if r.raster > 0.0 { r.raster } else { 1.0 };
    }
    let clipboard = frame.clipboard;
    out.nav = text(frame.nav);
    out.webgl = text(frame.webgl);
    out.camera = text(frame.camera);
    out.download = text(frame.download);
    out.audio = text(frame.audio);
    out.window_action = text(frame.window_action);
    if clipboard {
        out.clipboard = text(p.clipboard());
    }
    true
}

fn do_render(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_FRAME, r.seq);
    let out = reply(res);
    let rendered = with_renderer(v, |p| render_into(p, r, out)).unwrap_or(false);
    if !rendered && !proc_of(v).is_null() {
        drop(install(v, None));
    }
    post(res);
}

fn do_tick(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_TICK, r.seq);
    if let Some(tick) = with_renderer(v, Renderer::tick).flatten() {
        let out = reply(res);
        out.ok = glib::boolean(tick.ok);
        out.kind = c_int::from(tick.changed);
        out.animating = glib::boolean(tick.animating);
        out.pw = tick.page_w;
        out.ph = tick.page_h;
        out.nav = text(tick.nav);
        out.webgl = text(tick.webgl);
        out.camera = text(tick.camera);
        out.download = text(tick.download);
        out.audio = text(tick.audio);
        out.window_action = text(tick.window_action);
        out.title = text(tick.title);
        out.url = text(tick.url);
        out.url_pushed = glib::boolean(tick.url_pushed);
    }
    post(res);
}

fn do_link(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_LINK, r.seq);
    let out = reply(res);
    out.action = r.action;
    with_renderer(v, |p| {
        if r.action == ACT_HOVER {
            if let Some((href, cursor)) = p.link_cursor_at(r.x, r.y) {
                out.href = text(href);
                out.cursor = text(cursor);
            }
        } else if r.action == ACT_CONTEXT {
            let (prevented, edit) = p.contextmenu(r.x, r.y).unwrap_or((0, 0));
            out.prevented = prevented;
            out.edit = edit;
            if prevented == 0 {
                out.href = text(p.link_at(r.x, r.y));
            }
        } else {
            out.href = text(p.link_at(r.x, r.y));
        }
    });
    post(res);
}

fn do_viewport(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_VIEWPORT, r.seq);
    let page = with_renderer(v, |p| p.set_viewport(r.vw, r.vh)).flatten();
    if let Some(page) = page.filter(|page| page.ok) {
        let out = reply(res);
        out.ok = 1;
        out.pw = page.page_width;
        out.ph = page.page_height;
    }
    post(res);
}

fn do_key(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_KEY, r.seq);
    let out = reply(res);
    out.kind = r.kind;
    out.fallback_scroll = r.fallback_scroll;
    out.fallback_x = r.fallback_x;
    out.fallback_y = r.fallback_y;
    let key = arg(r.key).unwrap_or_default();
    let code = arg(r.code).unwrap_or_default();
    if let Some((href, prevented)) =
        with_renderer(v, |p| p.key(r.kind, key, code, r.keycode, r.mods)).flatten()
    {
        out.prevented = c_int::from(prevented);
        out.href = text(href);
    }
    post(res);
}

fn do_select(v: *mut NsProcView, r: &Req) {
    let kind = if r.kind == 4 || r.kind == 7 {
        RES_COPY
    } else {
        RES_SELECT
    };
    let res = new_res(v, kind, r.seq);
    let out = reply(res);
    out.kind = r.kind;
    out.href = text(with_renderer(v, |p| p.select(r.kind, r.x, r.y)).flatten());
    post(res);
}

fn do_dropfiles(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_DROPFILES, r.seq);
    if let Some(paths) = arg(r.paths).filter(|paths| !paths.is_empty()) {
        let list: Vec<&[u8]> = paths.split(|&b| b == b'\n').collect();
        if let Some(changed) = with_renderer(v, |p| p.drop_files(r.x, r.y, &list)) {
            reply(res).ok = glib::boolean(changed == 1);
        }
    }
    post(res);
}

fn do_release(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_RELEASE, r.seq);
    let out = reply(res);
    with_renderer(v, |p| {
        let href = p.release().and_then(|(href, changed)| {
            out.ok = glib::boolean(changed);
            href
        });
        let follows = href.as_ref().is_some_and(|href| !href.is_empty());
        out.href = text(href);
        if !follows && let Some((url, is_video, stream)) = p.media_at(r.x, r.y) {
            out.media_url = text(url);
            out.media_is_video = is_video;
            out.media_stream = stream;
        }
    });
    post(res);
}

fn do_export(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_EXPORT, r.seq);
    let mut ok = false;
    let exported = !r.export_dest.is_null()
        && arg(r.url).is_some_and(|url| with_renderer(v, |p| p.export(url)).unwrap_or(false));
    if exported {
        unsafe {
            let src = g_file_new_for_path(r.url);
            let dst = g_file_new_for_path(r.export_dest);
            if g_file_query_file_type(src, FILE_QUERY_INFO_NOFOLLOW_SYMLINKS, ptr::null_mut())
                == FILE_TYPE_REGULAR
            {
                ok = g_file_copy(
                    src,
                    dst,
                    FILE_COPY_OVERWRITE | FILE_COPY_NOFOLLOW_SYMLINKS,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ) != 0;
            }
            gtk::g_object_unref(src);
            gtk::g_object_unref(dst);
        }
    }
    if !r.url.is_null() {
        unsafe { gtk::g_unlink(r.url) };
    }
    let out = reply(res);
    out.ok = glib::boolean(ok);
    out.url = dup_or(r.export_dest, c"".as_ptr());
    post(res);
}

fn text_res(
    v: *mut NsProcView,
    seq: c_int,
    kind: c_int,
    fill: impl FnOnce(&mut Res),
    ask: impl FnOnce(&mut Renderer) -> Option<Vec<u8>>,
) {
    let res = new_res(v, kind, seq);
    let out = reply(res);
    fill(out);
    out.href = text(with_renderer(v, ask).flatten());
    post(res);
}

fn do_print(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_PRINT, r.seq);
    let out = reply(res);
    if let Some(pages) = with_renderer(v, |p| p.print(&mut out.print_setup)).flatten() {
        out.print_pages = pages.sheets.as_ptr().cast();
        out.print_scale = pages.scale;
    }
    out.ok = glib::boolean(!out.print_pages.is_null());
    post(res);
}

fn do_favicon(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_FAVICON, r.seq);
    if let Some(icon) = with_renderer(v, Renderer::favicon).flatten() {
        let out = reply(res);
        // SAFETY: the icon's pixels are a live slice of the given length, which GLib copies into the new GBytes.
        out.favicon_bytes =
            unsafe { gtk::g_bytes_new(icon.pixels.as_ptr().cast(), icon.pixels.len()) };
        out.favicon_w = icon.width;
        out.favicon_h = icon.height;
        out.favicon_stride = icon.stride;
    }
    post(res);
}

fn do_video_event(v: *mut NsProcView, r: &Req) {
    // SAFETY: v is a live view; load_seq is an integer the main thread bumps per load.
    let current = !proc_of(v).is_null() && r.seq == unsafe { (*v).load_seq };
    let sent = current
        && match (arg(r.key), arg(r.query)) {
            (Some(token), Some(kind)) => {
                with_renderer(v, |p| p.video_event(token, kind)).unwrap_or(false)
            }
            _ => false,
        };
    if sent {
        unsafe { gtk::g_idle_add(video_event_render, ns_proc_view_ref(v).cast()) };
    }
}

fn handle(v: *mut NsProcView, req: *mut Req) {
    // SAFETY: req was popped from the queue and stays owned by this worker until free_req.
    let r = unsafe { &*req };
    match r.r#type {
        REQ_LOAD => do_load(v, r),
        REQ_RENDER => do_render(v, r),
        REQ_TICK => do_tick(v, r),
        REQ_LINK => do_link(v, r),
        REQ_CLICK => text_res(v, r.seq, RES_CLICK, |_| {}, |p| p.click(r.x, r.y, r.mods)),
        REQ_VIEWPORT => do_viewport(v, r),
        REQ_KEY => do_key(v, r),
        REQ_SELECT => do_select(v, r),
        REQ_HOVER => {
            let res = new_res(v, RES_HOVER, r.seq);
            if let Some((changed, href, cursor)) = with_renderer(v, |p| p.hover(r.x, r.y)).flatten()
            {
                let out = reply(res);
                out.ok = glib::boolean(changed);
                out.href = text(href);
                out.cursor = text(cursor);
            }
            post(res);
        }
        REQ_SCROLLBAR => {
            let res = new_res(v, RES_SCROLLBAR, r.seq);
            let out = reply(res);
            out.kind = r.kind;
            out.ok = with_renderer(v, |p| p.scrollbar(r.kind, r.x, r.y)).map_or(0, glib::boolean);
            post(res);
        }
        REQ_DROPFILES => do_dropfiles(v, r),
        REQ_RELEASE => do_release(v, r),
        REQ_FIND => {
            let res = new_res(v, RES_FIND, r.seq);
            let query = arg(r.query).unwrap_or_default();
            let found = with_renderer(v, |p| {
                p.find(query, r.find_case != 0, r.find_dir, r.find_from_y)
            });
            if let Some([total, current, scroll_y]) = found.flatten() {
                let out = reply(res);
                out.find_total = total;
                out.find_current = current;
                out.find_scroll_y = scroll_y;
            }
            post(res);
        }
        REQ_EXPORT => do_export(v, r),
        REQ_CONSOLE => text_res(v, r.seq, RES_CONSOLE, |_| {}, Renderer::console_poll),
        REQ_EVAL => {
            let src = arg(r.query).unwrap_or_default();
            let (tab, inspect) = (r.dump_tab, r.inspect);
            text_res(
                v,
                r.seq,
                RES_EVAL,
                |out| {
                    out.dump_tab = tab;
                    out.inspect = inspect;
                },
                |p| p.eval(src),
            );
        }
        REQ_DUMP => {
            let kind = arg(r.query);
            let tab = r.dump_tab;
            text_res(
                v,
                r.seq,
                RES_DUMP,
                |out| out.dump_tab = tab,
                |p| kind.and_then(|kind| p.dump(kind)),
            );
        }
        REQ_WEBGL => {
            if let Some(origin) = arg(r.url) {
                with_renderer(v, |p| p.resolve_webgl(origin, r.mods != 0));
            }
        }
        REQ_PRINT => do_print(v, r),
        REQ_CAMERA => {
            if let Some(origin) = arg(r.url) {
                with_renderer(v, |p| p.resolve_camera(origin, r.mods != 0));
            }
        }
        REQ_COLOR_SCHEME => {
            with_renderer(v, |p| p.set_color_scheme(r.mods != 0));
        }
        REQ_FAVICON => do_favicon(v, r),
        REQ_VIDEO_EVENT => do_video_event(v, r),
        _ => {}
    }
}

unsafe extern "C" fn worker_main(data: *mut c_void) -> *mut c_void {
    let v = data.cast::<NsProcView>();
    loop {
        let req = unsafe { g_async_queue_pop((*v).queue) }.cast::<Req>();
        if unsafe { (*req).r#type } == REQ_QUIT {
            free_req(req);
            break;
        }
        if proc_of(v).is_null() && !closed(v) {
            drop(install(v, spawn(v)));
        }
        let dpr = unsafe { (*req).dpr };
        if dpr > 0.0 {
            with_renderer(v, |p| p.set_device_pixel_ratio(dpr));
        }
        handle(v, req);
        free_req(req);
    }
    if !proc_of(v).is_null() {
        drop(install(v, None));
    }
    ns_proc_view_unref(v);
    ptr::null_mut()
}

pub(super) fn start_worker(v: *mut NsProcView) -> *mut c_void {
    unsafe {
        gtk::g_thread_new(
            c"ns-proc-view".as_ptr(),
            worker_main,
            ns_proc_view_ref(v).cast(),
        )
    }
}

pub(super) fn ns_proc_renderer_path() -> *mut c_char {
    let env = unsafe { glib::g_getenv(c"NS_RENDERER".as_ptr()) };
    if unsafe { glib::bytes(env) }.is_some_and(|e| !e.is_empty()) {
        return unsafe { glib::g_strdup(env) };
    }
    let name = if cfg!(windows) {
        c"southstar-renderer.exe"
    } else {
        c"southstar-renderer"
    };
    let exe = ns_app_self_exe();
    if !exe.is_null() {
        unsafe {
            let dir = glib::g_path_get_dirname(exe);
            let parent = build_filename(&[c"..".as_ptr(), name.as_ptr()]);
            for rel in [name.as_ptr(), parent.cast_const()] {
                let cand = build_filename(&[dir.cast_const(), rel]);
                if glib::g_file_test(cand, FILE_TEST_IS_EXECUTABLE) != 0 {
                    glib::g_free(parent.cast());
                    glib::g_free(dir.cast());
                    return cand;
                }
                glib::g_free(cand.cast());
            }
            glib::g_free(parent.cast());
            glib::g_free(dir.cast());
        }
    }
    unsafe { glib::g_strdup(name.as_ptr()) }
}

fn build_filename(parts: &[*const c_char]) -> *mut c_char {
    let mut args: Vec<*mut c_char> = parts.iter().map(|p| p.cast_mut()).collect();
    args.push(ptr::null_mut());
    unsafe { glib::g_build_filenamev(args.as_mut_ptr()) }
}
