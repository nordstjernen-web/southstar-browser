//! Southstar — the engine as the renderer host sees it: an owned page handle over the ns_browser_* API in src/libsouthstar.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{GBoolean, GPtrArray};

use crate::tiles::{self, LayerPlan, LayerTarget, Sticky, TileTarget, VpLayer};

#[repr(C)]
pub struct NsBrowser {
    _private: [u8; 0],
}

#[repr(C)]
struct GArray {
    data: *mut c_char,
    len: c_uint,
}

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

#[repr(C)]
struct ResponseHead {
    status: c_long,
    strings: [*mut c_char; 10],
    body: *mut GByteArray,
}

#[repr(C)]
struct Texture {
    _private: [u8; 0],
}

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

#[repr(C)]
struct Cairo {
    _private: [u8; 0],
}

#[repr(C)]
pub struct PaintLayerPlan {
    dynamic: GBoolean,
    kinds: *mut c_void,
    vp: *mut GArray,
}

#[repr(C)]
#[derive(Default)]
struct StickyY {
    has_top: GBoolean,
    has_bottom: GBoolean,
    top_start: f64,
    top_cap: f64,
    bottom_start: f64,
    bottom_cap: f64,
}

#[repr(C)]
#[derive(Default)]
struct VpLayerInfo {
    kind: c_int,
    top: f64,
    bottom: f64,
    x_offset: f64,
    sticky: StickyY,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PrintSetup {
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

const CAIRO_FORMAT_RGB24: c_int = 1;
const CAIRO_STATUS_SUCCESS: c_int = 0;

unsafe extern "C" {
    fn free(ptr: *mut c_void);
    fn g_get_monotonic_time() -> i64;
    fn g_ptr_array_free(array: *mut GPtrArray, free_segment: GBoolean) -> *mut *mut c_void;
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(string: *mut GString, free_segment: GBoolean) -> *mut c_char;

    fn ns_browser_init() -> c_int;
    fn ns_browser_open_viewport(
        url: *const c_char,
        width: c_int,
        height: f64,
        settle_ms: c_int,
    ) -> *mut NsBrowser;
    fn ns_browser_open_post_viewport(
        url: *const c_char,
        width: c_int,
        height: f64,
        settle_ms: c_int,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
    ) -> *mut NsBrowser;
    fn ns_browser_close(b: *mut NsBrowser);
    fn ns_browser_busy(b: *const NsBrowser) -> c_int;
    fn ns_browser_take_post(
        b: *mut NsBrowser,
        out_len: *mut usize,
        out_content_type: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_browser_render_text(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_dump_dom(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_dump_layout(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_dump_performance(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_render_image(b: *mut NsBrowser, path: *const c_char) -> c_int;
    fn ns_browser_page_size(b: *mut NsBrowser, w: *mut c_int, h: *mut c_int) -> c_int;
    fn ns_browser_snap_document(
        b: *mut NsBrowser,
        viewport_w: f64,
        viewport_h: f64,
        prev_x: c_int,
        prev_y: c_int,
        x: *mut c_int,
        y: *mut c_int,
    ) -> c_int;
    fn ns_browser_set_viewport(b: *mut NsBrowser, width: c_int, height: f64) -> c_int;
    fn ns_browser_set_device_pixel_ratio(b: *mut NsBrowser, dppx: f64) -> c_int;
    fn ns_browser_window_action_applied(b: *mut NsBrowser);
    fn ns_browser_render_argb32(
        b: *mut NsBrowser,
        sx: c_int,
        sy: c_int,
        w: c_int,
        h: c_int,
        scale: f64,
        out: *mut u8,
        stride: c_int,
    ) -> c_int;
    fn ns_browser_link_at(b: *mut NsBrowser, x: c_int, y: c_int) -> *mut c_char;
    fn ns_browser_link_under(b: *mut NsBrowser, x: c_int, y: c_int) -> *mut c_char;
    fn ns_browser_release_click(b: *mut NsBrowser, out_changed: *mut c_int) -> *mut c_char;
    fn ns_browser_cursor_at(b: *mut NsBrowser, x: c_int, y: c_int) -> *mut c_char;
    fn ns_browser_press(b: *mut NsBrowser, x: c_int, y: c_int, mods: c_int) -> *mut c_char;
    fn ns_browser_key_full(
        b: *mut NsBrowser,
        kind: c_int,
        key: *const c_char,
        code: *const c_char,
        keycode: c_int,
        mods: c_int,
        out_prevented: *mut c_int,
    ) -> *mut c_char;
    fn ns_browser_focused_editable(b: *mut NsBrowser) -> c_int;
    fn ns_browser_focused_editable_value(
        b: *mut NsBrowser,
        caret: *mut usize,
        anchor: *mut usize,
    ) -> *mut c_char;
    fn ns_browser_set_focused_editable_selection(
        b: *mut NsBrowser,
        caret: usize,
        anchor: usize,
    ) -> c_int;
    fn ns_browser_hover(b: *mut NsBrowser, x: c_int, y: c_int) -> c_int;
    fn ns_browser_scroll_at(b: *mut NsBrowser, x: c_int, y: c_int, dx: c_int, dy: c_int) -> c_int;
    fn ns_browser_scroll_at_full(
        b: *mut NsBrowser,
        x: c_int,
        y: c_int,
        dx: c_int,
        dy: c_int,
        out_snapped: *mut c_int,
    ) -> c_int;
    fn ns_browser_scrollbar_press(b: *mut NsBrowser, x: c_int, y: c_int) -> c_int;
    fn ns_browser_scrollbar_drag(b: *mut NsBrowser, x: c_int, y: c_int) -> c_int;
    fn ns_browser_scrollbar_release(b: *mut NsBrowser);
    fn ns_browser_drop_files(
        b: *mut NsBrowser,
        x: c_int,
        y: c_int,
        paths: *const *const c_char,
        n_paths: c_int,
    ) -> c_int;
    fn ns_browser_take_pending_audio(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_video_helper_event(
        b: *mut NsBrowser,
        token: *const c_char,
        kind: *const c_char,
    ) -> c_int;
    fn ns_browser_console_drain(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_eval(b: *mut NsBrowser, src: *const c_char) -> *mut c_char;
    fn ns_browser_media_at(
        b: *mut NsBrowser,
        x: c_int,
        y: c_int,
        out_is_video: *mut c_int,
        out_stream: *mut c_int,
    ) -> *mut c_char;
    fn ns_browser_contextmenu_full(
        b: *mut NsBrowser,
        x: c_int,
        y: c_int,
        out_edit: *mut c_int,
    ) -> c_int;
    fn ns_browser_find(
        b: *mut NsBrowser,
        query: *const c_char,
        case_sensitive: c_int,
        direction: c_int,
        from_y: c_int,
        out_total: *mut c_int,
        out_current: *mut c_int,
        out_y: *mut c_int,
    ) -> c_int;
    fn ns_browser_select(b: *mut NsBrowser, kind: c_int, x: c_int, y: c_int) -> *mut c_char;
    fn ns_browser_tick(b: *mut NsBrowser, budget_ms: c_int) -> c_int;
    fn ns_browser_animating(b: *mut NsBrowser) -> c_int;
    fn ns_browser_set_caret_blink_active(b: *mut NsBrowser, active: c_int) -> c_int;
    fn ns_browser_caret_blinking(b: *mut NsBrowser) -> c_int;
    fn ns_browser_title(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_url(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_set_next_referrer(url: *const c_char);
    fn ns_browser_set_next_user_activated(user_activated: c_int);
    fn ns_browser_set_color_scheme(dark: c_int);
    fn ns_browser_security(b: *mut NsBrowser, out_ip: *mut *const c_char) -> c_int;
    fn ns_browser_take_pending_nav(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_take_soft_nav_pushed(b: *mut NsBrowser) -> c_int;
    fn ns_browser_take_pending_scroll(b: *mut NsBrowser, x: *mut c_int, y: *mut c_int) -> c_int;
    fn ns_browser_take_pending_webgl(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_resolve_webgl(b: *mut NsBrowser, origin: *const c_char, allow: c_int);
    fn ns_browser_take_pending_camera(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_resolve_camera(b: *mut NsBrowser, origin: *const c_char, allow: c_int);
    fn ns_browser_has_pending_clipboard(b: *mut NsBrowser) -> c_int;
    fn ns_browser_take_pending_clipboard(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_take_pending_download(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_take_pending_window_action(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_links(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_favicon_url(b: *mut NsBrowser) -> *mut c_char;
    fn ns_browser_bfcache_eligible(b: *mut NsBrowser) -> c_int;
    fn ns_browser_bfcache_park(b: *mut NsBrowser);
    fn ns_browser_bfcache_restore(b: *mut NsBrowser, width: c_int, height: f64);
    fn ns_browser_print_pages(b: *mut NsBrowser, out_setup: *mut PrintSetup) -> *mut GPtrArray;
    fn ns_print_setup_default(setup: *mut PrintSetup);

    fn ns_browser_note_viewport(
        b: *mut NsBrowser,
        scroll_x: c_int,
        scroll_y: c_int,
        height: c_int,
        scale: f64,
    );
    fn ns_browser_flush_video_rects(b: *mut NsBrowser);
    fn ns_browser_layers_prepare(
        b: *mut NsBrowser,
        scroll_x: c_int,
        scroll_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        plan: *mut PaintLayerPlan,
    ) -> c_int;
    fn ns_browser_render_doc_tile(
        b: *mut NsBrowser,
        plan: *const PaintLayerPlan,
        scroll_x: c_int,
        tile_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        bufs: *const *mut u8,
        stride: c_int,
        upper_used: *mut GBoolean,
    ) -> c_int;
    fn ns_browser_vp_layer_info(
        b: *mut NsBrowser,
        plan: *const PaintLayerPlan,
        index: c_int,
        out: *mut VpLayerInfo,
    ) -> c_int;
    fn ns_browser_render_vp_layer(
        b: *mut NsBrowser,
        plan: *const PaintLayerPlan,
        index: c_int,
        scroll_x: c_int,
        scroll_y: c_int,
        origin_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        out: *mut u8,
        stride: c_int,
    ) -> c_int;
    fn ns_browser_canvas_color(b: *mut NsBrowser, rgba_out: *mut f64) -> GBoolean;
    fn ns_browser_scroller_rects(b: *mut NsBrowser, out: *mut GString, max_rects: c_int);
    fn ns_paint_layer_plan_init(plan: *mut PaintLayerPlan);
    fn ns_paint_layer_plan_clear(plan: *mut PaintLayerPlan);

    fn ns_net_fetch_blocking(
        url: *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> *mut ResponseHead;
    fn ns_response_free(resp: *mut ResponseHead);
    fn ns_net_log_clear();
    fn ns_net_log_dump() -> *mut c_char;
    fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> GBoolean;
    fn ns_image_decode_bytes(
        data: *const u8,
        len: usize,
        w: *mut c_int,
        h: *mut c_int,
    ) -> *mut Texture;
    fn ns_texture_download(texture: *mut Texture, dst: *mut u8, dst_stride: usize);
    fn ns_texture_unref(texture: *mut Texture);

    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut CairoSurface;
    fn cairo_create(target: *mut CairoSurface) -> *mut Cairo;
    fn cairo_set_source_rgb(cr: *mut Cairo, r: f64, g: f64, b: f64);
    fn cairo_paint(cr: *mut Cairo);
    fn cairo_scale(cr: *mut Cairo, sx: f64, sy: f64);
    fn cairo_set_source_surface(cr: *mut Cairo, surface: *mut CairoSurface, x: f64, y: f64);
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_surface_write_to_png(surface: *mut CairoSurface, filename: *const c_char) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

pub fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take(text: *mut c_char) -> Option<Vec<u8>> {
    if text.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    unsafe { free(text.cast()) };
    Some(bytes)
}

fn opt_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

pub fn init() -> c_int {
    unsafe { ns_browser_init() }
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn set_color_scheme(dark: c_int) {
    unsafe { ns_browser_set_color_scheme(dark) };
}

pub fn set_next_referrer(url: &CStr) {
    unsafe { ns_browser_set_next_referrer(url.as_ptr()) };
}

pub fn set_next_user_activated(activated: bool) {
    unsafe { ns_browser_set_next_user_activated(c_int::from(activated)) };
}

pub fn net_log_clear() {
    unsafe { ns_net_log_clear() };
}

pub fn net_log_dump() -> Option<Vec<u8>> {
    take(unsafe { ns_net_log_dump() })
}

pub fn same_origin(a: &CStr, b: &CStr) -> bool {
    unsafe { ns_url_same_origin(a.as_ptr(), b.as_ptr()) != 0 }
}

pub fn set_device_pixel_ratio(page: Option<&mut Browser>, dppx: f64) -> c_int {
    let raw = page.map_or(ptr::null_mut(), |page| page.raw());
    unsafe { ns_browser_set_device_pixel_ratio(raw, dppx) }
}

pub fn print_setup_default() -> PrintSetup {
    let mut setup = PrintSetup::default();
    unsafe { ns_print_setup_default(&mut setup) };
    setup
}

pub struct Favicon {
    pub width: c_int,
    pub height: c_int,
    pub pixels: Vec<u8>,
}

pub fn fetch_favicon(url: &CStr) -> Option<Favicon> {
    let resp = unsafe { ns_net_fetch_blocking(url.as_ptr(), ptr::null_mut(), ptr::null_mut()) };
    let head = unsafe { resp.as_ref() }?;
    let body = unsafe { head.body.as_ref() }.filter(|body| head.status < 400 && body.len > 0);
    let Some(body) = body else {
        unsafe { ns_response_free(resp) };
        return None;
    };
    let (mut w, mut h) = (0, 0);
    let tex = unsafe { ns_image_decode_bytes(body.data, body.len as usize, &mut w, &mut h) };
    unsafe { ns_response_free(resp) };
    if tex.is_null() {
        return None;
    }
    if !(1..=512).contains(&w) || !(1..=512).contains(&h) {
        unsafe { ns_texture_unref(tex) };
        return None;
    }
    let stride = w as usize * 4;
    let mut pixels = vec![0u8; stride * h as usize];
    unsafe {
        ns_texture_download(tex, pixels.as_mut_ptr(), stride);
        ns_texture_unref(tex);
    }
    Some(Favicon {
        width: w,
        height: h,
        pixels,
    })
}

pub struct PrintPages(NonNull<GPtrArray>);

impl PrintPages {
    pub fn into_raw(self) -> *mut GPtrArray {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }

    fn sheets(&self) -> Vec<*mut CairoSurface> {
        let array = unsafe { self.0.as_ref() };
        (0..array.len as usize)
            .map(|i| unsafe { *array.pdata.add(i) }.cast())
            .collect()
    }

    pub fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.len as usize
    }

    pub fn write_png(
        &self,
        index: usize,
        width: c_int,
        height: c_int,
        scale: f64,
        path: &CStr,
    ) -> bool {
        let Some(&sheet) = self.sheets().get(index) else {
            return false;
        };
        unsafe {
            let img = cairo_image_surface_create(CAIRO_FORMAT_RGB24, width, height);
            let cr = cairo_create(img);
            cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
            cairo_paint(cr);
            cairo_scale(cr, scale, scale);
            cairo_set_source_surface(cr, sheet, 0.0, 0.0);
            cairo_paint(cr);
            cairo_destroy(cr);
            let ok = cairo_surface_write_to_png(img, path.as_ptr()) == CAIRO_STATUS_SUCCESS;
            cairo_surface_destroy(img);
            ok
        }
    }
}

impl Drop for PrintPages {
    fn drop(&mut self) {
        for sheet in self.sheets() {
            unsafe { cairo_surface_destroy(sheet) };
        }
        unsafe { g_ptr_array_free(self.0.as_ptr(), 1) };
    }
}

pub struct Post {
    pub body: Vec<u8>,
    pub content_type: Option<CString>,
}

pub struct Browser(NonNull<NsBrowser>);

impl Drop for Browser {
    fn drop(&mut self) {
        unsafe { ns_browser_close(self.0.as_ptr()) };
    }
}

macro_rules! taker {
    ($($name:ident => $c:ident),* $(,)?) => {
        $(pub fn $name(&mut self) -> Option<Vec<u8>> {
            take(unsafe { $c(self.raw()) })
        })*
    };
}

impl Browser {
    fn raw(&mut self) -> *mut NsBrowser {
        self.0.as_ptr()
    }

    pub fn open_viewport(
        url: &CStr,
        width: c_int,
        height: f64,
        settle_ms: c_int,
    ) -> Option<Browser> {
        NonNull::new(unsafe { ns_browser_open_viewport(url.as_ptr(), width, height, settle_ms) })
            .map(Browser)
    }

    pub fn open_post_viewport(
        url: &CStr,
        width: c_int,
        height: f64,
        settle_ms: c_int,
        post: &Post,
    ) -> Option<Browser> {
        NonNull::new(unsafe {
            ns_browser_open_post_viewport(
                url.as_ptr(),
                width,
                height,
                settle_ms,
                post.body.as_ptr().cast(),
                post.body.len(),
                opt_ptr(post.content_type.as_deref()),
            )
        })
        .map(Browser)
    }

    taker! {
        url => ns_browser_url,
        title => ns_browser_title,
        render_text => ns_browser_render_text,
        dump_dom => ns_browser_dump_dom,
        dump_layout => ns_browser_dump_layout,
        dump_performance => ns_browser_dump_performance,
        links => ns_browser_links,
        favicon_url => ns_browser_favicon_url,
        console_drain => ns_browser_console_drain,
        take_pending_nav => ns_browser_take_pending_nav,
        take_pending_webgl => ns_browser_take_pending_webgl,
        take_pending_camera => ns_browser_take_pending_camera,
        take_pending_download => ns_browser_take_pending_download,
        take_pending_audio => ns_browser_take_pending_audio,
        take_pending_window_action => ns_browser_take_pending_window_action,
        take_pending_clipboard => ns_browser_take_pending_clipboard,
    }

    pub fn busy(&mut self) -> bool {
        unsafe { ns_browser_busy(self.raw()) != 0 }
    }

    pub fn take_post(&mut self) -> Option<Post> {
        let mut len = 0;
        let mut content_type: *mut c_char = ptr::null_mut();
        let body = unsafe { ns_browser_take_post(self.raw(), &mut len, &mut content_type) };
        let content_type = take(content_type).map(|ct| cstring(&ct));
        if body.is_null() {
            return None;
        }
        let bytes = unsafe { core::slice::from_raw_parts(body.cast::<u8>(), len) }.to_vec();
        unsafe { free(body.cast()) };
        Some(Post {
            body: bytes,
            content_type,
        })
    }

    pub fn render_image(&mut self, path: &CStr) -> c_int {
        unsafe { ns_browser_render_image(self.raw(), path.as_ptr()) }
    }

    pub fn page_size(&mut self, w: &mut c_int, h: &mut c_int) {
        unsafe { ns_browser_page_size(self.raw(), w, h) };
    }

    pub fn snap_document(
        &mut self,
        vw: f64,
        vh: f64,
        prev: (c_int, c_int),
        x: &mut c_int,
        y: &mut c_int,
    ) -> bool {
        unsafe { ns_browser_snap_document(self.raw(), vw, vh, prev.0, prev.1, x, y) != 0 }
    }

    pub fn set_viewport(&mut self, width: c_int, height: f64) -> c_int {
        unsafe { ns_browser_set_viewport(self.raw(), width, height) }
    }

    pub fn window_action_applied(&mut self) {
        unsafe { ns_browser_window_action_applied(self.raw()) };
    }

    pub fn render_argb32(
        &mut self,
        sx: c_int,
        sy: c_int,
        w: c_int,
        h: c_int,
        scale: f64,
        out: &mut [u8],
    ) -> c_int {
        unsafe {
            ns_browser_render_argb32(
                self.raw(),
                sx,
                sy,
                w,
                h,
                scale,
                out.as_mut_ptr(),
                w.wrapping_mul(4),
            )
        }
    }

    pub fn link_at(&mut self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_link_at(self.raw(), x, y) })
    }

    pub fn link_under(&mut self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_link_under(self.raw(), x, y) })
    }

    pub fn cursor_at(&mut self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_cursor_at(self.raw(), x, y) })
    }

    pub fn release_click(&mut self, changed: &mut c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_release_click(self.raw(), changed) })
    }

    pub fn press(&mut self, x: c_int, y: c_int, mods: c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_press(self.raw(), x, y, mods) })
    }

    pub fn select(&mut self, kind: c_int, x: c_int, y: c_int) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_select(self.raw(), kind, x, y) })
    }

    pub fn key_full(
        &mut self,
        kind: c_int,
        key: &CStr,
        code: &CStr,
        keycode: c_int,
        mods: c_int,
        prevented: &mut c_int,
    ) -> Option<Vec<u8>> {
        take(unsafe {
            ns_browser_key_full(
                self.raw(),
                kind,
                key.as_ptr(),
                code.as_ptr(),
                keycode,
                mods,
                prevented,
            )
        })
    }

    pub fn focused_editable(&mut self) -> c_int {
        unsafe { ns_browser_focused_editable(self.raw()) }
    }

    pub fn focused_editable_value(
        &mut self,
        caret: &mut usize,
        anchor: &mut usize,
    ) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_focused_editable_value(self.raw(), caret, anchor) })
    }

    pub fn set_focused_editable_selection(&mut self, caret: usize, anchor: usize) -> c_int {
        unsafe { ns_browser_set_focused_editable_selection(self.raw(), caret, anchor) }
    }

    pub fn hover(&mut self, x: c_int, y: c_int) -> c_int {
        unsafe { ns_browser_hover(self.raw(), x, y) }
    }

    pub fn scroll_at(&mut self, x: c_int, y: c_int, dx: c_int, dy: c_int) -> c_int {
        unsafe { ns_browser_scroll_at(self.raw(), x, y, dx, dy) }
    }

    pub fn scroll_at_full(
        &mut self,
        x: c_int,
        y: c_int,
        dx: c_int,
        dy: c_int,
        snapped: &mut c_int,
    ) -> bool {
        unsafe { ns_browser_scroll_at_full(self.raw(), x, y, dx, dy, snapped) != 0 }
    }

    pub fn scrollbar_press(&mut self, x: c_int, y: c_int) -> c_int {
        unsafe { ns_browser_scrollbar_press(self.raw(), x, y) }
    }

    pub fn scrollbar_drag(&mut self, x: c_int, y: c_int) -> c_int {
        unsafe { ns_browser_scrollbar_drag(self.raw(), x, y) }
    }

    pub fn scrollbar_release(&mut self) {
        unsafe { ns_browser_scrollbar_release(self.raw()) };
    }

    pub fn drop_files(&mut self, x: c_int, y: c_int, paths: &[CString]) -> c_int {
        let pointers: Vec<*const c_char> = paths.iter().map(|path| path.as_ptr()).collect();
        unsafe {
            ns_browser_drop_files(self.raw(), x, y, pointers.as_ptr(), pointers.len() as c_int)
        }
    }

    pub fn video_helper_event(&mut self, token: Option<&CStr>, kind: Option<&CStr>) -> c_int {
        unsafe { ns_browser_video_helper_event(self.raw(), opt_ptr(token), opt_ptr(kind)) }
    }

    pub fn eval(&mut self, src: &CStr) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_eval(self.raw(), src.as_ptr()) })
    }

    pub fn media_at(
        &mut self,
        x: c_int,
        y: c_int,
        is_video: &mut c_int,
        stream: &mut c_int,
    ) -> Option<Vec<u8>> {
        take(unsafe { ns_browser_media_at(self.raw(), x, y, is_video, stream) })
    }

    pub fn contextmenu_full(&mut self, x: c_int, y: c_int, edit: &mut c_int) -> c_int {
        unsafe { ns_browser_contextmenu_full(self.raw(), x, y, edit) }
    }

    pub fn find(
        &mut self,
        query: &CStr,
        options: (c_int, c_int, c_int),
        out: (&mut c_int, &mut c_int, &mut c_int),
    ) {
        unsafe {
            ns_browser_find(
                self.raw(),
                query.as_ptr(),
                options.0,
                options.1,
                options.2,
                out.0,
                out.1,
                out.2,
            )
        };
    }

    pub fn tick(&mut self, budget_ms: c_int) -> c_int {
        unsafe { ns_browser_tick(self.raw(), budget_ms) }
    }

    pub fn animating(&mut self) -> bool {
        unsafe { ns_browser_animating(self.raw()) != 0 }
    }

    pub fn set_caret_blink_active(&mut self, active: bool) -> c_int {
        unsafe { ns_browser_set_caret_blink_active(self.raw(), c_int::from(active)) }
    }

    pub fn caret_blinking(&mut self) -> bool {
        unsafe { ns_browser_caret_blinking(self.raw()) != 0 }
    }

    pub fn security(&mut self) -> (c_int, Option<Vec<u8>>) {
        let mut ip: *const c_char = ptr::null();
        let security = unsafe { ns_browser_security(self.raw(), &mut ip) };
        let ip = (!ip.is_null()).then(|| unsafe { CStr::from_ptr(ip) }.to_bytes().to_vec());
        (security, ip)
    }

    pub fn take_soft_nav_pushed(&mut self) -> c_int {
        unsafe { ns_browser_take_soft_nav_pushed(self.raw()) }
    }

    pub fn take_pending_scroll(&mut self, x: &mut c_int, y: &mut c_int) {
        unsafe { ns_browser_take_pending_scroll(self.raw(), x, y) };
    }

    pub fn resolve_webgl(&mut self, origin: &CStr, allow: c_int) {
        unsafe { ns_browser_resolve_webgl(self.raw(), origin.as_ptr(), allow) };
    }

    pub fn resolve_camera(&mut self, origin: &CStr, allow: c_int) {
        unsafe { ns_browser_resolve_camera(self.raw(), origin.as_ptr(), allow) };
    }

    pub fn has_pending_clipboard(&mut self) -> bool {
        unsafe { ns_browser_has_pending_clipboard(self.raw()) != 0 }
    }

    pub fn bfcache_eligible(&mut self) -> bool {
        unsafe { ns_browser_bfcache_eligible(self.raw()) != 0 }
    }

    pub fn bfcache_park(&mut self) {
        unsafe { ns_browser_bfcache_park(self.raw()) };
    }

    pub fn bfcache_restore(&mut self, width: c_int, height: f64) {
        unsafe { ns_browser_bfcache_restore(self.raw(), width, height) };
    }

    pub fn print_pages(&mut self, setup: &mut PrintSetup) -> Option<PrintPages> {
        NonNull::new(unsafe { ns_browser_print_pages(self.raw(), setup) }).map(PrintPages)
    }
}

pub struct Plan(Box<PaintLayerPlan>);

impl Plan {
    pub fn new() -> Plan {
        let mut plan = Box::new(PaintLayerPlan {
            dynamic: 0,
            kinds: ptr::null_mut(),
            vp: ptr::null_mut(),
        });
        unsafe { ns_paint_layer_plan_init(&mut *plan) };
        Plan(plan)
    }
}

impl Drop for Plan {
    fn drop(&mut self) {
        unsafe { ns_paint_layer_plan_clear(&mut *self.0) };
    }
}

impl LayerPlan for Plan {
    fn vp_count(&self) -> usize {
        unsafe { self.0.vp.as_ref() }.map_or(0, |vp| vp.len as usize)
    }
}

impl tiles::Page for Browser {
    type Plan = Plan;

