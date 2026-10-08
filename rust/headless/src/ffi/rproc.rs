//! Southstar — the renderer connection of src/rproc_http.h as the headless driver uses it, with the page and frame replies it frees.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};

#[repr(C)]
struct RprocHttp {
    _private: [u8; 0],
}

#[repr(C)]
struct RawPage {
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
struct RawFrame {
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
struct CairoSurface {
    _private: [u8; 0],
}

type CairoWrite = unsafe extern "C" fn(closure: *mut c_void, data: *const u8, length: u32) -> c_int;

const CAIRO_FORMAT_ARGB32: c_int = 0;
const CAIRO_STATUS_SUCCESS: c_int = 0;

unsafe extern "C" {
    fn ns_rproc_single_process_enable();
    fn ns_rproc_http_spawn_shm(path: *const c_char, max_w: c_int, max_h: c_int) -> *mut RprocHttp;
    fn ns_rproc_http_open(
        r: *mut RprocHttp,
        url: *const c_char,
        vw: c_int,
        vh: c_int,
        settle_ms: c_int,
        out: *mut RawPage,
    ) -> c_int;
    fn ns_rproc_http_page_clear(out: *mut RawPage);
    fn ns_rproc_http_render(
        r: *mut RprocHttp,
        width: c_int,
        height: c_int,
        scroll_x: c_int,
        scroll_y: c_int,
        scale: f64,
        caret_active: c_int,
        out: *mut RawFrame,
    ) -> c_int;
    fn ns_rproc_http_click(r: *mut RprocHttp, x: c_int, y: c_int, mods: c_int) -> *mut c_char;
    fn ns_rproc_http_release_full(r: *mut RprocHttp, out_changed: *mut c_int) -> *mut c_char;
    fn ns_rproc_http_select(r: *mut RprocHttp, kind: c_int, x: c_int, y: c_int) -> *mut c_char;
    fn ns_rproc_http_contextmenu(
        r: *mut RprocHttp,
        x: c_int,
        y: c_int,
        out_prevented: *mut c_int,
        out_edit: *mut c_int,
    );
    fn ns_rproc_http_key(
        r: *mut RprocHttp,
        kind: c_int,
        key: *const c_char,
        code: *const c_char,
        keycode: c_int,
        mods: c_int,
    ) -> *mut c_char;
    fn ns_rproc_http_eval(r: *mut RprocHttp, src: *const c_char) -> *mut c_char;
    fn ns_rproc_http_set_viewport(
        r: *mut RprocHttp,
        width: c_int,
        height: c_int,
        out: *mut RawPage,
    ) -> c_int;
    fn ns_rproc_http_dump(r: *mut RprocHttp, kind: *const c_char) -> *mut c_char;
    fn ns_rproc_http_console_poll(r: *mut RprocHttp) -> *mut c_char;
    fn ns_rproc_http_close(r: *mut RprocHttp);
    fn free(p: *mut c_void);
    fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: c_int,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut CairoSurface;
    fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    fn cairo_surface_write_to_png_stream(
        surface: *mut CairoSurface,
        write: CairoWrite,
        closure: *mut c_void,
    ) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

pub fn single_process_enable() {
    unsafe { ns_rproc_single_process_enable() };
}

pub struct CBuf(NonNull<c_char>);

impl CBuf {
    fn take(p: *mut c_char) -> Option<CBuf> {
        NonNull::new(p).map(CBuf)
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { CStr::from_ptr(self.0.as_ptr()) }.to_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

impl Drop for CBuf {
    fn drop(&mut self) {
        unsafe { free(self.0.as_ptr().cast()) };
    }
}

fn copy(p: *const c_char) -> Option<Vec<u8>> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes().to_vec())
}

pub struct Page(RawPage);

impl Page {
    pub fn nav(&self) -> Option<Vec<u8>> {
        copy(self.0.nav)
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe { ns_rproc_http_page_clear(&mut self.0) };
    }
}

fn zeroed_page() -> RawPage {
    unsafe { MaybeUninit::zeroed().assume_init() }
}

pub struct Frame(RawFrame);

impl Frame {
    pub fn nav(&self) -> Option<Vec<u8>> {
        copy(self.0.nav)
    }

    pub fn size(&self) -> Option<(c_int, c_int)> {
        (self.0.ok != 0 && !self.0.pixels.is_null()).then_some((self.0.width, self.0.height))
    }

    pub fn encode_png(&self) -> Option<Vec<u8>> {
        let f = &self.0;
        let surface = unsafe {
            cairo_image_surface_create_for_data(
                f.pixels.cast_mut(),
                CAIRO_FORMAT_ARGB32,
                f.width,
                f.height,
                f.stride,
            )
        };
        let mut png = Vec::new();
        let ok = unsafe { cairo_surface_status(surface) } == CAIRO_STATUS_SUCCESS;
        if ok {
            unsafe {
                cairo_surface_write_to_png_stream(
                    surface,
                    png_sink,
                    (&mut png as *mut Vec<u8>).cast(),
                )
            };
        }
        unsafe { cairo_surface_destroy(surface) };
        ok.then_some(png)
    }
}

unsafe extern "C" fn png_sink(closure: *mut c_void, data: *const u8, length: u32) -> c_int {
    let png = unsafe { &mut *closure.cast::<Vec<u8>>() };
    png.extend_from_slice(unsafe { core::slice::from_raw_parts(data, length as usize) });
    CAIRO_STATUS_SUCCESS
}

impl Drop for Frame {
    fn drop(&mut self) {
        let f = &mut self.0;
        for p in [
            f.nav,
            f.webgl,
            f.camera,
            f.download,
            f.audio,
            f.window_action,
        ] {
            unsafe { free(p.cast()) };
        }
    }
}

fn zeroed_frame() -> RawFrame {
    unsafe { MaybeUninit::zeroed().assume_init() }
}

pub struct Renderer(NonNull<RprocHttp>);

impl Renderer {
    pub fn spawn_shm(max_w: c_int, max_h: c_int) -> Option<Renderer> {
        NonNull::new(unsafe { ns_rproc_http_spawn_shm(ptr::null(), max_w, max_h) }).map(Renderer)
    }

    fn raw(&self) -> *mut RprocHttp {
        self.0.as_ptr()
    }

    pub fn open(&self, url: &CStr, vw: c_int, vh: c_int, settle_ms: c_int) -> Option<Page> {
        let mut page = Page(zeroed_page());
        let rc =
            unsafe { ns_rproc_http_open(self.raw(), url.as_ptr(), vw, vh, settle_ms, &mut page.0) };
        (rc == 0).then_some(page)
    }

    pub fn render(&self, width: c_int, height: c_int) -> Option<Frame> {
        let mut frame = Frame(zeroed_frame());
        let rc =
            unsafe { ns_rproc_http_render(self.raw(), width, height, 0, 0, 1.0, 0, &mut frame.0) };
        (rc == 0).then_some(frame)
    }

    pub fn click(&self, x: c_int, y: c_int) -> Option<CBuf> {
        CBuf::take(unsafe { ns_rproc_http_click(self.raw(), x, y, 0) })
    }

    pub fn release_full(&self) -> Option<CBuf> {
        let mut changed: c_int = 0;
        CBuf::take(unsafe { ns_rproc_http_release_full(self.raw(), &mut changed) })
    }

    pub fn select(&self, kind: c_int, x: c_int, y: c_int) -> Option<CBuf> {
        CBuf::take(unsafe { ns_rproc_http_select(self.raw(), kind, x, y) })
    }

    pub fn contextmenu(&self, x: c_int, y: c_int) -> (c_int, c_int) {
        let (mut prevented, mut edit): (c_int, c_int) = (0, 0);
        unsafe { ns_rproc_http_contextmenu(self.raw(), x, y, &mut prevented, &mut edit) };
        (prevented, edit)
    }

    pub fn key(&self, kind: c_int, key: &CStr, code: &CStr, keycode: c_int) -> Option<CBuf> {
        CBuf::take(unsafe {
            ns_rproc_http_key(self.raw(), kind, key.as_ptr(), code.as_ptr(), keycode, 0)
        })
    }

    pub fn eval(&self, src: &CStr) -> Option<CBuf> {
        CBuf::take(unsafe { ns_rproc_http_eval(self.raw(), src.as_ptr()) })
    }

    pub fn set_viewport(&self, width: c_int, height: c_int) -> Page {
        let mut page = Page(zeroed_page());
        unsafe { ns_rproc_http_set_viewport(self.raw(), width, height, &mut page.0) };
        page
    }

    pub fn dump(&self, kind: &CStr) -> Option<CBuf> {
        CBuf::take(unsafe { ns_rproc_http_dump(self.raw(), kind.as_ptr()) })
    }

    pub fn console_poll(&self) -> Option<CBuf> {
        CBuf::take(unsafe { ns_rproc_http_console_poll(self.raw()) })
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { ns_rproc_http_close(self.raw()) };
    }
}
