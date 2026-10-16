//! Southstar — the tab view's worker thread, which carries each request to the renderer process and posts its reply to the main loop.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk_ffi as gtk;

use super::pagelayers::ns_layer_update_parse;
use super::procview_draw::ns_pv_stage_fill;
use super::procview_result::on_result;
use super::procview_state::{NsProcView, PrintSetup, Req, Res, TilesReq, Wheel};
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

#[repr(C)]
struct Page {
    ok: c_int,
    page_width: c_int,
    page_height: c_int,
    title: *mut c_char,
    url: *mut c_char,
    nav: *mut c_char,
    security: c_int,
    remote_ip: *mut c_char,
}

#[repr(C)]
struct Frame {
    ok: c_int,
    width: c_int,
    height: c_int,
    stride: c_int,
    animating: c_int,
    caret_blinking: c_int,
    wheel_snapped: c_int,
    page_w: c_int,
    page_h: c_int,
    scroll_y: c_int,
    scroll_x: c_int,
    unchanged: c_int,
    render_rc: c_int,
    pixels: *const u8,
    nav: *mut c_char,
    webgl: *mut c_char,
    camera: *mut c_char,
    download: *mut c_char,
    audio: *mut c_char,
    window_action: *mut c_char,
    clipboard: c_int,
    tiles: *mut c_char,
}

#[repr(C)]
struct Tick {
    ok: c_int,
    changed: c_int,
    animating: c_int,
    page_w: c_int,
    page_h: c_int,
    nav: *mut c_char,
    webgl: *mut c_char,
    camera: *mut c_char,
    download: *mut c_char,
    audio: *mut c_char,
    window_action: *mut c_char,
    title: *mut c_char,
    url: *mut c_char,
    url_pushed: c_int,
}

