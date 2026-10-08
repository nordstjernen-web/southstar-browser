//! Southstar — the C ABI of the renderer client, as declared in src/rproc_http.h and src/print.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long, c_void};
use core::ptr;
use std::sync::Mutex;

use southstar_glib::{self as glib, GPtrArray};

use crate::client::{Frame, Page, Renderer, Tick, TilesRequest, View, Wheel};
use crate::os::{self, AttachFn};

#[repr(C)]
pub struct CPage {
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
pub struct CFrame {
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
pub struct CTick {
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

#[repr(C)]
pub struct CWheel {
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
    viewport: c_int,
}

#[repr(C)]
pub struct CTilesRequest {
    tile_h: c_int,
    want_y0: c_int,
    want_y1: c_int,
    generation: c_int,
    vp_held: c_int,
    fill: c_int,
    have: *const c_char,
    hold: *const c_char,
}

#[repr(C)]
pub struct PrintSetup {
    width: f64,
    height: f64,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
    margin_left: f64,
}

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

pub type PrintFn =
    unsafe extern "C" fn(conn: *mut c_void, out_setup: *mut c_void) -> *mut GPtrArray;

static PRINT: Mutex<Option<PrintFn>> = Mutex::new(None);

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_get_monotonic_time() -> i64;
    fn g_ptr_array_new() -> *mut GPtrArray;
    fn g_ptr_array_add(array: *mut GPtrArray, data: *mut c_void);
    fn g_ptr_array_free(array: *mut GPtrArray, free_segment: c_int) -> *mut *mut c_void;
    fn g_remove(filename: *const c_char) -> c_int;
    fn ns_print_setup_default(setup: *mut PrintSetup);
    fn cairo_image_surface_create_from_png(filename: *const c_char) -> *mut CairoSurface;
    fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

fn malloc_bytes(bytes: &[u8]) -> *mut c_char {
    let out = unsafe { malloc(bytes.len() + 1) }.cast::<u8>();
    if !out.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            *out.add(bytes.len()) = 0;
        }
    }
    out.cast()
}

fn owned(value: Option<Vec<u8>>) -> *mut c_char {
    value.map_or(ptr::null_mut(), |v| malloc_bytes(&v))
}

unsafe fn release(text: &mut *mut c_char) {
    unsafe { free((*text).cast()) };
    *text = ptr::null_mut();
}

unsafe fn bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(text) }
}

unsafe fn renderer<'a>(r: *mut Renderer) -> Option<&'a mut Renderer> {
    unsafe { r.as_mut() }
}

fn page_out(page: Page) -> CPage {
    CPage {
        ok: c_int::from(page.ok),
        page_width: page.page_width,
        page_height: page.page_height,
        title: owned(page.title),
        url: owned(page.url),
        nav: owned(page.nav),
        security: page.security,
        remote_ip: owned(page.remote_ip),
    }
}

fn empty_page() -> CPage {
    page_out(Page::default())
}

fn frame_out(frame: Frame) -> CFrame {
    CFrame {
        ok: c_int::from(frame.ok),
        width: frame.width,
        height: frame.height,
        stride: frame.stride,
        animating: c_int::from(frame.animating),
        caret_blinking: c_int::from(frame.caret_blinking),
        wheel_snapped: c_int::from(frame.wheel_snapped),
        page_w: frame.page_w,
        page_h: frame.page_h,
        scroll_y: frame.scroll_y,
        scroll_x: frame.scroll_x,
        unchanged: c_int::from(frame.unchanged),
        render_rc: frame.render_rc,
        pixels: frame.pixels,
        nav: owned(frame.nav),
        webgl: owned(frame.webgl),
        camera: owned(frame.camera),
        download: owned(frame.download),
        audio: owned(frame.audio),
        window_action: owned(frame.window_action),
        clipboard: c_int::from(frame.clipboard),
        tiles: owned(frame.tiles),
    }
}

fn tick_out(tick: Tick) -> CTick {
    CTick {
        ok: c_int::from(tick.ok),
        changed: c_int::from(tick.changed),
        animating: c_int::from(tick.animating),
        page_w: tick.page_w,
        page_h: tick.page_h,
        nav: owned(tick.nav),
        webgl: owned(tick.webgl),
        camera: owned(tick.camera),
        download: owned(tick.download),
        audio: owned(tick.audio),
        window_action: owned(tick.window_action),
        title: owned(tick.title),
        url: owned(tick.url),
        url_pushed: c_int::from(tick.url_pushed),
    }
}

