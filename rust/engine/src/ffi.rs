//! Southstar — the entry points of the engine and its remaining C ABI: captures, dumps, print sheets, fetches, style sheet collection, cascade and relayout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod css;
mod images;
mod net;

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::OnceLock;

use southstar_anim::AnimHandle;
use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_glib as glib;
use southstar_layout::{BoxRef, NsBox};

pub use css::{
    CssScope, SheetList, is_self_contained, layout_frame_viewport, layout_frame_viewport_raw,
    media_query_matches, media_viewport_pop, media_viewport_push, merged_styles_cached,
    parse_import_cached, parse_url_cached, shadow_adopted_css, style_element_cache_end,
    style_element_text, stylesheet_from_style_element_cached, url_resolve_opt,
};
pub use images::{ImageCache, ImageSession, Wanted, collect_images};
pub use net::{
    Bytes, CssCache, Dest, Post, fetch_blocking, fetch_blocking_with_headers, monotonic_us,
    navigate_blocking, nosniff, preconnect, preload_clear, preload_request, url_is_http_or_https,
    url_origin, url_resolve,
};

use crate::fetch;

pub use southstar_print::PrintSetup;

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
}

pub fn err(bytes: &[u8]) {
    glib::stderr_write(bytes);
}

pub fn is_named(n: Node, tag: &CStr) -> bool {
    n.element_name() == Some(tag.to_bytes())
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
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
        // SAFETY: the capture owns its live cairo context until into_surface or drop destroys it.
        if let Some(cr) = unsafe { southstar_cairo::ContextRef::from_raw(self.cr.cast()) } {
            southstar_paint::paint(cr, root, None, None);
        }
    }

    pub fn show_page(&self) {
        unsafe { cairo_show_page(self.cr) };
    }

    pub fn draw_sheet(&self, root: BoxRef, setup: &PrintSetup, scale: f64, top: f64, bottom: f64) {
        // SAFETY: the capture owns its live cairo context until into_surface or drop destroys it.
        if let Some(cr) = unsafe { southstar_cairo::ContextRef::from_raw(self.cr.cast()) } {
            southstar_print::draw_page(cr, root, setup, scale, top, bottom);
        }
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
        self.into_surface_ref().as_ptr()
    }

    fn into_surface_ref(self) -> NonNull<CairoSurface> {
        let surface = self.surface;
        // SAFETY: the capture owns its context and destroys it once, here, keeping the surface's reference.
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

pub struct Offsets(Vec<f64>);

impl Offsets {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn top(&self, i: usize) -> f64 {
        self.0[i]
    }

    pub fn bottom(&self, i: usize, page_h: f64) -> f64 {
        southstar_print::page_bottom(&self.0, i, page_h)
    }
}

pub fn page_offsets(root: BoxRef, page_h: f64) -> Offsets {
    Offsets(southstar_print::page_offsets(Some(root), page_h))
}

pub struct Out(Vec<u8>);

fn fixed0(v: f64) -> String {
    if v.is_nan() {
        String::from("nan")
    } else {
        format!("{v:.0}")
    }
}

impl Out {
    fn collect(fill: impl FnOnce(&mut Out)) -> Vec<u8> {
        let mut out = Out(Vec::new());
        fill(&mut out);
        out.0
    }

    pub fn append(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    pub fn box_line(&mut self, b: BoxRef) {
        let name = southstar_layout_query::kind_name(b.kind_raw());
        self.0.extend_from_slice(name.to_bytes());
        let line = format!(
            " @({},{}) {}x{}",
            fixed0(b.x()),
            fixed0(b.y()),
            fixed0(b.content_width()),
            fixed0(b.content_height())
        );
        self.0.extend_from_slice(line.as_bytes());
    }
}

pub struct Page<'a> {
    pub doc: Option<Node<'a>>,
    pub base: Option<&'a CStr>,
    pub css_cache: Option<CssCache>,
}

pub fn compute_cascade(page: &Page<'_>) -> Rc<StyleMap> {
    let _scope = CssScope::enter(page.base);
    let sheets = crate::styles::collect_stylesheets(page.doc, page.base, page.css_cache);
    let styles = css::css_compute(page.doc, &sheets);
    drop(sheets);
    Rc::new(styles)
}

pub fn load_keyframes(anim: AnimHandle, page: &Page<'_>) {
    let sheets = crate::styles::collect_stylesheets(page.doc, page.base, page.css_cache);
    css::load_into_anim(anim, &sheets);
    drop(sheets);
}

pub fn anim_observe(anim: AnimHandle, styles: Option<&StyleMap>, now_us: i64) {
    if let Some(styles) = styles {
        anim.observe_all(styles, now_us);
    }
}

pub struct Relayout<'a> {
    pub page: Page<'a>,
    pub viewport_width: c_int,
    pub viewport_height: f64,
    pub images: Option<southstar_image::ImageCache>,
    pub anim: Option<AnimHandle>,
    pub js: *mut c_void,
    pub focused: Option<Node<'a>>,
    pub hover: Option<Node<'a>>,
    pub caret_byte: usize,
    pub sel_anchor_byte: usize,
}