unsafe extern "C" {
    fn g_async_queue_pop(queue: *mut c_void) -> *mut c_void;
    fn g_mutex_lock(mutex: *mut c_void);
    fn g_mutex_unlock(mutex: *mut c_void);
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_strsplit(
        string: *const c_char,
        delimiter: *const c_char,
        max_tokens: c_int,
    ) -> *mut *mut c_char;
    fn g_strv_length(str_array: *mut *mut c_char) -> c_uint;
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
    fn free(p: *mut c_void);

    fn ns_app_self_exe() -> *const c_char;

    fn ns_rproc_http_spawn_shm_ex(
        renderer_path: *const c_char,
        max_width: c_int,
        max_height: c_int,
        private_mode: c_int,
    ) -> *mut c_void;
    fn ns_rproc_http_set_device_pixel_ratio(r: *mut c_void, dpr: f64);
    fn ns_rproc_http_open_ex(
        r: *mut c_void,
        url: *const c_char,
        viewport_width: c_int,
        viewport_height: c_int,
        settle_ms: c_int,
        history: c_int,
        user_activated: c_int,
        out: *mut Page,
    ) -> c_int;
    fn ns_rproc_http_render_wheel(
        r: *mut c_void,
        width: c_int,
        height: c_int,
        scroll_x: c_int,
        scroll_y: c_int,
        scale: f64,
        caret_active: c_int,
        wheel: *const Wheel,
        out: *mut Frame,
    ) -> c_int;
    fn ns_rproc_http_render_tiles(
        r: *mut c_void,
        width: c_int,
        height: c_int,
        scroll_x: c_int,
        scroll_y: c_int,
        scale: f64,
        caret_active: c_int,
        wheel: *const Wheel,
        tiles: *const TilesReq,
        out: *mut Frame,
    ) -> c_int;
    fn ns_rproc_http_map_size(r: *const c_void) -> usize;
    fn ns_rproc_http_tick_page(r: *mut c_void, out: *mut Tick) -> c_int;
    fn ns_rproc_http_tick_clear(out: *mut Tick);
    fn ns_rproc_http_link_at(r: *mut c_void, x: c_int, y: c_int) -> *mut c_char;
    fn ns_rproc_http_link_cursor_at(
        r: *mut c_void,
        x: c_int,
        y: c_int,
        out_cursor: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_rproc_http_clipboard(r: *mut c_void) -> *mut c_char;
    fn ns_rproc_http_click(r: *mut c_void, x: c_int, y: c_int, mods: c_int) -> *mut c_char;
    fn ns_rproc_http_select(r: *mut c_void, kind: c_int, x: c_int, y: c_int) -> *mut c_char;
    fn ns_rproc_http_key_full(
        r: *mut c_void,
        kind: c_int,
        key: *const c_char,
        code: *const c_char,
        keycode: c_int,
        mods: c_int,
        out_prevented: *mut c_int,
    ) -> *mut c_char;
    fn ns_rproc_http_hover_full(
        r: *mut c_void,
        x: c_int,
        y: c_int,
        out_href: *mut *mut c_char,
        out_cursor: *mut *mut c_char,
    ) -> c_int;
    fn ns_rproc_http_scrollbar(r: *mut c_void, kind: c_int, x: c_int, y: c_int) -> c_int;
    fn ns_rproc_http_drop_files(
        r: *mut c_void,
        x: c_int,
        y: c_int,
        paths: *const *const c_char,
        n: c_int,
    ) -> c_int;
    fn ns_rproc_http_release_full(r: *mut c_void, out_changed: *mut c_int) -> *mut c_char;
    fn ns_rproc_http_find(
        r: *mut c_void,
        query: *const c_char,
        case_sensitive: c_int,
        direction: c_int,
        from_y: c_int,
        out_total: *mut c_int,
        out_current: *mut c_int,
        out_scroll_y: *mut c_int,
    ) -> c_int;
    fn ns_rproc_http_set_viewport(
        r: *mut c_void,
        width: c_int,
        height: c_int,
        out: *mut Page,
    ) -> c_int;
    fn ns_rproc_http_resolve_camera(r: *mut c_void, origin: *const c_char, allow: c_int) -> c_int;
    fn ns_rproc_http_resolve_webgl(r: *mut c_void, origin: *const c_char, allow: c_int) -> c_int;
    fn ns_rproc_http_set_color_scheme(r: *mut c_void, dark: c_int) -> c_int;
    fn ns_rproc_http_video_event(
        r: *mut c_void,
        token: *const c_char,
        kind: *const c_char,
    ) -> c_int;
    fn ns_rproc_http_eval(r: *mut c_void, src: *const c_char) -> *mut c_char;
    fn ns_rproc_http_dump(r: *mut c_void, kind: *const c_char) -> *mut c_char;
    fn ns_rproc_http_console_poll(r: *mut c_void) -> *mut c_char;
    fn ns_rproc_http_media_at(
        r: *mut c_void,
        x: c_int,
        y: c_int,
        out_is_video: *mut c_int,
        out_stream: *mut c_int,
    ) -> *mut c_char;
    fn ns_rproc_http_contextmenu(
        r: *mut c_void,
        x: c_int,
        y: c_int,
        out_prevented: *mut c_int,
        out_edit: *mut c_int,
    );
    fn ns_rproc_http_export(r: *mut c_void, path: *const c_char) -> c_int;
    fn ns_rproc_http_favicon(
        r: *mut c_void,
        out_w: *mut c_int,
        out_h: *mut c_int,
        out_stride: *mut c_int,
    ) -> *mut u8;
    fn ns_rproc_http_print(
        r: *mut c_void,
        out_setup: *mut PrintSetup,
        out_scale: *mut f64,
    ) -> *mut c_void;
    fn ns_rproc_http_page_clear(out: *mut Page);
    fn ns_rproc_http_close(r: *mut c_void);
    pub(super) fn ns_rproc_http_interrupt(r: *mut c_void);
    pub(super) fn ns_rproc_http_terminate(r: *mut c_void);
    pub(super) fn ns_rproc_http_pid(r: *mut c_void) -> c_int;
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

fn take_malloced(s: *mut c_char) -> *mut c_char {
    if s.is_null() {
        return ptr::null_mut();
    }
    let copy = unsafe { glib::g_strdup(s) };
    unsafe { free(s.cast()) };
    copy
}

fn proc_of(v: *mut NsProcView) -> *mut c_void {
    unsafe { (*v).proc }
}

pub(super) fn lock_proc(v: *mut NsProcView) {
    unsafe { g_mutex_lock((&raw mut (*v).proc_lock).cast()) };
}

pub(super) fn unlock_proc(v: *mut NsProcView) {
    unsafe { g_mutex_unlock((&raw mut (*v).proc_lock).cast()) };
}

fn swap_proc(v: *mut NsProcView, next: *mut c_void) -> *mut c_void {
    lock_proc(v);
    let old = unsafe { (*v).proc };
    unsafe { (*v).proc = next };
    unlock_proc(v);
    old
}

fn spawn(v: *mut NsProcView) -> *mut c_void {
    unsafe {
        ns_rproc_http_spawn_shm_ex(
            (*v).renderer_path,
            nav::MAX_WIDTH,
            nav::MAX_HEIGHT,
            (*v).private_mode,
        )
    }
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

fn open_page(v: *mut NsProcView, r: &Req, settle: c_int, pg: &mut Page) -> c_int {
    let p = proc_of(v);
    if p.is_null() {
        return -1;
    }
    unsafe {
        ns_rproc_http_open_ex(
            p,
            r.url,
            r.vw,
            r.vh,
            settle,
            r.history,
            r.user_activated,
            pg,
        )
    }
}

fn empty_page() -> Page {
    Page {
        ok: 0,
        page_width: 0,
        page_height: 0,
        title: ptr::null_mut(),
        url: ptr::null_mut(),
        nav: ptr::null_mut(),
        security: 0,
        remote_ip: ptr::null_mut(),
    }
}

fn do_load(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_PAGE, r.seq);
    let mut pg = empty_page();
    let settle = settle_ms();
    let mut rc = open_page(v, r, settle, &mut pg);
    if rc != 0 && !proc_of(v).is_null() && unsafe { (*v).closed } == 0 {
        unsafe { ns_rproc_http_close(swap_proc(v, ptr::null_mut())) };
        swap_proc(v, spawn(v));
        if !proc_of(v).is_null() {
            unsafe { ns_rproc_http_set_device_pixel_ratio(proc_of(v), r.dpr) };
        }
        rc = open_page(v, r, settle, &mut pg);
    }
    if rc == 0 && pg.ok != 0 {
        unsafe {
            let out = &mut *res;
            out.ok = 1;
            out.pw = pg.page_width;
            out.ph = pg.page_height;
            out.title = dup_or(pg.title, c"".as_ptr());
            out.url = dup_or(pg.url, r.url);
            out.nav = dup(pg.nav);
            out.security = pg.security;
            out.remote_ip = dup(pg.remote_ip);
        }
    }
    if rc == 0 {
        unsafe { ns_rproc_http_page_clear(&mut pg) };
    }
    post(res);
}

fn empty_frame() -> Frame {
    Frame {
        ok: 0,
        width: 0,
        height: 0,
        stride: 0,
        animating: 0,
        caret_blinking: 0,
        wheel_snapped: 0,
        page_w: 0,
        page_h: 0,
        scroll_y: 0,
        scroll_x: 0,
        unchanged: 0,
        render_rc: 0,
        pixels: ptr::null(),
        nav: ptr::null_mut(),
        webgl: ptr::null_mut(),
        camera: ptr::null_mut(),
        download: ptr::null_mut(),
        audio: ptr::null_mut(),
        window_action: ptr::null_mut(),
        clipboard: 0,
        tiles: ptr::null_mut(),
    }
}

fn do_render(v: *mut NsProcView, r: &mut Req) {
    let res = new_res(v, RES_FRAME, r.seq);
    let mut fr = empty_frame();
    r.tiles.have = r.have;
    r.tiles.hold = r.hold;
    let p = proc_of(v);
    let rendered = !p.is_null()
        && unsafe {
            if r.tiles.tile_h > 0 {
                ns_rproc_http_render_tiles(
                    p,
                    r.w,
                    r.h,
                    r.sx,
                    r.sy,
                    r.scale,
                    r.caret_active,
                    &r.wheel,
                    &r.tiles,
                    &mut fr,
                )
            } else {
                ns_rproc_http_render_wheel(
                    p,
                    r.w,
                    r.h,
                    r.sx,
                    r.sy,
                    r.scale,
                    r.caret_active,
                    &r.wheel,
                    &mut fr,
                )
            }
        } == 0
        && fr.ok != 0;
    if rendered {
        let out = unsafe { &mut *res };
        out.ok = 1;
        out.animating = glib::boolean(fr.animating != 0);
        out.caret_blinking = glib::boolean(fr.caret_blinking != 0);
        out.wheel_snapped = glib::boolean(fr.wheel_snapped != 0);
        out.pw = fr.page_w;
        out.ph = fr.page_h;
        out.requested_scroll_y = fr.scroll_y;
        out.requested_scroll_x = fr.scroll_x;
        out.frame_unchanged = glib::boolean(fr.unchanged != 0);
        if !fr.tiles.is_null() {
            out.layers = unsafe {
                ns_layer_update_parse(fr.tiles, fr.pixels, ns_rproc_http_map_size(proc_of(v)))
            }
            .cast();
            unsafe { free(fr.tiles.cast()) };
            out.frame_unchanged = glib::boolean(out.layers.is_null());
        } else if fr.unchanged == 0 {
            out.texture =
                unsafe { ns_pv_stage_fill(fr.pixels, fr.width, fr.height, fr.stride) }.cast();
            out.texture_scale = if r.raster > 0.0 { r.raster } else { 1.0 };
        }
        out.nav = take_malloced(fr.nav);
        out.webgl = take_malloced(fr.webgl);
        out.camera = take_malloced(fr.camera);
        out.download = take_malloced(fr.download);
        out.audio = take_malloced(fr.audio);
        out.window_action = take_malloced(fr.window_action);
        if fr.clipboard != 0 && !proc_of(v).is_null() {
            out.clipboard = unsafe { ns_rproc_http_clipboard(proc_of(v)) };
        }
    } else if !proc_of(v).is_null() {
        unsafe { ns_rproc_http_close(swap_proc(v, ptr::null_mut())) };
    }
    post(res);
}

fn do_tick(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_TICK, r.seq);
    let mut tick = Tick {
        ok: 0,
        changed: 0,
        animating: 0,
        page_w: 0,
        page_h: 0,
        nav: ptr::null_mut(),
        webgl: ptr::null_mut(),
        camera: ptr::null_mut(),
        download: ptr::null_mut(),
        audio: ptr::null_mut(),
        window_action: ptr::null_mut(),
        title: ptr::null_mut(),
        url: ptr::null_mut(),
        url_pushed: 0,
    };
    let p = proc_of(v);
    if !p.is_null() && unsafe { ns_rproc_http_tick_page(p, &mut tick) } == 0 {
        let out = unsafe { &mut *res };
        out.ok = glib::boolean(tick.ok != 0);
        out.kind = tick.changed;
        out.animating = glib::boolean(tick.animating != 0);
        out.pw = tick.page_w;
        out.ph = tick.page_h;
        out.nav = dup(tick.nav);
        out.webgl = dup(tick.webgl);
        out.camera = dup(tick.camera);
        out.download = dup(tick.download);
        out.audio = dup(tick.audio);
        out.window_action = dup(tick.window_action);
        out.title = dup(tick.title);
        out.url = dup(tick.url);
        out.url_pushed = glib::boolean(tick.url_pushed != 0);
        unsafe { ns_rproc_http_tick_clear(&mut tick) };
    }
    post(res);
}

fn do_link(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_LINK, r.seq);
    let out = unsafe { &mut *res };
    out.action = r.action;
    let p = proc_of(v);
    if !p.is_null() && r.action == ACT_HOVER {
        out.href = unsafe { ns_rproc_http_link_cursor_at(p, r.x, r.y, &mut out.cursor) };
    } else if !p.is_null() && r.action == ACT_CONTEXT {
        let mut prevented = 0;
        unsafe { ns_rproc_http_contextmenu(p, r.x, r.y, &mut prevented, &mut out.edit) };
        out.prevented = prevented;
        if prevented == 0 {
            out.href = unsafe { ns_rproc_http_link_at(p, r.x, r.y) };
        }
    } else if !p.is_null() {
        out.href = unsafe { ns_rproc_http_link_at(p, r.x, r.y) };
    }
    post(res);
}

