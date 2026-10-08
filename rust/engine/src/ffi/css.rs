//! Southstar — style sheets and their imports, the CSS parse caches, frame viewports for media queries, the cascade and the render context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray, GStr};
use southstar_layout::NsBox;

use super::net::{Bytes, GBytes};

#[repr(C)]
struct NsCssImport {
    url: *mut c_char,
    layer_name: *mut c_char,
    media: *mut c_char,
}

#[repr(C)]
struct NsCssStylesheetHead {
    _rules: *mut GPtrArray,
    imports: *mut GArray,
}

pub use southstar_render::{RenderCtx, RenderProfile};

type FrameViewportCb = unsafe extern "C" fn(frame: *const NsNode, w: *mut f64, h: *mut f64);

unsafe extern "C" {
    fn ns_css_media_query_matches(query: *const c_char) -> GBoolean;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_css_stylesheet_parse_import_cached(
        url: *const c_char,
        layer: *const c_char,
        bytes: *mut GBytes,
    ) -> *mut c_void;
    fn ns_css_stylesheet_parse_url_cached(
        url: *const c_char,
        text: *const c_char,
        len: isize,
    ) -> *mut c_void;
    fn ns_css_stylesheet_resolve_urls(sheet: *mut c_void, base_url: *const c_char);
    fn ns_css_merged_styles_cached(
        text: *const c_char,
        len: isize,
        base_url: *const c_char,
    ) -> *mut c_void;
    fn ns_css_stylesheet_from_style_element_cached(style: *mut NsNode) -> *mut c_void;
    fn ns_css_style_element_text(style: *mut NsNode) -> *mut c_char;
    fn ns_css_shadow_adopted_css(root: *mut NsNode) -> *mut c_char;
    fn ns_css_syntax_is_self_contained(text: *const c_char, len: usize) -> GBoolean;
    fn ns_css_style_element_cache_begin();
    fn ns_css_style_element_cache_end();
    fn ns_css_media_viewport_push(w: f64, h: f64);
    fn ns_css_media_viewport_pop();
    fn ns_layout_frame_viewport(frame: *const NsNode, w: *mut f64, h: *mut f64) -> GBoolean;
    fn ns_css_set_frame_viewport_cb(cb: FrameViewportCb);
    fn ns_css_relayout_enter();
    fn ns_css_relayout_leave();
    fn ns_css_set_doc_base(base: *const c_char);
    fn ns_css_compute(
        doc: *mut NsNode,
        sheets: *const *const c_void,
        docs: *const *const NsNode,
        n: c_uint,
    ) -> *mut GHashTable;
    fn ns_css_stylesheet_free(sheet: *mut c_void);
    fn ns_anim_load_from_stylesheet(anim: *mut c_void, sheet: *const c_void);
    fn ns_js_set_layout_root(js: *mut c_void, root: *const NsBox);
    fn ns_js_set_style_table(js: *mut c_void, styles: *mut GHashTable);
    fn ns_box_free(b: *mut NsBox);
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
    fn g_printerr(format: *const c_char, ...);
    fn g_ptr_array_set_size(array: *mut GPtrArray, length: c_int);
}

pub fn c_str_ptr<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn c_input(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take_text(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

pub struct SheetList(NonNull<GPtrArray>);

impl SheetList {
    pub unsafe fn from_ptr(array: *mut GPtrArray) -> Option<SheetList> {
        NonNull::new(array).map(SheetList)
    }

    pub fn new() -> SheetList {
        SheetList(NonNull::new(unsafe { glib::g_ptr_array_new() }).expect("g_ptr_array_new"))
    }

    pub fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.len as usize
    }

    pub fn push(&self, item: *mut c_void) {
        unsafe { glib::g_ptr_array_add(self.0.as_ptr(), item) };
    }

    fn data(&self) -> *const *const c_void {
        unsafe { self.0.as_ref() }.pdata.cast_const().cast()
    }

    fn items(&self) -> Vec<*mut c_void> {
        let a = unsafe { self.0.as_ref() };
        (0..a.len as usize)
            .map(|i| unsafe { *a.pdata.add(i) })
            .collect()
    }

    pub fn free_sheets(&self) {
        for sheet in self.items() {
            unsafe { ns_css_stylesheet_free(sheet) };
        }
    }

    pub fn truncate(&self) {
        unsafe { g_ptr_array_set_size(self.0.as_ptr(), 0) };
    }

    pub fn destroy(self) {
        unsafe { glib::g_ptr_array_free(self.0.as_ptr(), glib::TRUE) };
    }
}

pub struct Import<'a> {
    pub url: Option<&'a CStr>,
    pub layer_name: Option<&'a CStr>,
    pub media: Option<&'a CStr>,
}

#[derive(Clone, Copy)]
pub struct Stylesheet(NonNull<c_void>);

impl Stylesheet {
    fn from_ptr(p: *mut c_void) -> Option<Stylesheet> {
        NonNull::new(p).map(Stylesheet)
    }

