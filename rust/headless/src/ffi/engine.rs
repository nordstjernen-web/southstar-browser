//! Southstar — the engine, network, document, style, paint, animation, image and video calls of the in-process run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode, Tree};
use southstar_engine::Page;
use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GStr};
use southstar_layout::{BoxRef, NsBox};

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

#[repr(C)]
struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    _content_disposition: *mut c_char,
    _csp_header: *mut c_char,
    _xframe_options: *mut c_char,
    _x_content_type_options: *mut c_char,
    _cors_allow_origin: *mut c_char,
    _refresh: *mut c_char,
    content_language: *mut c_char,
    _raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
    _tls_warning: *mut c_char,
    _remote_ip: *mut c_char,
    _next_hop_protocol: *mut c_char,
    request_start_us: i64,
    request_start_real_ms: f64,
    domain_lookup_ms: f64,
    connect_ms: f64,
    tls_ms: f64,
    pretransfer_ms: f64,
    response_start_ms: f64,
    response_end_ms: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::offset_of!(NsResponse, response_end_ms) == 184);

pub use southstar_engine::{CssCache, PrintSetup};

unsafe extern "C" {
    fn ns_engine_navigate_blocking(
        url: *const c_char,
        top_url: *const c_char,
        user_activated: GBoolean,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_engine_navigate_post_blocking(
        url: *const c_char,
        top_url: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        user_activated: GBoolean,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_engine_fetch_blocking(
        url: *const c_char,
        top_url: *const c_char,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_build_error_page(
        url: *const c_char,
        status: c_long,
        transport_error: *const c_char,
    ) -> *mut c_char;
    fn ns_box_free(b: *mut NsBox);
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(s: *mut GString, free_segment: GBoolean) -> *mut c_char;
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub fn take_string(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

pub struct Timing {
    pub request_start_us: i64,
    pub request_start_real_ms: f64,
    pub domain_lookup_ms: f64,
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub pretransfer_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
}

pub struct Response {
    pub status: c_long,
    pub final_url: Option<Vec<u8>>,
    pub content_type: Option<Vec<u8>>,
    pub content_language: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
    pub error: Option<Vec<u8>>,
    pub timing: Timing,
}

fn owned(p: *const c_char) -> Option<Vec<u8>> {
    c_str(p).map(|s| s.to_bytes().to_vec())
}

unsafe fn take_response(resp: *mut NsResponse) -> Option<Response> {
    let r = unsafe { resp.as_ref() }?;
    let body = unsafe { r.body.as_ref() }.map(|b| {
        if b.data.is_null() || b.len == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(b.data, b.len as usize) }.to_vec()
        }
    });
    let out = Response {
        status: r.status,
        final_url: owned(r.final_url),
        content_type: owned(r.content_type),
        content_language: owned(r.content_language),
        body,
        error: owned(r.error),
        timing: Timing {
            request_start_us: r.request_start_us,
            request_start_real_ms: r.request_start_real_ms,
            domain_lookup_ms: r.domain_lookup_ms,
            connect_ms: r.connect_ms,
            tls_ms: r.tls_ms,
            pretransfer_ms: r.pretransfer_ms,
            response_start_ms: r.response_start_ms,
            response_end_ms: r.response_end_ms,
        },
    };
    unsafe { ns_response_free(resp) };
    Some(out)
}

fn error_message(err: *mut GError) -> Option<Vec<u8>> {
    let e = unsafe { err.as_ref() }?;
    let msg = owned(e.message);
    unsafe { glib::g_error_free(err) };
    msg
}

pub fn navigate(
    url: &CStr,
    top_url: Option<&CStr>,
    post: Option<(&[u8], &CStr)>,
    user_activated: bool,
) -> Result<Response, Option<Vec<u8>>> {
    let mut err: *mut GError = ptr::null_mut();
    let resp = unsafe {
        match post {
            Some((body, content_type)) => ns_engine_navigate_post_blocking(
                url.as_ptr(),
                ptr_or_null(top_url),
                body.as_ptr().cast(),
                body.len(),
                content_type.as_ptr(),
                glib::boolean(user_activated),
                &mut err,
            ),
            None => ns_engine_navigate_blocking(
                url.as_ptr(),
                ptr_or_null(top_url),
                glib::boolean(user_activated),
                &mut err,
            ),
        }
    };
    match unsafe { take_response(resp) } {
        Some(r) => {
            drop(error_message(err));
            Ok(r)
        }
        None => Err(error_message(err)),
    }
}

pub fn fetch(url: &CStr, top_url: Option<&CStr>) -> Option<Response> {
    unsafe {
        take_response(ns_engine_fetch_blocking(
            url.as_ptr(),
            ptr_or_null(top_url),
            ptr::null_mut(),
        ))
    }
}

pub fn error_page(url: Option<&CStr>, status: c_long, transport_error: Option<&CStr>) -> Vec<u8> {
    take_string(unsafe {
        ns_build_error_page(ptr_or_null(url), status, ptr_or_null(transport_error))
    })
    .unwrap_or_default()
}

pub fn image_document(url: Option<&CStr>) -> Vec<u8> {
    southstar_html_util::image_document(url.map_or(&b""[..], CStr::to_bytes))
}

pub fn json_document(url: Option<&CStr>, json: &[u8]) -> Option<Vec<u8>> {
    southstar_html_util::json_document(url.map(CStr::to_bytes), json)
}

pub fn xml_document(url: Option<&CStr>, xml: &[u8]) -> Vec<u8> {
    southstar_html_util::xml_document(url.map(CStr::to_bytes), xml)
}

pub fn decode_body(body: &[u8], content_type: Option<&CStr>) -> (Vec<u8>, Option<Vec<u8>>) {
    let (mut decoded, charset) =
        southstar_html_util::decode_body(body, content_type.map(CStr::to_bytes));
    if let Some(nul) = decoded.iter().position(|&b| b == 0) {
        decoded.truncate(nul);
    }
    (decoded, charset)
}

pub fn url_resolve(base: &CStr, href: &CStr) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())
}

pub fn suffix_before_ext(path: Option<&CStr>, suffix: &CStr) -> Option<CString> {
    let path = path?;
    let joined = southstar_engine::suffix_before_ext(path.to_bytes(), suffix.to_bytes());
    Some(CString::new(joined).unwrap_or_default())
}

pub struct Document(Tree);

impl Document {
    pub fn parse(html: &[u8], scripting: bool) -> Document {
        Document(southstar_html_parser::parse_document(html, scripting))
    }

    pub fn node(&self) -> Node<'static> {
        self.0.root()
    }

    pub fn as_ptr(&self) -> *mut NsNode {
        self.0.root().as_mut_ptr()
    }

    pub fn free(self) {
        drop(self.0);
    }
}