unsafe fn spawn(
    path: *const c_char,
    max_w: c_int,
    max_h: c_int,
    shm: bool,
    private: bool,
) -> *mut Renderer {
    os::spawn(unsafe { bytes(path) }, max_w, max_h, shm, private)
        .map_or(ptr::null_mut(), |r| Box::into_raw(Box::new(r)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_spawn(
    path: *const c_char,
    max_w: c_int,
    max_h: c_int,
) -> *mut Renderer {
    unsafe { spawn(path, max_w, max_h, false, false) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_spawn_shm(
    path: *const c_char,
    max_w: c_int,
    max_h: c_int,
) -> *mut Renderer {
    unsafe { spawn(path, max_w, max_h, true, false) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_spawn_shm_ex(
    path: *const c_char,
    max_w: c_int,
    max_h: c_int,
    private_mode: c_int,
) -> *mut Renderer {
    unsafe { spawn(path, max_w, max_h, true, private_mode != 0) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_http_set_inproc(attach: Option<AttachFn>) {
    os::set_attach(attach);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_http_set_inproc_print(print: Option<PrintFn>) {
    *PRINT.lock().unwrap_or_else(|e| e.into_inner()) = print;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_set_device_pixel_ratio(r: *mut Renderer, dpr: f64) {
    if let Some(r) = unsafe { renderer(r) } {
        r.set_device_pixel_ratio(dpr);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_open(
    r: *mut Renderer,
    url: *const c_char,
    width: c_int,
    height: c_int,
    settle_ms: c_int,
    out: *mut CPage,
) -> c_int {
    unsafe { ns_rproc_http_open_ex(r, url, width, height, settle_ms, 0, 1, out) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_rproc_http_open_ex(
    r: *mut Renderer,
    url: *const c_char,
    width: c_int,
    height: c_int,
    settle_ms: c_int,
    history: c_int,
    user_activated: c_int,
    out: *mut CPage,
) -> c_int {
    let (Some(r), Some(url)) = (unsafe { renderer(r) }, unsafe { bytes(url) }) else {
        return -1;
    };
    if out.is_null() {
        return -1;
    }
    unsafe { out.write(empty_page()) };
    match r.open(
        url,
        width,
        height,
        settle_ms,
        history != 0,
        user_activated != 0,
    ) {
        Some(page) => {
            unsafe { out.write(page_out(page)) };
            0
        }
        None => -1,
    }
}

fn view(
    width: c_int,
    height: c_int,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    caret: c_int,
) -> View {
    View {
        width,
        height,
        scroll_x,
        scroll_y,
        scale,
        caret: caret != 0,
    }
}

unsafe fn wheel(wheel: *const CWheel) -> Option<Wheel> {
    unsafe { wheel.as_ref() }.map(|w| Wheel {
        x: w.x,
        y: w.y,
        dx: w.dx,
        dy: w.dy,
        viewport: w.viewport != 0,
    })
}

fn frame_result(out: *mut CFrame, frame: Option<Frame>) -> c_int {
    let rc = if frame.is_some() { 0 } else { -1 };
    unsafe { out.write(frame_out(frame.unwrap_or_default())) };
    rc
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_rproc_http_render(
    r: *mut Renderer,
    width: c_int,
    height: c_int,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    caret_active: c_int,
    out: *mut CFrame,
) -> c_int {
    unsafe {
        ns_rproc_http_render_wheel(
            r,
            width,
            height,
            scroll_x,
            scroll_y,
            scale,
            caret_active,
            ptr::null(),
            out,
        )
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_rproc_http_render_wheel(
    r: *mut Renderer,
    width: c_int,
    height: c_int,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    caret_active: c_int,
    wheel_in: *const CWheel,
    out: *mut CFrame,
) -> c_int {
    let Some(r) = (unsafe { renderer(r) }) else {
        return -1;
    };
    if out.is_null() {
        return -1;
    }
    let wheel = unsafe { wheel(wheel_in) };
    let frame = r.render(
        view(width, height, scroll_x, scroll_y, scale, caret_active),
        wheel.as_ref(),
    );
    frame_result(out, frame)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_rproc_http_render_tiles(
    r: *mut Renderer,
    width: c_int,
    height: c_int,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    caret_active: c_int,
    wheel_in: *const CWheel,
    tiles: *const CTilesRequest,
    out: *mut CFrame,
) -> c_int {
    let shm = unsafe { r.as_ref() }.is_some_and(|r| r.map_size() > 0);
    let (Some(rr), Some(t)) = (unsafe { renderer(r) }, unsafe { tiles.as_ref() }) else {
        return unsafe {
            ns_rproc_http_render_wheel(
                r,
                width,
                height,
                scroll_x,
                scroll_y,
                scale,
                caret_active,
                wheel_in,
                out,
            )
        };
    };
    if out.is_null() || !shm {
        return unsafe {
            ns_rproc_http_render_wheel(
                r,
                width,
                height,
                scroll_x,
                scroll_y,
                scale,
                caret_active,
                wheel_in,
                out,
            )
        };
    }
    let wheel = unsafe { wheel(wheel_in) };
    let request = TilesRequest {
        tile_h: t.tile_h,
        want_y0: t.want_y0,
        want_y1: t.want_y1,
        generation: t.generation,
        vp_held: t.vp_held,
        fill: t.fill,
        have: unsafe { bytes(t.have) }.unwrap_or_default(),
        hold: unsafe { bytes(t.hold) }.unwrap_or_default(),
    };
    let frame = rr.render_tiles(
        view(width, height, scroll_x, scroll_y, scale, caret_active),
        wheel.as_ref(),
        &request,
    );
    frame_result(out, frame)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_map_size(r: *const Renderer) -> usize {
    unsafe { r.as_ref() }.map_or(0, Renderer::map_size)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_tick_page(r: *mut Renderer, out: *mut CTick) -> c_int {
    let Some(r) = (unsafe { renderer(r) }) else {
        return -1;
    };
    if out.is_null() {
        return -1;
    }
    unsafe { out.write(tick_out(Tick::default())) };
    match r.tick() {
        Some(tick) => {
            unsafe { out.write(tick_out(tick)) };
            0
        }
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_tick_clear(out: *mut CTick) {
    let Some(t) = (unsafe { out.as_mut() }) else {
        return;
    };
    unsafe {
        for text in [
            &mut t.nav,
            &mut t.webgl,
            &mut t.camera,
            &mut t.download,
            &mut t.audio,
            &mut t.window_action,
            &mut t.title,
            &mut t.url,
        ] {
            release(text);
        }
        out.write(tick_out(Tick::default()));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_link_at(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| {
        owned(r.xy_text("/link", x, y, None, "href"))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_link_cursor_at(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    out_cursor: *mut *mut c_char,
) -> *mut c_char {
    if !out_cursor.is_null() {
        unsafe { *out_cursor = ptr::null_mut() };
    }
    let Some(r) = (unsafe { renderer(r) }) else {
        return ptr::null_mut();
    };
    let Some((href, cursor)) = r.link_cursor_at(x, y) else {
        return ptr::null_mut();
    };
    if !out_cursor.is_null() {
        unsafe { *out_cursor = owned(cursor) };
    }
    owned(href)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_clipboard(r: *mut Renderer) -> *mut c_char {
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| owned(r.request("/clipboard", "{}")))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_click(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    mods: c_int,
) -> *mut c_char {
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| {
        owned(r.xy_text("/click", x, y, Some(("mods", mods)), "href"))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_select(
    r: *mut Renderer,
    kind: c_int,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| {
        owned(r.xy_text("/select", x, y, Some(("kind", kind)), "href"))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_key_full(
    r: *mut Renderer,
    kind: c_int,
    key: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
    out_prevented: *mut c_int,
) -> *mut c_char {
    if !out_prevented.is_null() {
        unsafe { *out_prevented = 0 };
    }
    let Some(r) = (unsafe { renderer(r) }) else {
        return ptr::null_mut();
    };
    let key = unsafe { bytes(key) }.unwrap_or_default();
    let code = unsafe { bytes(code) }.unwrap_or_default();
    let Some((href, prevented)) = r.key(kind, key, code, keycode, mods) else {
        return ptr::null_mut();
    };
    if !out_prevented.is_null() {
        unsafe { *out_prevented = c_int::from(prevented) };
    }
    owned(href)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_key(
    r: *mut Renderer,
    kind: c_int,
    key: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
) -> *mut c_char {
    unsafe { ns_rproc_http_key_full(r, kind, key, code, keycode, mods, ptr::null_mut()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_release_full(
    r: *mut Renderer,
    out_changed: *mut c_int,
) -> *mut c_char {
    if !out_changed.is_null() {
        unsafe { *out_changed = 0 };
    }
    let Some((href, changed)) = (unsafe { renderer(r) }).and_then(Renderer::release) else {
        return ptr::null_mut();
    };
    if !out_changed.is_null() {
        unsafe { *out_changed = c_int::from(changed) };
    }
    owned(href)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_focused_editable(r: *mut Renderer) -> c_int {
    unsafe { renderer(r) }.map_or(0, |r| c_int::from(r.focused_editable()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_focused_editable_value(
    r: *mut Renderer,
    out_caret: *mut usize,
    out_anchor: *mut usize,
) -> *mut c_char {
    unsafe {
        if !out_caret.is_null() {
            *out_caret = 0;
        }
        if !out_anchor.is_null() {
            *out_anchor = 0;
        }
    }
    let Some((value, caret, anchor)) =
        (unsafe { renderer(r) }).and_then(Renderer::focused_editable_value)
    else {
        return ptr::null_mut();
    };
    unsafe {
        if !out_caret.is_null() && caret > 0 {
            *out_caret = caret;
        }
        if !out_anchor.is_null() && anchor > 0 {
            *out_anchor = anchor;
        }
    }
    owned(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_set_focused_editable_selection(
    r: *mut Renderer,
    caret: usize,
    anchor: usize,
) -> c_int {
    unsafe { renderer(r) }.map_or(0, |r| {
        c_int::from(r.set_focused_editable_selection(caret, anchor))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_hover_full(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    out_href: *mut *mut c_char,
    out_cursor: *mut *mut c_char,
) -> c_int {
    unsafe {
        if !out_href.is_null() {
            *out_href = ptr::null_mut();
        }
        if !out_cursor.is_null() {
            *out_cursor = ptr::null_mut();
        }
    }
    let Some((changed, href, cursor)) = (unsafe { renderer(r) }).and_then(|r| r.hover(x, y)) else {
        return -1;
    };
    unsafe {
        if !out_href.is_null() {
            *out_href = owned(href);
        }
        if !out_cursor.is_null() {
            *out_cursor = owned(cursor);
        }
    }
    c_int::from(changed)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_scroll(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
) -> c_int {
    let json = format!("{{\"x\":{x},\"y\":{y},\"dx\":{dx},\"dy\":{dy}}}");
    unsafe { renderer(r) }.map_or(0, |r| c_int::from(r.flag("/scroll", &json, "consumed")))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_scrollbar(
    r: *mut Renderer,
    kind: c_int,
    x: c_int,
    y: c_int,
) -> c_int {
    let path = match kind {
        0 => "/scrollbar-press",
        1 => "/scrollbar-drag",
        _ => "/scrollbar-release",
    };
    let json = format!("{{\"x\":{x},\"y\":{y}}}");
    unsafe { renderer(r) }.map_or(0, |r| c_int::from(r.flag(path, &json, "hit")))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_rproc_http_find(
    r: *mut Renderer,
    query: *const c_char,
    case_sensitive: c_int,
    direction: c_int,
    from_y: c_int,
    out_total: *mut c_int,
    out_current: *mut c_int,
    out_scroll_y: *mut c_int,
) -> c_int {
    let outs = [out_total, out_current, out_scroll_y];
    for out in outs {
        if !out.is_null() {
            unsafe { *out = 0 };
        }
    }
    let Some(r) = (unsafe { renderer(r) }) else {
        return -1;
    };
    let query = unsafe { bytes(query) }.unwrap_or_default();
    let Some(values) = r.find(query, case_sensitive != 0, direction, from_y) else {
        return -1;
    };
    for (out, value) in outs.into_iter().zip(values) {
        if !out.is_null() {
            unsafe { *out = value };
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_drop_files(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    paths: *const *const c_char,
    n_paths: c_int,
) -> c_int {
    let Some(r) = (unsafe { renderer(r) }) else {
        return 0;
    };
    if paths.is_null() || n_paths <= 0 {
        return 0;
    }
    let list: Vec<&[u8]> = (0..n_paths as usize)
        .map(|i| unsafe { bytes(*paths.add(i)) }.unwrap_or_default())
        .collect();
    r.drop_files(x, y, &list)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_set_viewport(
    r: *mut Renderer,
    width: c_int,
    height: c_int,
    out: *mut CPage,
) -> c_int {
    if !out.is_null() {
        unsafe { out.write(empty_page()) };
    }
    let Some(r) = (unsafe { renderer(r) }) else {
        return -1;
    };
    let Some(page) = r.set_viewport(width, height) else {
        return -1;
    };
    let ok = page.ok;
    if !out.is_null() {
        unsafe { out.write(page_out(page)) };
    }
    if ok { 0 } else { -1 }
}

unsafe fn origin_decision(
    r: *mut Renderer,
    path: &str,
    origin: *const c_char,
    allow: c_int,
) -> c_int {
    let (Some(r), Some(origin)) = (unsafe { renderer(r) }, unsafe { bytes(origin) }) else {
        return -1;
    };
    if r.origin_decision(path, origin, allow != 0) {
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_resolve_camera(
    r: *mut Renderer,
    origin: *const c_char,
    allow: c_int,
) -> c_int {
    unsafe { origin_decision(r, "/camera", origin, allow) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_resolve_webgl(
    r: *mut Renderer,
    origin: *const c_char,
    allow: c_int,
) -> c_int {
    unsafe { origin_decision(r, "/webgl", origin, allow) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_set_color_scheme(r: *mut Renderer, dark: c_int) -> c_int {
    let Some(r) = (unsafe { renderer(r) }) else {
        return -1;
    };
    let json = format!("{{\"dark\":{}}}", c_int::from(dark != 0));
    if r.request("/colorscheme", &json).is_some() {
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_video_event(
    r: *mut Renderer,
    token: *const c_char,
    kind: *const c_char,
) -> c_int {
    let (Some(r), Some(token), Some(kind)) =
        (unsafe { renderer(r) }, unsafe { bytes(token) }, unsafe {
            bytes(kind)
        })
    else {
        return -1;
    };
    if r.video_event(token, kind) { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_eval(r: *mut Renderer, src: *const c_char) -> *mut c_char {
    let src = unsafe { bytes(src) }.unwrap_or_default();
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| {
        owned(r.text_request("/eval", "src", Some(src)))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_dump(r: *mut Renderer, kind: *const c_char) -> *mut c_char {
    let (Some(r), Some(kind)) = (unsafe { renderer(r) }, unsafe { bytes(kind) }) else {
        return ptr::null_mut();
    };
    owned(r.text_request("/dump", "kind", Some(kind)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_console_poll(r: *mut Renderer) -> *mut c_char {
    unsafe { renderer(r) }.map_or(ptr::null_mut(), |r| {
        owned(r.text_request("/console", "", None))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_media_at(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    out_is_video: *mut c_int,
    out_stream: *mut c_int,
) -> *mut c_char {
    unsafe {
        if !out_is_video.is_null() {
            *out_is_video = 0;
        }
        if !out_stream.is_null() {
            *out_stream = 0;
        }
    }
    let Some((url, is_video, stream)) = (unsafe { renderer(r) }).and_then(|r| r.media_at(x, y))
    else {
        return ptr::null_mut();
    };
    unsafe {
        if !out_is_video.is_null() {
            *out_is_video = is_video;
        }
        if !out_stream.is_null() {
            *out_stream = stream;
        }
    }
    owned(url)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_contextmenu(
    r: *mut Renderer,
    x: c_int,
    y: c_int,
    out_prevented: *mut c_int,
    out_edit: *mut c_int,
) {
    unsafe {
        if !out_prevented.is_null() {
            *out_prevented = 0;
        }
        if !out_edit.is_null() {
            *out_edit = 0;
        }
    }
    let Some((prevented, edit)) = (unsafe { renderer(r) }).and_then(|r| r.contextmenu(x, y)) else {
        return;
    };
    unsafe {
        if !out_prevented.is_null() {
            *out_prevented = prevented;
        }
        if !out_edit.is_null() {
            *out_edit = edit;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_export(r: *mut Renderer, path: *const c_char) -> c_int {
    let (Some(r), Some(path)) = (unsafe { renderer(r) }, unsafe { bytes(path) }) else {
        return -1;
    };
    if r.export(path) { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_print(
    r: *mut Renderer,
    out_setup: *mut PrintSetup,
    out_scale: *mut f64,
) -> *mut GPtrArray {
    let (Some(r), Some(setup)) = (unsafe { renderer(r) }, unsafe { out_setup.as_mut() }) else {
        return ptr::null_mut();
    };
    if !out_scale.is_null() {
        unsafe { *out_scale = 1.0 };
    }
    let hook = *PRINT.lock().unwrap_or_else(|e| e.into_inner());
    if let (false, Some(print)) = (r.inproc_conn.is_null(), hook) {
        return unsafe { print(r.inproc_conn, (setup as *mut PrintSetup).cast()) };
    }
    let mut prefix = unsafe { bytes(g_get_user_runtime_dir()) }
        .unwrap_or_default()
        .to_vec();
    prefix.extend_from_slice(
        format!(
            "/southstar-print-{}-{}",
            unsafe { g_get_monotonic_time() },
            os::next_print_counter()
        )
        .as_bytes(),
    );
    let Some(reply) = r.print(&prefix) else {
        return ptr::null_mut();
    };
    unsafe { ns_print_setup_default(setup) };
    let fields = [
        (&mut setup.width, reply.width),
        (&mut setup.height, reply.height),
        (&mut setup.margin_top, reply.margins[0]),
        (&mut setup.margin_right, reply.margins[1]),
        (&mut setup.margin_bottom, reply.margins[2]),
        (&mut setup.margin_left, reply.margins[3]),
    ];
    for (slot, value) in fields {
        if let Some(value) = value {
            *slot = value;
        }
    }
    let sheets = unsafe { g_ptr_array_new() };
    for i in 0..reply.pages.max(0) {
        let mut path = prefix.clone();
        path.extend_from_slice(format!("-{i}.png").as_bytes());
        let path = std::ffi::CString::new(path).unwrap_or_default();
        if unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_EXISTS) } == 0 {
            break;
        }
        let img = unsafe { cairo_image_surface_create_from_png(path.as_ptr()) };
        if unsafe { cairo_surface_status(img) } == 0 {
            unsafe { g_ptr_array_add(sheets, img.cast()) };
        } else {
            unsafe { cairo_surface_destroy(img) };
        }
        unsafe { g_remove(path.as_ptr()) };
    }
    if unsafe { (*sheets).len } == 0 {
        unsafe { g_ptr_array_free(sheets, 1) };
        return ptr::null_mut();
    }
    if !out_scale.is_null() && reply.scale > 0.0 {
        unsafe { *out_scale = reply.scale };
    }
    sheets
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_favicon(
    r: *mut Renderer,
    out_w: *mut c_int,
    out_h: *mut c_int,
    out_stride: *mut c_int,
) -> *mut u8 {
    let outs = [out_w, out_h, out_stride];
    for out in outs {
        if !out.is_null() {
            unsafe { *out = 0 };
        }
    }
    let Some(icon) = (unsafe { renderer(r) }).and_then(Renderer::favicon) else {
        return ptr::null_mut();
    };
    let pixels = unsafe { malloc(icon.pixels.len()) }.cast::<u8>();
    if pixels.is_null() {
        return ptr::null_mut();
    }
    unsafe { ptr::copy_nonoverlapping(icon.pixels.as_ptr(), pixels, icon.pixels.len()) };
    for (out, value) in outs.into_iter().zip([icon.width, icon.height, icon.stride]) {
        if !out.is_null() {
            unsafe { *out = value };
        }
    }
    pixels
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_page_clear(out: *mut CPage) {
    let Some(page) = (unsafe { out.as_mut() }) else {
        return;
    };
    unsafe {
        release(&mut page.title);
        release(&mut page.url);
        release(&mut page.nav);
        release(&mut page.remote_ip);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_close(r: *mut Renderer) {
    if !r.is_null() {
        os::close(*unsafe { Box::from_raw(r) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_interrupt(r: *mut Renderer) {
    if !r.is_null() {
        os::interrupt(unsafe { (&raw const (*r).sock).read() });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_pid(r: *mut Renderer) -> c_int {
    if r.is_null() {
        return -1;
    }
    os::pid(unsafe { &(*r).child })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_terminate(r: *mut Renderer) {
    if r.is_null() {
        return;
    }
    let sock = unsafe { (&raw const (*r).sock).read() };
    os::terminate(unsafe { &(*r).child }, sock);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_self_pid() -> c_int {
    os::self_pid()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_rproc_http_proc_info(
    pid: c_int,
    state: *mut c_char,
    state_sz: c_int,
    rss_kb: *mut c_long,
) -> c_int {
    let info = os::proc_info(pid);
    if !state.is_null() && state_sz > 0 {
        let word = info.state.unwrap_or("running").as_bytes();
        let n = word.len().min(state_sz as usize - 1);
        unsafe {
            ptr::copy_nonoverlapping(word.as_ptr(), state.cast::<u8>(), n);
            *state.add(n) = 0;
        }
    }
    if !rss_kb.is_null() {
        unsafe { *rss_kb = info.rss_kb.unwrap_or(-1) };
    }
    c_int::from(info.alive)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_http_proc_cpu(pid: c_int) -> f64 {
    os::proc_cpu(pid)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_http_proc_threads(pid: c_int) -> c_int {
    os::proc_threads(pid)
}