fn do_viewport(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_VIEWPORT, r.seq);
    let mut pg = empty_page();
    let p = proc_of(v);
    if !p.is_null() && unsafe { ns_rproc_http_set_viewport(p, r.vw, r.vh, &mut pg) } == 0 {
        let out = unsafe { &mut *res };
        out.ok = pg.ok;
        out.pw = pg.page_width;
        out.ph = pg.page_height;
        unsafe { ns_rproc_http_page_clear(&mut pg) };
    }
    post(res);
}

fn do_key(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_KEY, r.seq);
    let out = unsafe { &mut *res };
    out.kind = r.kind;
    out.fallback_scroll = r.fallback_scroll;
    out.fallback_x = r.fallback_x;
    out.fallback_y = r.fallback_y;
    let p = proc_of(v);
    out.href = if p.is_null() {
        ptr::null_mut()
    } else {
        unsafe {
            ns_rproc_http_key_full(
                p,
                r.kind,
                r.key,
                r.code,
                r.keycode,
                r.mods,
                &mut out.prevented,
            )
        }
    };
    post(res);
}

fn do_select(v: *mut NsProcView, r: &Req) {
    let kind = if r.kind == 4 || r.kind == 7 {
        RES_COPY
    } else {
        RES_SELECT
    };
    let res = new_res(v, kind, r.seq);
    let out = unsafe { &mut *res };
    out.kind = r.kind;
    let p = proc_of(v);
    out.href = if p.is_null() {
        ptr::null_mut()
    } else {
        unsafe { ns_rproc_http_select(p, r.kind, r.x, r.y) }
    };
    post(res);
}