pub fn node_dump(node: Node) -> Vec<u8> {
    southstar_dom::serialize::dump(Some(node))
}

fn gstring_take(s: *mut GString) -> Vec<u8> {
    let Some(g) = (unsafe { s.as_ref() }) else {
        return Vec::new();
    };
    let bytes = if g.str.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(g.str.cast::<u8>(), g.len) }.to_vec()
    };
    unsafe { g_string_free(s, glib::TRUE) };
    bytes
}

pub fn with_gstring(fill: impl FnOnce(*mut GString)) -> Vec<u8> {
    let s = unsafe { g_string_new(ptr::null()) };
    fill(s);
    gstring_take(s)
}

pub fn dump_text(root: Option<BoxRef<'_>>) -> Vec<u8> {
    root.map_or_else(Vec::new, southstar_engine::dump_text)
}

pub fn dump_layout(root: Option<BoxRef<'_>>) -> Vec<u8> {
    root.map_or_else(Vec::new, southstar_engine::dump_layout)
}

pub fn print_setup_default() -> PrintSetup {
    PrintSetup::a4()
}

pub fn apply_render_page_rule(setup: &mut PrintSetup) {
    if let Some(rule) = southstar_engine::page_rule() {
        setup.apply_page_rule(&rule);
    }
}

pub fn css_set_print_media(printing: bool) {
    southstar_css_media::set_print_media(printing);
}

pub fn css_set_viewport(vw: f64, vh: f64) {
    southstar_css_media::set_base_viewport(vw, vh);
}

pub fn css_set_target_fragment(fragment: Option<&CStr>) {
    southstar_css::set_target_fragment(fragment.map(CStr::to_bytes));
}

pub fn css_set_doc_language(lang: Option<&CStr>) {
    southstar_css::set_doc_language(lang.map(CStr::to_bytes));
}

pub fn css_set_active_node(node: Option<Node>) {
    southstar_css::set_active_node(node);
}

fn page<'a>(doc: &Document, base: Option<&'a CStr>, css_cache: CssCache) -> Page<'a> {
    Page {
        doc: Some(doc.node()),
        base,
        css_cache: Some(css_cache),
    }
}

