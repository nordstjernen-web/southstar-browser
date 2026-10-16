//! Southstar — style sheets and their imports, the CSS parse caches, frame viewports for media queries, the cascade and the render context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_css::OwnedSheet;
use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_layout::NsBox;

use super::net::Bytes;

pub use southstar_render::{RenderCtx, RenderProfile};

unsafe extern "C" {
    fn ns_layout_frame_viewport(frame: *const NsNode, w: *mut f64, h: *mut f64) -> GBoolean;
    fn ns_anim_load_from_stylesheet(anim: *mut c_void, sheet: *const c_void);
    fn ns_js_set_layout_root(js: *mut c_void, root: *const NsBox);
    fn ns_js_set_style_table(js: *mut c_void, styles: *mut GHashTable);
    fn ns_box_free(b: *mut NsBox);
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
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
    let (mut w, mut h) = (0.0, 0.0);
    let ok = unsafe { ns_layout_frame_viewport(frame as *const NsNode, &mut w, &mut h) };
    (ok != 0).then_some((w, h))
}

pub fn layout_frame_viewport(frame: Node) -> Option<(f64, f64)> {
    layout_frame_viewport_raw(frame.as_ptr() as usize)
}

pub struct Relayout<'a> {
    pub doc: *mut NsNode,
    pub base: Option<&'a CStr>,
    pub viewport_width: c_int,
    pub viewport_height: f64,
    pub images: *mut c_void,
    pub anim: *mut c_void,
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

pub fn load_into_anim(anim: *mut c_void, sheets: &SheetList) {
    for sheet in &sheets.sheets {
        // SAFETY: anim is the embedder's animation engine, and the sheet stays alive for the call.
        unsafe { ns_anim_load_from_stylesheet(anim, sheet.as_ptr()) };
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
    unsafe {
        if !js.is_null() {
            ns_js_set_layout_root(js, ptr::null());
            ns_js_set_style_table(js, ptr::null_mut());
        }
        if !out_layout.is_null() && !(*out_layout).is_null() {
            ns_box_free(*out_layout);
            *out_layout = ptr::null_mut();
        }
        if !styles.is_null() {
            glib::g_hash_table_destroy(styles);
        }
    }
}

pub fn log_relayout(styles: *mut GHashTable, viewport_width: c_int) {
    const DLOG_RENDER: c_uint = 3;
    unsafe {
        let message = g_strdup_printf(
            c"styles=%u vw=%d".as_ptr(),
            table_size(styles),
            viewport_width,
        );
        ns_debug_log_emit_take(DLOG_RENDER, c"relayout".as_ptr(), message);
    }
}
