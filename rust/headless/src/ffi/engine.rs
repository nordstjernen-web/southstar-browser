//! Southstar — the engine, network, document, style, paint, animation, image and video calls of the in-process run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_int, c_long, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_css::StyleMap;
use southstar_dom::{Node, Tree};
use southstar_engine::Page;
use southstar_layout::{BoxPool, BoxRef, NsBox};

pub use southstar_engine::{CssCache, PrintSetup};

pub use southstar_net::ResponseTiming as Timing;

pub struct Response {
    pub status: c_long,
    pub final_url: Option<Vec<u8>>,
    pub content_type: Option<Vec<u8>>,
    pub content_language: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
    pub error: Option<Vec<u8>>,
    pub timing: Timing,
}

fn owned(text: Option<&CStr>) -> Option<Vec<u8>> {
    text.map(|t| t.to_bytes().to_vec())
}

fn take_response(resp: southstar_net::Response) -> Response {
    Response {
        status: resp.status(),
        final_url: owned(resp.final_url()),
        content_type: owned(resp.content_type()),
        content_language: owned(resp.content_language()),
        body: resp.body().map(<[u8]>::to_vec),
        error: owned(resp.error()),
        timing: resp.timing(),
    }
}

pub fn navigate(
    url: &CStr,
    top_url: Option<&CStr>,
    post: Option<(&[u8], &CStr)>,
    user_activated: bool,
) -> Result<Response, Option<Vec<u8>>> {
    let post = post.map(|(body, content_type)| southstar_engine::Post {
        body,
        content_type: Some(content_type),
    });
    southstar_engine::navigate_blocking(url, top_url, top_url, user_activated, post.as_ref())
        .map(take_response)
}

pub fn fetch(url: &CStr, top_url: Option<&CStr>) -> Option<Response> {
    southstar_engine::fetch_blocking(url, top_url).map(take_response)
}

pub fn error_page(url: Option<&CStr>, status: c_long, transport_error: Option<&CStr>) -> Vec<u8> {
    southstar_net::build_error_page(
        url.map(CStr::to_bytes),
        status,
        transport_error.map(CStr::to_bytes),
    )
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

    pub fn free(self) {
        drop(self.0);
    }
}

pub fn node_dump(node: Node) -> Vec<u8> {
    southstar_dom::serialize::dump(Some(node))
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

pub fn compute_cascade(doc: &Document, base: Option<&CStr>, css_cache: CssCache) -> Rc<StyleMap> {
    southstar_engine::compute_cascade(&page(doc, base, css_cache))
}

pub use southstar_anim::AnimHandle as Anim;

pub fn load_keyframes(anim: Anim, doc: &Document, base: Option<&CStr>, css_cache: CssCache) {
    southstar_engine::load_keyframes(anim, &page(doc, base, css_cache));
}

pub fn anim_observe(anim: Anim, styles: Option<&StyleMap>, now_us: i64) {
    southstar_engine::anim_observe(anim, styles, now_us);
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

pub fn relayout(r: &Relayout, layout: &Cell<*mut NsBox>) -> Option<Rc<StyleMap>> {
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
    BoxPool::free_tree(unsafe { BoxRef::from_ptr(layout) });
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

type VideoSources = (Option<Rc<CStr>>, Option<Rc<CStr>>);

pub fn video_sources(b: BoxRef<'_>) -> Option<VideoSources> {
    let m = b.media()?;
    m.video()
        .is_null()
        .then(|| (m.video_src(), m.video_poster()))
}

pub use southstar_video::StillVideo as Video;