pub fn compute_cascade(
    doc: &Document,
    base: Option<&CStr>,
    css_cache: CssCache,
) -> *mut GHashTable {
    southstar_engine::compute_cascade(&page(doc, base, css_cache))
}

pub fn destroy_table(table: *mut GHashTable) {
    unsafe { glib::g_hash_table_destroy(table) };
}

pub use southstar_anim::AnimHandle as Anim;

pub fn load_keyframes(anim: Anim, doc: &Document, base: Option<&CStr>, css_cache: CssCache) {
    southstar_engine::load_keyframes(anim, &page(doc, base, css_cache));
}

pub fn anim_observe(anim: Anim, styles: *mut GHashTable, now_us: i64) {
    // SAFETY: the run passes the style table its cascade just computed, which is alive.
    unsafe { southstar_engine::anim_observe(anim, styles, now_us) };
}

pub fn anim_raw(anim: Option<Anim>) -> *mut c_void {
    anim.map_or(ptr::null_mut(), Anim::raw)
}

pub use southstar_paint::set_anim as paint_set_anim;

pub fn paint_set_js(js: *mut c_void) {
    // SAFETY: the run's script engine stays alive through the captures painted after this, and the next run replaces it before it paints.
    unsafe { southstar_paint::set_js(js) };
}

pub use southstar_image::ImageCache;

pub fn image_cache_raw(cache: Option<ImageCache>) -> *mut c_void {
    cache.map_or(ptr::null_mut(), ImageCache::raw)
}

pub use southstar_video::VideoCache;

pub struct Relayout<'a> {
    pub doc: Option<Node<'a>>,
    pub base: Option<&'a CStr>,
    pub vw: c_int,
    pub vh: f64,
    pub images: Option<ImageCache>,
    pub anim: Option<Anim>,
    pub js: Option<super::Js>,
    pub css_cache: CssCache,
    pub focused: Option<Node<'a>>,
    pub caret: usize,
    pub anchor: usize,
}

pub fn relayout(r: &Relayout, layout: &Cell<*mut NsBox>) -> *mut GHashTable {
    let req = southstar_engine::Relayout {
        page: Page {
            doc: r.doc,
            base: r.base,
            css_cache: Some(r.css_cache),
        },
        viewport_width: r.vw,
        viewport_height: r.vh,
        images: r.images,
        anim: r.anim,
        js: super::js_raw(r.js),
        focused: r.focused,
        hover: None,
        caret_byte: r.caret,
        sel_anchor_byte: r.anchor,
    };
    // SAFETY: the run's script engine, if any, is alive while it relays out its page.
    unsafe { southstar_engine::relayout(&req, layout) }
}

pub fn free_layout(layout: *mut NsBox) {
    southstar_paint::invalidate_3d();
    // SAFETY: the run passes its own layout tree, which nothing reads after this.
    unsafe { ns_box_free(layout) };
}

pub fn fetch_images(root: Option<BoxRef<'_>>, base: Option<&CStr>, cache: Option<ImageCache>) {
    if let (Some(root), Some(base), Some(cache)) = (root, base, cache) {
        southstar_engine::fetch_images(root, base, cache);
    }
}

pub fn write_png(root: Option<BoxRef<'_>>, path: Option<&CStr>) -> c_int {
    match (root, path) {
        (Some(root), Some(path)) => southstar_engine::write_png(root, path),
        _ => 2,
    }
}

pub fn write_pdf(root: Option<BoxRef<'_>>, path: Option<&CStr>) -> c_int {
    match (root, path) {
        (Some(root), Some(path)) => southstar_engine::write_pdf(root, path),
        _ => 2,
    }
}

pub fn write_pdf_paged(root: Option<BoxRef<'_>>, path: Option<&CStr>, setup: &PrintSetup) -> c_int {
    match (root, path) {
        (Some(root), Some(path)) => southstar_engine::write_pdf_paged(root, path, setup),
        _ => 2,
    }
}

pub use southstar_layout_hit::{hit_form_dom, hit_inline_dom, hit_link};

pub use southstar_layout_query::collect_videos;

pub fn video_sources(b: BoxRef<'_>) -> Option<(Option<&CStr>, Option<&CStr>)> {
    let m = b.media()?;
    m.video()
        .is_null()
        .then(|| (m.video_src(), m.video_poster()))
}

pub use southstar_video::StillVideo as Video;
