//! Southstar — the C ABI of the pipeline's captures and dumps in src/engine.h, over cairo, paint, print and the GLib string they fill.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod images;
mod net;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GError, GHashTable, GPtrArray};
use southstar_layout::{BoxRef, NsBox};

pub use images::{ImageCache, Session, Wanted, collect_images};
pub use net::{
    Bytes, CssCache, Dest, GBytes, NsResponse, Request, fetch_blocking_with_headers,
    in_blocking_fetch, monotonic_us, preconnect, preload_clear, preload_request, record_timing,
    url_is_http_or_https, url_origin, url_resolve,
};

use crate::fetch::{self, CssFetch};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PrintSetup {
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

#[repr(C)]
pub struct GString {
    _str: *mut c_char,
    _len: usize,
    _allocated_len: usize,
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
struct CairoRectangle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

const CAIRO_STATUS_SUCCESS: c_int = 0;
const CAIRO_FORMAT_ARGB32: c_int = 0;
const CAIRO_CONTENT_COLOR_ALPHA: c_int = 0x3000;
const CAIRO_PDF_METADATA_CREATOR: c_int = 4;
const CAIRO_PDF_METADATA_CREATE_DATE: c_int = 5;

unsafe extern "C" {
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut CairoSurface;
    fn cairo_pdf_surface_create(path: *const c_char, width: f64, height: f64) -> *mut CairoSurface;
    fn cairo_pdf_surface_set_metadata(
        surface: *mut CairoSurface,
        metadata: c_int,
        utf8: *const c_char,
    );
    fn cairo_recording_surface_create(
        content: c_int,
        extents: *const CairoRectangle,
    ) -> *mut CairoSurface;
    fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
    fn cairo_surface_write_to_png(surface: *mut CairoSurface, path: *const c_char) -> c_int;
    fn cairo_status_to_string(status: c_int) -> *const c_char;
    fn cairo_create(surface: *mut CairoSurface) -> *mut Cairo;
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_show_page(cr: *mut Cairo);
    fn ns_paint(cr: *mut Cairo, root: *const NsBox, highlight: *const c_char);
    fn ns_print_page_offsets(root: *const NsBox, page_content_height: f64) -> *mut GArray;
    fn ns_print_page_bottom(offsets: *const GArray, i: c_uint, page_content_height: f64) -> f64;
    fn ns_print_draw_page(
        cr: *mut Cairo,
        root: *const NsBox,
        setup: *const PrintSetup,
        scale: f64,
        page_top: f64,
        page_bottom: f64,
    );
    fn ns_box_kind_name(kind: c_uint) -> *const c_char;
    fn ns_node_is_element_named(n: *const NsNode, tag: *const c_char) -> GBoolean;
    fn ns_engine_collect_stylesheets(
        doc: *mut NsNode,
        base_url: *const c_char,
        out: *mut GPtrArray,
        out_docs: *mut GPtrArray,
        css_cache: *mut GHashTable,
    );
    fn ns_anim_load_from_stylesheet(anim: *mut c_void, sheet: *const c_void);
    fn ns_anim_observe_all(anim: *mut c_void, styles: *mut GHashTable, now_us: i64);
    fn ns_css_stylesheet_free(sheet: *mut c_void);
    fn g_array_free(array: *mut GArray, free_segment: glib::GBoolean) -> *mut c_char;
    fn g_string_append_len(s: *mut GString, val: *const c_char, len: isize) -> *mut GString;
    fn g_string_append_printf(s: *mut GString, format: *const c_char, ...);
}

pub fn err(bytes: &[u8]) {
    glib::stderr_write(bytes);
}

pub fn is_named(n: Node, tag: &CStr) -> bool {
    unsafe { ns_node_is_element_named(n.as_ptr(), tag.as_ptr()) != 0 }
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub struct Capture {
    surface: NonNull<CairoSurface>,
    cr: *mut Cairo,
}

impl Capture {
    fn with_surface(
        surface: *mut CairoSurface,
        before_draw: impl FnOnce(*mut CairoSurface),
    ) -> Option<Capture> {
        let surface = NonNull::new(surface)?;
        if unsafe { cairo_surface_status(surface.as_ptr()) } != CAIRO_STATUS_SUCCESS {
            unsafe { cairo_surface_destroy(surface.as_ptr()) };
            return None;
        }
        before_draw(surface.as_ptr());
        let cr = unsafe { cairo_create(surface.as_ptr()) };
        Some(Capture { surface, cr })
    }

    pub fn image(width: i32, height: i32) -> Option<Capture> {
        Capture::with_surface(
            unsafe { cairo_image_surface_create(CAIRO_FORMAT_ARGB32, width, height) },
            |_| {},
        )
    }

    pub fn pdf(path: &CStr, width: f64, height: f64) -> Option<Capture> {
        Capture::with_surface(
            unsafe { cairo_pdf_surface_create(path.as_ptr(), width, height) },
            set_pdf_metadata,
        )
    }

    pub fn recording(width: f64, height: f64) -> Capture {
        let extent = CairoRectangle {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let surface = unsafe { cairo_recording_surface_create(CAIRO_CONTENT_COLOR_ALPHA, &extent) };
        let surface = NonNull::new(surface).expect("cairo_recording_surface_create");
        let cr = unsafe { cairo_create(surface.as_ptr()) };
        Capture { surface, cr }
    }

    pub fn paint(&self, root: BoxRef) {
        unsafe { ns_paint(self.cr, root.as_ptr(), ptr::null()) };
    }

    pub fn show_page(&self) {
        unsafe { cairo_show_page(self.cr) };
    }

    pub fn draw_sheet(&self, root: BoxRef, setup: &PrintSetup, scale: f64, top: f64, bottom: f64) {
        unsafe { ns_print_draw_page(self.cr, root.as_ptr(), setup, scale, top, bottom) };
    }

    pub fn write_png(self, path: &CStr) -> Result<(), Vec<u8>> {
        let surface = self.into_surface();
        let status = unsafe { cairo_surface_write_to_png(surface, path.as_ptr()) };
        unsafe { cairo_surface_destroy(surface) };
        if status == CAIRO_STATUS_SUCCESS {
            return Ok(());
        }
        let reason = unsafe { CStr::from_ptr(cairo_status_to_string(status)) };
        Err(reason.to_bytes().to_vec())
    }

    fn into_surface(self) -> *mut CairoSurface {
        let surface = self.surface.as_ptr();
        unsafe { cairo_destroy(self.cr) };
        core::mem::forget(self);
        surface
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            cairo_destroy(self.cr);
            cairo_surface_destroy(self.surface.as_ptr());
        }
    }
}

fn set_pdf_metadata(surface: *mut CairoSurface) {
    unsafe {
        cairo_pdf_surface_set_metadata(surface, CAIRO_PDF_METADATA_CREATOR, c"Southstar".as_ptr())
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if let Ok(date) = CString::new(crate::iso8601_utc(now)) {
        unsafe {
            cairo_pdf_surface_set_metadata(surface, CAIRO_PDF_METADATA_CREATE_DATE, date.as_ptr())
        };
    }
}

pub struct Offsets(NonNull<GArray>);

impl Offsets {
    pub fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.len as usize
    }

    pub fn top(&self, i: usize) -> f64 {
        unsafe { *self.0.as_ref().data.cast::<f64>().add(i) }
    }

    pub fn bottom(&self, i: usize, page_h: f64) -> f64 {
        unsafe { ns_print_page_bottom(self.0.as_ptr(), i as c_uint, page_h) }
    }
}

impl Drop for Offsets {
    fn drop(&mut self) {
        unsafe { g_array_free(self.0.as_ptr(), glib::TRUE) };
    }
}

pub fn page_offsets(root: BoxRef, page_h: f64) -> Offsets {
    Offsets(
        NonNull::new(unsafe { ns_print_page_offsets(root.as_ptr(), page_h) })
            .expect("ns_print_page_offsets"),
    )
}

pub struct Out(NonNull<GString>);

impl Out {
    pub fn append(&mut self, bytes: &[u8]) {
        unsafe {
            g_string_append_len(self.0.as_ptr(), bytes.as_ptr().cast(), bytes.len() as isize)
        };
    }

    pub fn box_line(&mut self, b: BoxRef) {
        let name = unsafe { ns_box_kind_name(b.kind_raw()) };
        unsafe {
            g_string_append_printf(
                self.0.as_ptr(),
                c"%s @(%.0f,%.0f) %.0fx%.0f".as_ptr(),
                name,
                b.x(),
                b.y(),
                b.content_width(),
                b.content_height(),
            )
        };
    }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

unsafe fn root<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_write_png(root_box: *const NsBox, path: *const c_char) -> c_int {
    match (unsafe { root(root_box) }, c_str(path)) {
        (Some(r), Some(path)) => crate::write_png(r, path),
        _ => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_write_pdf(root_box: *const NsBox, path: *const c_char) -> c_int {
    match (unsafe { root(root_box) }, c_str(path)) {
        (Some(r), Some(path)) => crate::write_pdf(r, path),
        _ => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_write_pdf_paged(
    root_box: *const NsBox,
    path: *const c_char,
    setup: *const PrintSetup,
) -> c_int {
    match (unsafe { root(root_box) }, c_str(path), unsafe {
        setup.as_ref()
    }) {
        (Some(r), Some(path), Some(setup)) => crate::write_pdf_paged(r, path, setup),
        _ => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_print_recordings(
    root_box: *const NsBox,
    setup: *const PrintSetup,
) -> *mut GPtrArray {
    let (Some(r), Some(setup)) = (unsafe { root(root_box) }, unsafe { setup.as_ref() }) else {
        return ptr::null_mut();
    };
    let sheets = crate::print_recordings(r, setup);
    let pages = unsafe { glib::g_ptr_array_new() };
    for sheet in sheets {
        unsafe { glib::g_ptr_array_add(pages, sheet.into_surface().cast()) };
    }
    pages
}

unsafe fn out(s: *mut GString) -> Option<Out> {
    NonNull::new(s).map(Out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_dump_text(b: *const NsBox, s: *mut GString) {
    if let (Some(b), Some(mut out)) = (unsafe { root(b) }, unsafe { out(s) }) {
        crate::dump_text(b, &mut out);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_dump_layout(b: *const NsBox, indent: c_int, s: *mut GString) {
    if let (Some(b), Some(mut out)) = (unsafe { root(b) }, unsafe { out(s) }) {
        crate::dump_layout(b, indent, &mut out);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_suffix_before_ext(
    path: *const c_char,
    suffix: *const c_char,
) -> *mut c_char {
    let Some(path) = c_str(path) else {
        return ptr::null_mut();
    };
    let suffix = c_str(suffix).map_or(&b""[..], CStr::to_bytes);
    glib::strdup(&crate::suffix_before_ext(path.to_bytes(), suffix))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_load_keyframes(
    anim: *mut c_void,
    doc: *mut NsNode,
    base_url: *const c_char,
    css_cache: *mut GHashTable,
) {
    if anim.is_null() {
        return;
    }
    let sheets = unsafe { glib::g_ptr_array_new() };
    unsafe { ns_engine_collect_stylesheets(doc, base_url, sheets, ptr::null_mut(), css_cache) };
    let a = unsafe { &*sheets };
    let list: Vec<*mut c_void> = (0..a.len as usize)
        .map(|i| unsafe { *a.pdata.add(i) })
        .collect();
    for &sheet in &list {
        if !sheet.is_null() {
            unsafe { ns_anim_load_from_stylesheet(anim, sheet) };
        }
    }
    for &sheet in &list {
        unsafe { ns_css_stylesheet_free(sheet) };
    }
    unsafe { glib::g_ptr_array_free(sheets, glib::TRUE) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_anim_observe(
    anim: *mut c_void,
    styles: *mut GHashTable,
    now_us: i64,
) {
    unsafe { ns_anim_observe_all(anim, styles, now_us) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_engine_in_blocking_fetch() -> GBoolean {
    glib::boolean(in_blocking_fetch())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_fetch_blocking(
    url: *const c_char,
    top_url: *const c_char,
    error: *mut *mut GError,
) -> *mut NsResponse {
    let r = Request {
        url,
        top_url,
        method: c"GET",
        body: ptr::null(),
        body_len: 0,
        content_type: ptr::null(),
        navigation: false,
        user_activated: false,
        headers: ptr::null(),
    };
    unsafe { net::request_blocking(&r, error) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_navigate_blocking(
    url: *const c_char,
    top_url: *const c_char,
    user_activated: GBoolean,
    error: *mut *mut GError,
) -> *mut NsResponse {
    let r = Request {
        url,
        top_url,
        method: c"GET",
        body: ptr::null(),
        body_len: 0,
        content_type: ptr::null(),
        navigation: true,
        user_activated: user_activated != 0,
        headers: ptr::null(),
    };
    unsafe { net::request_blocking(&r, error) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_navigate_post_blocking(
    url: *const c_char,
    top_url: *const c_char,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
    user_activated: GBoolean,
    error: *mut *mut GError,
) -> *mut NsResponse {
    let r = Request {
        url,
        top_url,
        method: c"POST",
        body,
        body_len,
        content_type,
        navigation: true,
        user_activated: user_activated != 0,
        headers: ptr::null(),
    };
    unsafe { net::request_blocking(&r, error) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_linked_css_text(url: *const c_char) -> *mut c_char {
    let Some(url) = c_str(url) else {
        return ptr::null_mut();
    };
    fetch::linked_css_text(url.to_bytes()).map_or(ptr::null_mut(), |css| glib::strdup(&css))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_remember_linked_css(url: *const c_char, bytes: *mut GBytes) {
    let (Some(url), Some(bytes)) = (c_str(url), unsafe { Bytes::borrow(bytes) }) else {
        return;
    };
    fetch::remember_linked_css(url.to_bytes(), bytes.data());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_take_resource_timings(top_url: *const c_char) -> *mut GPtrArray {
    net::take_resource_timings(c_str(top_url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_fetch_css_bytes(
    url: *const c_char,
    top_url: *const c_char,
    cache: *mut GHashTable,
    strict_mime: GBoolean,
    initiator: *const c_char,
    render_blocking: GBoolean,
    in_frame: GBoolean,
) -> *mut GBytes {
    let Some(url) = c_str(url) else {
        return ptr::null_mut();
    };
    let initiator = match c_str(initiator).map(CStr::to_bytes) {
        Some(b"link") => c"link",
        _ => c"css",
    };
    let f = CssFetch {
        url,
        top_url: c_str(top_url),
        strict_mime: strict_mime != 0,
        initiator,
        render_blocking: render_blocking != 0,
        in_frame: in_frame != 0,
    };
    fetch::fetch_css_bytes(&f, unsafe { CssCache::from_ptr(cache) })
        .map_or(ptr::null_mut(), Bytes::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_speculative_preload(
    doc: *mut NsNode,
    base_url: *const c_char,
    include_images: GBoolean,
) {
    let (Some(doc), Some(base)) = (unsafe { Node::from_ptr(doc) }, c_str(base_url)) else {
        return;
    };
    fetch::speculative_preload(doc, base, include_images != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_fetch_images(
    root_box: *mut NsBox,
    base_url: *const c_char,
    cache: *mut c_void,
) {
    let (Some(r), Some(base), Some(cache)) = (unsafe { root(root_box) }, c_str(base_url), unsafe {
        ImageCache::from_ptr(cache)
    }) else {
        return;
    };
    let wanted = fetch::collect_wanted_images(r, base, cache, 0.0, 0.0).wanted;
    if wanted.len() > 0 {
        images::fetch_images_blocking(&wanted, base, cache);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_fetch_images_start(
    root_box: *mut NsBox,
    base_url: *const c_char,
    cache: *mut c_void,
    requested: *mut GHashTable,
    scroll_y: f64,
    viewport_h: f64,
    deferred_any: *mut GBoolean,
    arrived_cb: images::ArrivedCb,
    user_data: *mut c_void,
) -> *mut Session {
    let (Some(r), Some(base), Some(cache)) = (unsafe { root(root_box) }, c_str(base_url), unsafe {
        ImageCache::from_ptr(cache)
    }) else {
        return images::null_session();
    };
    let found = fetch::collect_wanted_images(r, base, cache, scroll_y, viewport_h);
    if found.deferred_any && !deferred_any.is_null() {
        unsafe { *deferred_any = glib::TRUE };
    }
    found.wanted.exclude_and_record(requested);
    if found.wanted.len() == 0 {
        return images::null_session();
    }
    images::start_session(&found.wanted, base, cache, arrived_cb, user_data)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_img_session_outstanding(s: *const Session) -> c_int {
    unsafe { images::session_outstanding(s) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_engine_img_session_close(s: *mut Session) {
    unsafe { images::session_close(s) };
}
