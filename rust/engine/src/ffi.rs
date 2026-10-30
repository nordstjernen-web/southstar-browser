//! Southstar — the entry points of the engine and its remaining C ABI: captures, dumps, print sheets, fetches, style sheet collection, cascade and relayout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod css;
mod images;
mod net;

use core::cell::Cell;
use core::ffi::{CStr, c_int, c_void};
use core::ptr::NonNull;
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::OnceLock;

use southstar_anim::AnimHandle;
use southstar_cairo::{Content, Context, Format, PdfMetadata, Rectangle, Surface};
use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_glib as glib;
use southstar_layout::BoxRef;

pub use css::{
    CssScope, SheetList, is_self_contained, layout_frame_viewport, media_query_matches,
    media_viewport_pop, media_viewport_push, merged_styles_cached, parse_import_cached,
    parse_url_cached, shadow_adopted_css, style_element_cache_end, style_element_text,
    stylesheet_from_style_element_cached, url_resolve_opt,
};
pub use images::{ImageCache, ImageSession, Wanted, collect_images};
pub use net::{
    Bytes, CssCache, Dest, Post, fetch_blocking, fetch_blocking_with_headers, monotonic_us,
    navigate_blocking, nosniff, preconnect, preload_clear, preload_request, url_is_http_or_https,
    url_origin, url_resolve,
};

use crate::fetch;

pub use southstar_print::PrintSetup;

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
    cr: Context,
    surface: Surface,
}

impl Capture {
    fn with_surface(surface: Surface) -> Option<Capture> {
        surface.as_ref().status().ok()?;
        let cr = Context::new(surface.as_ref());
        Some(Capture { cr, surface })
    }

    pub fn image(width: i32, height: i32) -> Option<Capture> {
        Capture::with_surface(Surface::image(Format::Argb32, width, height)?)
    }

    pub fn pdf(path: &CStr, width: f64, height: f64) -> Option<Capture> {
        let surface = Surface::pdf(path, width, height);
        surface.as_ref().status().ok()?;
        set_pdf_metadata(&surface);
        Capture::with_surface(surface)
    }

    pub fn recording(width: f64, height: f64) -> Capture {
        let extent = Rectangle {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let surface = Surface::recording(Content::ColorAlpha, Some(&extent));
        let cr = Context::new(surface.as_ref());
        Capture { cr, surface }
    }

    pub fn paint(&self, root: BoxRef) {
        southstar_paint::paint(self.cr.as_ref(), root, None, None);
    }

    pub fn show_page(&self) {
        self.cr.as_ref().show_page();
    }

    pub fn draw_sheet(&self, root: BoxRef, setup: &PrintSetup, scale: f64, top: f64, bottom: f64) {
        southstar_print::draw_page(self.cr.as_ref(), root, setup, scale, top, bottom);
    }

    pub fn write_png(self, path: &CStr) -> Result<(), Vec<u8>> {
        self.into_surface()
            .as_ref()
            .write_to_png(path)
            .map_err(|error| error.description().to_bytes().to_vec())
    }

    fn into_surface(self) -> Surface {
        let Capture { cr, surface } = self;
        drop(cr);
        surface
    }
}

fn set_pdf_metadata(surface: &Surface) {
    let surface = surface.as_ref();
    surface.set_pdf_metadata(PdfMetadata::Creator, c"Southstar");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if let Ok(date) = CString::new(crate::iso8601_utc(now)) {
        surface.set_pdf_metadata(PdfMetadata::CreateDate, &date);
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
    pub js: Option<southstar_js_runtime::api::Page>,
    pub focused: Option<Node<'a>>,
    pub hover: Option<Node<'a>>,
    pub caret_byte: usize,
    pub sel_anchor_byte: usize,
}

pub fn relayout(
    req: &Relayout<'_>,
    layout: &Cell<Option<BoxRef<'static>>>,
) -> Option<Rc<StyleMap>> {
    let viewport_width = req.viewport_width;
    let js = req.js;
    let r = css::Relayout {
        doc: req.page.doc,
        base: req.page.base,
        viewport_width,
        viewport_height: req.viewport_height,
        images: req.images,
        anim: req.anim,
        focused: req.focused,
        hover: req.hover,
        caret_byte: req.caret_byte,
        sel_anchor_byte: req.sel_anchor_byte,
    };
    let cache = req.page.css_cache;
    let doc_node = req.page.doc;
    let scope = CssScope::enter(r.base);
    let mut sheets = crate::styles::collect_stylesheets(doc_node, r.base, cache);
    let ctx = css::render_ctx(&r, &sheets);
    let t0 = monotonic_us();
    let (mut styles, root) = if profile_enabled() {
        css::render_relayout_profiled(&ctx, viewport_width)
    } else {
        css::render_relayout(&ctx)
    };
    layout.set(root);
    css::publish_to_page(js, Some(&styles), root);
    if crate::styles::frame_viewports_disagree_with_layout() {
        css::discard_layout(js, Some(styles), layout);
        drop(sheets);
        scope.cache_begin();
        sheets = crate::styles::collect_stylesheets(doc_node, r.base, cache);
        let ctx = css::render_ctx(&r, &sheets);
        let root;
        (styles, root) = css::render_relayout(&ctx);
        layout.set(root);
        css::publish_to_page(js, Some(&styles), root);
    }
    crate::styles::add_relayout(monotonic_us() - t0);
    css::log_relayout(Some(&styles), viewport_width);
    drop(sheets);
    Some(styles)
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

pub struct Sheet(Surface);

impl Sheet {
    pub fn as_ptr(&self) -> NonNull<c_void> {
        NonNull::new(self.0.as_raw()).expect("a sheet holds a live surface")
    }

    pub fn into_raw(self) -> NonNull<c_void> {
        NonNull::new(self.0.into_raw()).expect("a sheet holds a live surface")
    }
}

pub fn print_sheets(root: BoxRef<'_>, setup: &PrintSetup) -> Vec<Sheet> {
    crate::print_recordings(root, setup)
        .into_iter()
        .map(|capture| Sheet(capture.into_surface()))
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
