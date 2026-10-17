//! Southstar — style sheets and their imports, the CSS parse caches, frame viewports for media queries, the cascade and the render context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_css::OwnedSheet;
use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GHashTable};
use southstar_js_runtime::api;
use southstar_layout::{BoxPool, BoxRef, NsBox};

use super::net::Bytes;

pub use southstar_render::{RenderCtx, RenderProfile};

unsafe extern "C" {
    fn g_printerr(format: *const c_char, ...);
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

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

pub fn layout_frame_viewport_raw(frame: usize) -> Option<(f64, f64)> {
    // SAFETY: frame is the address of a node of this thread's store, and from_ptr returns None once that node is freed.
    unsafe { Node::from_ptr(frame as *const NsNode) }.and_then(layout_frame_viewport)
}

pub fn layout_frame_viewport(frame: Node) -> Option<(f64, f64)> {
    southstar_layout_query::frame_viewport(frame)
}

pub struct Relayout<'a> {
    pub doc: *mut NsNode,
    pub base: Option<&'a CStr>,
    pub viewport_width: c_int,
    pub viewport_height: f64,
    pub images: *mut c_void,
    pub anim: Option<southstar_anim::AnimHandle>,
    pub js: *mut c_void,
    pub focused: *const NsNode,
    pub hover: *const NsNode,
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

pub fn load_into_anim(anim: southstar_anim::AnimHandle, sheets: &SheetList) {
    for sheet in &sheets.sheets {
        anim.load_keyframes(sheet);
    }
}

pub fn css_compute(doc: Option<Node>, sheets: &SheetList) -> *mut GHashTable {
    southstar_css::compute_styles(doc, &sheets.sheets, &sheets.docs, None)
}

pub fn render_ctx<'a>(r: &Relayout, sheets: &'a SheetList<'a>) -> RenderCtx<'a> {
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
        base_url: ptr_or_null(r.base),
        anim: r.anim,
        js: r.js,
        focused_input: r.focused,
        hover_node: r.hover,
        caret_byte: r.caret_byte,
        sel_anchor_byte: r.sel_anchor_byte,
        resolve_url: None,
        font_allowed: None,
        cb_ud: ptr::null_mut(),
    }
}

pub fn publish_to_page(js: *mut c_void, styles: *mut GHashTable, layout: *mut NsBox) {
    if !js.is_null() {
        api::set_style_table(js.cast(), styles);
        api::set_layout_root(js.cast(), layout.cast_const().cast());
    }
}

pub fn render_relayout(ctx: &RenderCtx, out_layout: *mut *mut NsBox) -> *mut GHashTable {
    unsafe { southstar_render::ns_render_relayout(ctx, out_layout) }
}

pub fn render_relayout_profiled(
    ctx: &RenderCtx,
    out_layout: *mut *mut NsBox,
    viewport_width: c_int,
) -> *mut GHashTable {
    let mut prof = RenderProfile::default();
    let t0 = super::net::monotonic_us();
    let styles =
        unsafe { southstar_render::ns_render_relayout_profile(ctx, out_layout, &mut prof) };
    let total = super::net::monotonic_us() - t0;
    let nstyles = table_size(styles);
    unsafe {
        g_printerr(
            c"[profile] relayout vw=%d nodes=%u total=%.2fms css=%.2f style=%.2f layout=%.2f"
                .as_ptr(),
            viewport_width,
            nstyles,
            total as f64 / 1000.0,
            prof.css1_us as f64 / 1000.0,
            prof.style1_us as f64 / 1000.0,
            prof.layout1_us as f64 / 1000.0,
        );
        if prof.container_pass != 0 {
            g_printerr(
                c" | containers=%u passes=%u cq_collect=%.2f css2=%.2f style2=%.2f layout2=%.2f"
                    .as_ptr(),
                prof.containers,
                prof.container_passes,
                prof.container_us as f64 / 1000.0,
                prof.css2_us as f64 / 1000.0,
                prof.style2_us as f64 / 1000.0,
                prof.layout2_us as f64 / 1000.0,
            );
        }
        g_printerr(c"\n".as_ptr());
    }
    styles
}

pub fn table_size(table: *mut GHashTable) -> c_uint {
    if table.is_null() {
        0
    } else {
        unsafe { glib::g_hash_table_size(table) }
    }
}

pub fn discard_layout(js: *mut c_void, styles: *mut GHashTable, out_layout: *mut *mut NsBox) {
    if !js.is_null() {
        api::set_layout_root(js.cast(), ptr::null());
        api::set_style_table(js.cast(), ptr::null_mut());
    }
    unsafe {
        if !out_layout.is_null() && !(*out_layout).is_null() {
            BoxPool::free_tree(BoxRef::from_ptr(*out_layout));
            *out_layout = ptr::null_mut();
        }
        if !styles.is_null() {
            glib::g_hash_table_destroy(styles);
        }
    }
}

pub fn log_relayout(styles: *mut GHashTable, viewport_width: c_int) {
    const DLOG_RENDER: c_int = 3;
    let message = format!("styles={} vw={}", table_size(styles), viewport_width);
    southstar_debuglog::emit(DLOG_RENDER, b"relayout", message.as_bytes());
}