    fn note_viewport(&mut self, sx: c_int, sy: c_int, height: c_int, scale: f64) {
        unsafe { ns_browser_note_viewport(self.raw(), sx, sy, height, scale) };
    }

    fn prepare_layers(
        &mut self,
        plan: &mut Plan,
        sx: c_int,
        sy: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
    ) -> c_int {
        unsafe { ns_browser_layers_prepare(self.raw(), sx, sy, width, height, scale, &mut *plan.0) }
    }

    fn vp_layer(&mut self, plan: &Plan, index: usize) -> Option<VpLayer> {
        let mut info = VpLayerInfo::default();
        if unsafe { ns_browser_vp_layer_info(self.raw(), &*plan.0, index as c_int, &mut info) } != 0
        {
            return None;
        }
        Some(VpLayer {
            kind: info.kind,
            top: info.top,
            bottom: info.bottom,
            x_offset: info.x_offset,
            sticky: Sticky {
                has_top: info.sticky.has_top != 0,
                has_bottom: info.sticky.has_bottom != 0,
                top_start: info.sticky.top_start,
                top_cap: info.sticky.top_cap,
                bottom_start: info.sticky.bottom_start,
                bottom_cap: info.sticky.bottom_cap,
            },
        })
    }

    fn render_doc_tile(&mut self, plan: &Plan, target: TileTarget<'_, '_>) -> c_int {
        let bufs: Vec<*mut u8> = target.bufs.iter_mut().map(|buf| buf.as_mut_ptr()).collect();
        unsafe {
            ns_browser_render_doc_tile(
                self.raw(),
                &*plan.0,
                target.sx,
                target.tile_y,
                target.width,
                target.height,
                target.scale,
                bufs.as_ptr(),
                target.stride,
                target.upper_used.as_mut_ptr(),
            )
        }
    }

    fn render_vp_layer(&mut self, plan: &Plan, index: usize, target: LayerTarget<'_>) -> c_int {
        unsafe {
            ns_browser_render_vp_layer(
                self.raw(),
                &*plan.0,
                index as c_int,
                target.sx,
                target.sy,
                target.origin_y,
                target.width,
                target.height,
                target.scale,
                target.out.as_mut_ptr(),
                target.stride,
            )
        }
    }

    fn canvas_color(&mut self, rgba: &mut [f64; 4]) {
        unsafe { ns_browser_canvas_color(self.raw(), rgba.as_mut_ptr()) };
    }

    fn scroller_rects(&mut self, out: &mut Vec<u8>, max_rects: c_int) {
        unsafe {
            let rects = g_string_new(ptr::null());
            ns_browser_scroller_rects(self.raw(), rects, max_rects);
            if !(*rects).str.is_null() {
                out.extend_from_slice(core::slice::from_raw_parts(
                    (*rects).str.cast(),
                    (*rects).len,
                ));
            }
            g_string_free(rects, 1);
        }
    }

    fn flush_video_rects(&mut self) {
        unsafe { ns_browser_flush_video_rects(self.raw()) };
    }
}