fn do_dropfiles(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_DROPFILES, r.seq);
    let p = proc_of(v);
    if !p.is_null() && !r.paths.is_null() && unsafe { *r.paths } != 0 {
        let list = unsafe { g_strsplit(r.paths, c"\n".as_ptr(), -1) };
        let count = if list.is_null() {
            0
        } else {
            unsafe { g_strv_length(list) }
        };
        if count > 0 {
            unsafe {
                (*res).ok = glib::boolean(
                    ns_rproc_http_drop_files(
                        p,
                        r.x,
                        r.y,
                        list.cast::<*const c_char>(),
                        count as c_int,
                    ) == 1,
                );
            }
        }
        unsafe { glib::g_strfreev(list) };
    }
    post(res);
}

fn do_release(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_RELEASE, r.seq);
    let out = unsafe { &mut *res };
    let p = proc_of(v);
    out.href = if p.is_null() {
        ptr::null_mut()
    } else {
        unsafe { ns_rproc_http_release_full(p, &mut out.ok) }
    };
    if !p.is_null() && (out.href.is_null() || unsafe { *out.href } == 0) {
        out.media_url = unsafe {
            ns_rproc_http_media_at(p, r.x, r.y, &mut out.media_is_video, &mut out.media_stream)
        };
    }
    post(res);
}

