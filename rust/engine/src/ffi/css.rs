//! Southstar — style sheets and their imports, the CSS parse caches, frame viewports for media queries, the cascade and the render context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_int, c_uint};
use std::ffi::CString;
use std::rc::Rc;

use southstar_css::{OwnedSheet, StyleMap};
use southstar_dom::Node;
use southstar_js_runtime::api;
use southstar_layout::{BoxPool, BoxRef};

use super::net::Bytes;

pub use southstar_render::{RenderCtx, RenderProfile};

#[derive(Default)]
pub struct SheetList<'a> {
    pub sheets: Vec<OwnedSheet>,
    pub docs: Vec<Node<'a>>,
}

pub fn media_query_matches(query: &CStr) -> bool {
    southstar_css_media::query_matches(query.to_bytes())
}

pub fn url_resolve_opt(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    southstar_url::bytes::resolve(base.map(CStr::to_bytes), href.to_bytes())
        .and_then(|abs| CString::new(abs).ok())
}

pub fn parse_import_cached(url: &CStr, layer: Option<&CStr>, bytes: &Bytes) -> OwnedSheet {
    southstar_css::parse_import_cached(url.to_bytes(), layer.map(CStr::to_bytes), bytes.data())
}

pub fn parse_url_cached(url: &CStr, text: &[u8]) -> Option<OwnedSheet> {
    (!text.is_empty()).then(|| southstar_css::parse_url_cached(url.to_bytes(), text))
}

pub fn merged_styles_cached(run: &[u8], base: Option<&CStr>) -> Option<OwnedSheet> {
    southstar_css::merged_styles_cached(run, base.map(CStr::to_bytes))
}

pub fn stylesheet_from_style_element_cached(style: Node) -> Option<OwnedSheet> {
    southstar_css::stylesheet_from_style_element_cached(style)
}

pub fn style_element_text(style: Node) -> Option<Vec<u8>> {
    southstar_css::style_element_text(style)
}

pub fn shadow_adopted_css(root: Node) -> Option<Vec<u8>> {
    southstar_css::shadow_adopted_text(root)
}

pub fn is_self_contained(css: &[u8]) -> bool {
    let end = css.iter().position(|&b| b == 0).unwrap_or(css.len());
    southstar_css_syntax::is_self_contained(&css[..end])
}

pub fn style_element_cache_end() {
    southstar_css::style_element_cache_end();
}

pub fn media_viewport_push(w: f64, h: f64) {
    southstar_css_media::viewport_push(w, h);
}

pub fn media_viewport_pop() {
    southstar_css_media::viewport_pop();
}

pub fn layout_frame_viewport(frame: Node) -> Option<(f64, f64)> {
    southstar_layout_query::frame_viewport(frame)
}

pub struct Relayout<'a> {
    pub doc: Option<Node<'a>>,
    pub base: Option<&'a CStr>,
    pub viewport_width: c_int,
    pub viewport_height: f64,
    pub images: Option<southstar_image::ImageCache>,
    pub anim: Option<southstar_anim::AnimHandle>,
    pub focused: Option<Node<'a>>,
    pub hover: Option<Node<'a>>,
    pub caret_byte: usize,
    pub sel_anchor_byte: usize,
}

pub struct CssScope;

impl CssScope {
    pub fn enter(base: Option<&CStr>) -> CssScope {
        southstar_css::set_frame_viewport_measure(Some(crate::styles::frame_viewport_measured));
        southstar_css::relayout_enter();
        southstar_css::set_doc_base(base.map(CStr::to_bytes));
        southstar_css::style_element_cache_begin();
        CssScope
    }

    pub fn cache_begin(&self) {
        southstar_css::style_element_cache_begin();
    }
}

impl Drop for CssScope {
    fn drop(&mut self) {
        southstar_css::relayout_leave();
    }
}

pub fn load_into_anim(anim: &southstar_anim::AnimHandle, sheets: &SheetList) {
    for sheet in &sheets.sheets {
        anim.load_keyframes(sheet);
    }
}

pub fn css_compute(doc: Option<Node>, sheets: &SheetList) -> StyleMap {
    southstar_css::compute_styles(doc, &sheets.sheets, &sheets.docs, None)
}

pub fn render_ctx<'a>(r: &Relayout<'a>, sheets: &'a SheetList<'a>) -> RenderCtx<'a> {
    RenderCtx {
        doc: r.doc,
        sheets: &sheets.sheets,
        sheet_docs: &sheets.docs,
        viewport_width: r.viewport_width as f64,
        viewport_height: if r.viewport_height > 0.0 {
            r.viewport_height
        } else {
            r.viewport_width as f64 * 0.75
        },
        zoom: 1.0,
        images: r.images,
        base_url: r.base.map(CStr::to_bytes),
        anim: r.anim.clone(),
        focused_input: r.focused,
        hover_node: r.hover,
        caret_byte: r.caret_byte,
        sel_anchor_byte: r.sel_anchor_byte,
    }
}

pub fn publish_to_page(
    js: Option<api::Page>,
    styles: Option<&Rc<StyleMap>>,
    layout: Option<BoxRef<'static>>,
) {
    if js.is_some() {
        api::set_style_table(js, styles.cloned());
        api::set_layout_root(js, layout);
    }
}

pub fn render_relayout(ctx: &RenderCtx) -> (Rc<StyleMap>, Option<BoxRef<'static>>) {
    southstar_render::relayout(ctx, None)
}

pub fn render_relayout_profiled(
    ctx: &RenderCtx,
    viewport_width: c_int,
) -> (Rc<StyleMap>, Option<BoxRef<'static>>) {
    let mut prof = RenderProfile::default();
    let t0 = super::net::monotonic_us();
    let (styles, layout) = southstar_render::relayout(ctx, Some(&mut prof));
    let total = super::net::monotonic_us() - t0;
    let ms = |us: i64| us as f64 / 1000.0;
    let mut line = format!(
        "[profile] relayout vw={viewport_width} nodes={} total={:.2}ms css={:.2} style={:.2} layout={:.2}",
        table_size(Some(&styles)),
        ms(total),
        ms(prof.css1_us),
        ms(prof.style1_us),
        ms(prof.layout1_us),
    );
    if prof.container_pass != 0 {
        line.push_str(&format!(
            " | containers={} passes={} cq_collect={:.2} css2={:.2} style2={:.2} layout2={:.2}",
            prof.containers,
            prof.container_passes,
            ms(prof.container_us),
            ms(prof.css2_us),
            ms(prof.style2_us),
            ms(prof.layout2_us),
        ));
    }
    eprintln!("{line}");
    (styles, layout)
}

pub fn table_size(table: Option<&StyleMap>) -> c_uint {
    table.map_or(0, |table| table.len() as c_uint)
}

pub fn discard_layout(
    js: Option<api::Page>,
    styles: Option<Rc<StyleMap>>,
    layout: &Cell<Option<BoxRef<'static>>>,
) {
    if js.is_some() {
        api::set_layout_root(js, None);
        api::set_style_table(js, None);
    }
    BoxPool::free_tree(layout.take());
    drop(styles);
}

pub fn log_relayout(styles: Option<&StyleMap>, viewport_width: c_int) {
    const DLOG_RENDER: c_int = 3;
    let message = format!("styles={} vw={}", table_size(styles), viewport_width);
    southstar_debuglog::emit(DLOG_RENDER, b"relayout", message.as_bytes());
}