/// # Safety
///
/// `js` is null or the page's live script engine.
pub unsafe fn relayout(req: &Relayout<'_>, layout: &Cell<*mut NsBox>) -> Option<Rc<StyleMap>> {
    let viewport_width = req.viewport_width;
    let js = req.js;
    let out_layout = layout.as_ptr();
    let r = css::Relayout {
        doc: req.page.doc.map_or(ptr::null_mut(), Node::as_mut_ptr),
        base: req.page.base,
        viewport_width,
        viewport_height: req.viewport_height,
        images: req.images,
        anim: req.anim,
        js,
        focused: Node::ptr_or_null(req.focused),
        hover: Node::ptr_or_null(req.hover),
        caret_byte: req.caret_byte,
        sel_anchor_byte: req.sel_anchor_byte,
    };
    let cache = req.page.css_cache;
    let doc_node = req.page.doc;
    let scope = CssScope::enter(r.base);
    let mut sheets = crate::styles::collect_stylesheets(doc_node, r.base, cache);
    let ctx = css::render_ctx(&r, &sheets);
    let t0 = monotonic_us();
    let mut styles = if profile_enabled() {
        css::render_relayout_profiled(&ctx, out_layout, viewport_width)
    } else {
        css::render_relayout(&ctx, out_layout)
    };
    css::publish_to_page(js, styles.as_ref(), layout.get());
    if crate::styles::frame_viewports_disagree_with_layout() {
        css::discard_layout(js, styles, out_layout);
        drop(sheets);
        scope.cache_begin();
        sheets = crate::styles::collect_stylesheets(doc_node, r.base, cache);
        let ctx = css::render_ctx(&r, &sheets);
        styles = css::render_relayout(&ctx, out_layout);
        css::publish_to_page(js, styles.as_ref(), layout.get());
    }
    crate::styles::add_relayout(monotonic_us() - t0);
    css::log_relayout(styles.as_deref(), viewport_width);
    drop(sheets);
    styles
}

fn profile_enabled() -> bool {
    static PROFILE: OnceLock<bool> = OnceLock::new();
    *PROFILE.get_or_init(|| std::env::var_os("NS_PROFILE").is_some())
}

pub fn dump_text(root: BoxRef<'_>) -> Vec<u8> {
    Out::collect(|out| crate::write_text(root, out))
}

pub fn dump_layout(root: BoxRef<'_>) -> Vec<u8> {
    Out::collect(|out| crate::write_layout(root, 0, out))
}

pub struct Sheet(NonNull<CairoSurface>);

impl Sheet {
    pub fn as_ptr(&self) -> NonNull<c_void> {
        self.0.cast()
    }

    pub fn into_raw(self) -> NonNull<c_void> {
        let surface = self.0.cast();
        core::mem::forget(self);
        surface
    }
}

impl Drop for Sheet {
    fn drop(&mut self) {
        // SAFETY: the sheet owns one reference to its recording surface and releases it once.
        unsafe { cairo_surface_destroy(self.0.as_ptr()) };
    }
}

pub fn print_sheets(root: BoxRef<'_>, setup: &PrintSetup) -> Vec<Sheet> {
    crate::print_recordings(root, setup)
        .into_iter()
        .map(|capture| Sheet(capture.into_surface_ref()))
        .collect()
}

pub fn fetch_images(root: BoxRef<'_>, base: &CStr, cache: southstar_image::ImageCache) {
    let cache = ImageCache::of(cache);
    let wanted = fetch::collect_wanted_images(root, base, cache, 0.0, 0.0).wanted;
    if wanted.len() > 0 {
        images::fetch_images_blocking(&wanted, base, cache);
    }
}

pub struct ImageFetch<'a> {
    pub root: BoxRef<'a>,
    pub base: &'a CStr,
    pub cache: southstar_image::ImageCache,
    pub scroll_y: f64,
    pub viewport_h: f64,
}

pub fn start_image_session(
    fetch: &ImageFetch<'_>,
    requested: &mut HashSet<CString>,
    arrived: Rc<dyn Fn()>,
) -> (Option<ImageSession>, bool) {
    let cache = ImageCache::of(fetch.cache);
    let found = fetch::collect_wanted_images(
        fetch.root,
        fetch.base,
        cache,
        fetch.scroll_y,
        fetch.viewport_h,
    );
    found.wanted.exclude_and_record(requested);
    if found.wanted.len() == 0 {
        return (None, found.deferred_any);
    }
    let session = images::start_session(&found.wanted, fetch.base, cache, arrived);
    (Some(session), found.deferred_any)
}