fn do_export(v: *mut NsProcView, r: &Req) {
    let res = new_res(v, RES_EXPORT, r.seq);
    let mut ok = false;
    let p = proc_of(v);
    if !p.is_null()
        && !r.url.is_null()
        && !r.export_dest.is_null()
        && unsafe { ns_rproc_http_export(p, r.url) } == 0
    {
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
    unsafe {
        (*res).ok = glib::boolean(ok);
        (*res).url = dup_or(r.export_dest, c"".as_ptr());
    }
    post(res);
}

fn text_res(
    v: *mut NsProcView,
    seq: c_int,
    kind: c_int,
    fill: impl FnOnce(&mut Res),
    ask: impl FnOnce(*mut c_void) -> *mut c_char,
) {
    let res = new_res(v, kind, seq);
    let p = proc_of(v);
    unsafe {
        fill(&mut *res);
        (*res).href = if p.is_null() { ptr::null_mut() } else { ask(p) };
    }
    post(res);
}

fn handle(v: *mut NsProcView, req: *mut Req) {
    let r = unsafe { &mut *req };
    match r.r#type {
        REQ_LOAD => do_load(v, r),
        REQ_RENDER => do_render(v, r),
        REQ_TICK => do_tick(v, r),
        REQ_LINK => do_link(v, r),
        REQ_CLICK => {
            let (x, y, mods) = (r.x, r.y, r.mods);
            text_res(
                v,
                r.seq,
                RES_CLICK,
                |_| {},
                |p| unsafe { ns_rproc_http_click(p, x, y, mods) },
            );
        }
        REQ_VIEWPORT => do_viewport(v, r),
        REQ_KEY => do_key(v, r),
        REQ_SELECT => do_select(v, r),
        REQ_HOVER => {
            let res = new_res(v, RES_HOVER, r.seq);
            let p = proc_of(v);
            if !p.is_null() {
                unsafe {
                    let out = &mut *res;
                    out.ok = glib::boolean(
                        ns_rproc_http_hover_full(p, r.x, r.y, &mut out.href, &mut out.cursor) == 1,
                    );
                }
            }
            post(res);
        }
        REQ_SCROLLBAR => {
            let res = new_res(v, RES_SCROLLBAR, r.seq);
            let p = proc_of(v);
            unsafe {
                (*res).kind = r.kind;
                (*res).ok = if p.is_null() {
                    0
                } else {
                    ns_rproc_http_scrollbar(p, r.kind, r.x, r.y)
                };
            }
            post(res);
        }
        REQ_DROPFILES => do_dropfiles(v, r),
        REQ_RELEASE => do_release(v, r),
        REQ_FIND => {
            let res = new_res(v, RES_FIND, r.seq);
            let p = proc_of(v);
            if !p.is_null() {
                unsafe {
                    let out = &mut *res;
                    ns_rproc_http_find(
                        p,
                        r.query,
                        r.find_case,
                        r.find_dir,
                        r.find_from_y,
                        &mut out.find_total,
                        &mut out.find_current,
                        &mut out.find_scroll_y,
                    );
                }
            }
            post(res);
        }
        REQ_EXPORT => do_export(v, r),
        REQ_CONSOLE => text_res(
            v,
            r.seq,
            RES_CONSOLE,
            |_| {},
            |p| unsafe { ns_rproc_http_console_poll(p) },
        ),
        REQ_EVAL => {
            let query = r.query;
            let (tab, inspect) = (r.dump_tab, r.inspect);
            text_res(
                v,
                r.seq,
                RES_EVAL,
                |out| {
                    out.dump_tab = tab;
                    out.inspect = inspect;
                },
                |p| unsafe { ns_rproc_http_eval(p, query) },
            );
        }
        REQ_DUMP => {
            let query = r.query;
            let tab = r.dump_tab;
            text_res(
                v,
                r.seq,
                RES_DUMP,
                |out| out.dump_tab = tab,
                |p| unsafe { ns_rproc_http_dump(p, query) },
            );
        }
        REQ_WEBGL => {
            let p = proc_of(v);
            if !p.is_null() {
                unsafe { ns_rproc_http_resolve_webgl(p, r.url, r.mods) };
            }
        }
        REQ_PRINT => {
            let res = new_res(v, RES_PRINT, r.seq);
            let p = proc_of(v);
            unsafe {
                let out = &mut *res;
                if !p.is_null() {
                    out.print_pages =
                        ns_rproc_http_print(p, &mut out.print_setup, &mut out.print_scale);
                }
                out.ok = glib::boolean(!out.print_pages.is_null());
            }
            post(res);
        }
        REQ_CAMERA => {
            let p = proc_of(v);
            if !p.is_null() {
                unsafe { ns_rproc_http_resolve_camera(p, r.url, r.mods) };
            }
        }
        REQ_COLOR_SCHEME => {
            let p = proc_of(v);
            if !p.is_null() {
                unsafe { ns_rproc_http_set_color_scheme(p, r.mods) };
            }
        }
        REQ_FAVICON => {
            let res = new_res(v, RES_FAVICON, r.seq);
            let p = proc_of(v);
            if !p.is_null() {
                unsafe {
                    let out = &mut *res;
                    out.favicon_data = ns_rproc_http_favicon(
                        p,
                        &mut out.favicon_w,
                        &mut out.favicon_h,
                        &mut out.favicon_stride,
                    );
                }
            }
            post(res);
        }
        REQ_VIDEO_EVENT => {
            let p = proc_of(v);
            let rc = if !p.is_null() && r.seq == unsafe { (*v).load_seq } {
                unsafe { ns_rproc_http_video_event(p, r.key, r.query) }
            } else {
                -1
            };
            if rc == 0 {
                unsafe { gtk::g_idle_add(video_event_render, ns_proc_view_ref(v).cast()) };
            }
        }
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
        if proc_of(v).is_null() && unsafe { (*v).closed } == 0 {
            swap_proc(v, spawn(v));
        }
        let dpr = unsafe { (*req).dpr };
        if !proc_of(v).is_null() && dpr > 0.0 {
            unsafe { ns_rproc_http_set_device_pixel_ratio(proc_of(v), dpr) };
        }
        handle(v, req);
        free_req(req);
    }
    if !proc_of(v).is_null() {
        unsafe { ns_rproc_http_close(swap_proc(v, ptr::null_mut())) };
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
    let exe = unsafe { ns_app_self_exe() };
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