    pub fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn imports<'a>(self) -> Vec<Import<'a>> {
        let head = unsafe { &*self.0.as_ptr().cast::<NsCssStylesheetHead>() };
        let Some(imports) = (unsafe { head.imports.as_ref() }) else {
            return Vec::new();
        };
        let data = imports.data.cast::<NsCssImport>();
        (0..imports.len as usize)
            .map(|i| {
                let im = unsafe { &*data.add(i) };
                Import {
                    url: c_str_ptr(im.url),
                    layer_name: c_str_ptr(im.layer_name),
                    media: c_str_ptr(im.media),
                }
            })
            .collect()
    }

    pub fn resolve_urls(self, base: Option<&CStr>) {
        unsafe { ns_css_stylesheet_resolve_urls(self.as_ptr(), ptr_or_null(base)) };
    }
}

pub fn media_query_matches(query: &CStr) -> bool {
    unsafe { ns_css_media_query_matches(query.as_ptr()) != 0 }
}

pub fn url_resolve_opt(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    unsafe { GStr::take(ns_url_resolve(ptr_or_null(base), href.as_ptr())) }.map(|s| s.to_owned())
}

pub fn parse_import_cached(url: &CStr, layer: Option<&CStr>, bytes: &Bytes) -> Option<Stylesheet> {
    Stylesheet::from_ptr(unsafe {
        ns_css_stylesheet_parse_import_cached(url.as_ptr(), ptr_or_null(layer), bytes.raw())
    })
}

pub fn parse_url_cached(url: &CStr, text: &[u8]) -> Option<Stylesheet> {
    let data = if text.is_empty() {
        ptr::null()
    } else {
        text.as_ptr().cast()
    };
    Stylesheet::from_ptr(unsafe {
        ns_css_stylesheet_parse_url_cached(url.as_ptr(), data, text.len() as isize)
    })
}

pub fn merged_styles_cached(run: &[u8], base: Option<&CStr>) -> Option<Stylesheet> {
    let mut text = run.to_vec();
    text.push(0);
    Stylesheet::from_ptr(unsafe {
        ns_css_merged_styles_cached(text.as_ptr().cast(), run.len() as isize, ptr_or_null(base))
    })
}

pub fn stylesheet_from_style_element_cached(style: Node) -> Option<Stylesheet> {
    Stylesheet::from_ptr(unsafe { ns_css_stylesheet_from_style_element_cached(style.as_mut_ptr()) })
}

pub fn style_element_text(style: Node) -> Option<Vec<u8>> {
    take_text(unsafe { ns_css_style_element_text(style.as_mut_ptr()) })
}

pub fn shadow_adopted_css(root: Node) -> Option<Vec<u8>> {
    take_text(unsafe { ns_css_shadow_adopted_css(root.as_mut_ptr()) })
}

pub fn is_self_contained(css: &[u8]) -> bool {
    let c = c_input(css);
    unsafe { ns_css_syntax_is_self_contained(c.as_ptr(), c.as_bytes().len()) != 0 }
}

pub fn style_element_cache_end() {
    unsafe { ns_css_style_element_cache_end() };
}

pub fn media_viewport_push(w: f64, h: f64) {
    unsafe { ns_css_media_viewport_push(w, h) };
}

pub fn media_viewport_pop() {
    unsafe { ns_css_media_viewport_pop() };
}

pub fn layout_frame_viewport_raw(frame: usize) -> Option<(f64, f64)> {
    let (mut w, mut h) = (0.0, 0.0);
    let ok = unsafe { ns_layout_frame_viewport(frame as *const NsNode, &mut w, &mut h) };
    (ok != 0).then_some((w, h))
}

pub fn layout_frame_viewport(frame: Node) -> Option<(f64, f64)> {
    layout_frame_viewport_raw(frame.as_ptr() as usize)
}

unsafe extern "C" fn on_frame_viewport(frame: *const NsNode, w: *mut f64, h: *mut f64) {
    let (fw, fh) =
        unsafe { Node::from_ptr(frame) }.map_or((0.0, 0.0), crate::styles::frame_viewport_measured);
    unsafe {
        if !w.is_null() {
            *w = fw;
        }
        if !h.is_null() {
            *h = fh;
        }
    }
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
        unsafe {
            ns_css_set_frame_viewport_cb(on_frame_viewport);
            ns_css_relayout_enter();
            ns_css_set_doc_base(ptr_or_null(base));
            ns_css_style_element_cache_begin();
        }
        CssScope
    }

    pub fn cache_begin(&self) {
        unsafe { ns_css_style_element_cache_begin() };
    }
}

impl Drop for CssScope {
    fn drop(&mut self) {
        unsafe { ns_css_relayout_leave() };
    }
}

pub fn load_into_anim(anim: *mut c_void, sheets: &SheetList) {
    for sheet in sheets.items() {
        unsafe { ns_anim_load_from_stylesheet(anim, sheet) };
    }
}

pub fn css_compute(doc: *mut NsNode, sheets: &SheetList, docs: &SheetList) -> *mut GHashTable {
    unsafe {
        ns_css_compute(
            doc,
            sheets.data(),
            docs.data().cast(),
            sheets.len() as c_uint,
        )
    }
}

pub fn render_ctx(r: &Relayout, sheets: &SheetList, docs: &SheetList) -> RenderCtx {
    RenderCtx {
        doc: r.doc,
        sheets: sheets.data(),
        sheet_docs: docs.data().cast(),
        n_sheets: sheets.len() as c_uint,
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
