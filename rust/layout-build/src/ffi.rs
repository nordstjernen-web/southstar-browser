//! Southstar — the open-select and datalist entry points, and the css, image, svg, paint and layout-crate calls the box-tree builder makes, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{
    GBoolean, GPtrArray, g_array_append_vals, g_free, g_ptr_array_add, g_ptr_array_new,
    g_ptr_array_new_with_free_func, strdup,
};
use southstar_layout::{BoxRef, InlineAtomic, InlineAttr, LinkRange, MediaRef, NsBox, Style};
use southstar_style::{StyleRef, StyleTable, ValueRef};

use crate::{block, generated, media, state};

#[repr(C)]
#[derive(Default)]
struct RawSvgSize {
    width: f64,
    height: f64,
    ratio: f64,
    has_width: GBoolean,
    has_height: GBoolean,
    has_ratio: GBoolean,
}

pub struct SvgSize {
    pub width: f64,
    pub height: f64,
    pub ratio: f64,
    pub has_width: bool,
    pub has_height: bool,
    pub has_ratio: bool,
}

unsafe extern "C" {
    fn ns_image_cache_peek(cache: *mut c_void, url: *const c_char) -> *mut c_void;
    fn ns_image_cache_insert_encoded(
        cache: *mut c_void,
        url: *const c_char,
        data: *const u8,
        len: usize,
    ) -> *mut c_void;
    fn ns_image_cache_begin_generation(cache: *mut c_void);
    fn ns_image_cache_collect(cache: *mut c_void);
    fn ns_svg_intrinsic_size(svg: *const NsNode, out: *mut RawSvgSize);
    fn ns_paint_sync_inline_atomic_offsets(root: *mut NsBox);
    fn ns_paint_li_is_inside(s: *const Style) -> GBoolean;
    fn ns_paint_li_marker_text(
        li: *const NsNode,
        s: *const Style,
        out: *mut c_char,
        out_sz: usize,
    ) -> GBoolean;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_isalnum(c: u32) -> GBoolean;
}

pub fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

pub fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn style_ptr(s: Option<StyleRef<'_>>) -> *const Style {
    s.map_or(ptr::null(), StyleRef::as_ptr)
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

pub fn border_image_source<'a>(s: Option<StyleRef<'a>>) -> Option<ValueRef<'a>> {
    southstar_style::border_image_source(s)
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> c_int {
    southstar_style::font_stretch_rank(v)
}

pub fn set_container_dims(inline_px: f64, block_px: f64) {
    southstar_css::set_container_unit_dims(inline_px, block_px);
}

pub use southstar_layout_float::{clear_kind_of, float_side_of};

pub fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    let base = base?;
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())
        .and_then(|abs| CString::new(abs).ok())
}

pub fn image_cache_peek(cache: *mut c_void, url: &CStr) -> *mut c_void {
    unsafe { ns_image_cache_peek(cache, url.as_ptr()) }
}

pub fn image_cache_insert_encoded(cache: *mut c_void, key: &CStr, bytes: &[u8]) -> *mut c_void {
    unsafe { ns_image_cache_insert_encoded(cache, key.as_ptr(), bytes.as_ptr(), bytes.len()) }
}

pub fn image_cache_begin_generation(cache: *mut c_void) {
    unsafe { ns_image_cache_begin_generation(cache) };
}

pub fn image_cache_collect(cache: *mut c_void) {
    unsafe { ns_image_cache_collect(cache) };
}

pub fn data_url_decode(url: &CStr, budget: u64) -> Option<Vec<u8>> {
    southstar_net::decode_data_url(url.to_bytes(), budget)
}

pub fn choose_img_url<'a>(n: Node<'a>) -> (*mut c_char, Option<Node<'a>>, f64) {
    let chosen = southstar_layout::choose_img(n);
    let url = chosen.url.map_or(ptr::null_mut(), |url| strdup(&url));
    (url, Some(chosen.img), chosen.density)
}

pub fn svg_intrinsic_size(n: Node<'_>) -> SvgSize {
    let mut raw = RawSvgSize::default();
    unsafe { ns_svg_intrinsic_size(n.as_ptr(), &mut raw) };
    SvgSize {
        width: raw.width,
        height: raw.height,
        ratio: raw.ratio,
        has_width: raw.has_width != 0,
        has_height: raw.has_height != 0,
        has_ratio: raw.has_ratio != 0,
    }
}

pub fn set_bg_layers(m: MediaRef<'_>, layers: &[Option<&CStr>]) {
    unsafe {
        let srcs = g_ptr_array_new_with_free_func(Some(g_free));
        let images: *mut GPtrArray = g_ptr_array_new();
        for l in layers {
            let src = l.map_or(ptr::null_mut(), |s| strdup(s.to_bytes()));
            g_ptr_array_add(srcs, src.cast());
            g_ptr_array_add(images, media::peek_image(*l));
        }
        m.set_bg_layers(srcs, images);
    }
}

pub fn inline_merge_prefix(
    prefix: BoxRef<'static>,
    suffix: Option<BoxRef<'static>>,
) -> Option<BoxRef<'static>> {
    southstar_layout_tree::merge_prefix(Some(prefix), suffix)
}

pub fn is_table_box(n: Node<'_>, st: StyleTable) -> bool {
    southstar_layout_tree::is_table_box(Some(n), st)
}

pub fn node_is_table_internal(n: Option<Node<'_>>, st: StyleTable) -> bool {
    southstar_layout_tree::node_is_table_internal(n, st)
}

pub fn build_table(n: Node<'_>, st: StyleTable) -> Option<BoxRef<'static>> {
    Some(southstar_layout_tree::build_table(n, st))
}

pub fn build_anonymous_table(
    start: Node<'_>,
    end: Option<Node<'_>>,
    st: StyleTable,
) -> Option<BoxRef<'static>> {
    southstar_layout_tree::build_anonymous_table(Some(start), end, st)
}

pub fn sync_inline_atomic_offsets(root: BoxRef<'_>) {
    unsafe { ns_paint_sync_inline_atomic_offsets(root.as_mut_ptr()) };
}

pub use southstar_layout_query::compute_paint_bounds;

pub fn record_frame_viewports(root: *mut NsBox) {
    southstar_layout_query::record_frame_viewports(box_ref(root));
}

pub fn li_marker(li: Node<'_>, s: Option<StyleRef<'_>>) -> Option<Vec<u8>> {
    let sp = style_ptr(s);
    if unsafe { ns_paint_li_is_inside(sp) } == 0 {
        return None;
    }
    let mut marker = [0 as c_char; 64];
    let ok = unsafe { ns_paint_li_marker_text(li.as_ptr(), sp, marker.as_mut_ptr(), 64) };
    (ok != 0).then(|| {
        unsafe { CStr::from_ptr(marker.as_ptr()) }
            .to_bytes()
            .to_vec()
    })
}

pub fn utf8_get_char(s: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 8];
    let tail = &s[at.min(s.len())..];
    let n = tail.len().min(7);
    buf[..n].copy_from_slice(&tail[..n]);
    unsafe { g_utf8_get_char(buf.as_ptr().cast()) }
}

pub fn unichar_isalnum(c: u32) -> bool {
    unsafe { g_unichar_isalnum(c) != 0 }
}

pub struct RawLink {
    pub start: usize,
    pub len: usize,
    pub href: *const c_char,
    pub target: *const c_char,
    pub dom: *const NsNode,
}

pub struct RunParts {
    pub text: Vec<u8>,
    pub links: Vec<RawLink>,
    pub attrs: Vec<InlineAttr>,
    pub atomics: Vec<(usize, *mut NsBox)>,
}

fn dup_or_null(p: *const c_char) -> *mut c_char {
    c_str(p).map_or(ptr::null_mut(), |s| strdup(s.to_bytes()))
}

pub fn new_inline_run(parts: RunParts) -> *mut NsBox {
    let b = southstar_layout::BoxPool::new_inline();
    if !parts.links.is_empty() {
        let links = b.ensure_links_array();
        for l in &parts.links {
            let out = LinkRange::new(
                l.start,
                l.len,
                dup_or_null(l.href),
                dup_or_null(l.target),
                l.dom.cast(),
            );
            unsafe { g_array_append_vals(links, ptr::from_ref(&out).cast(), 1) };
        }
    }
    for a in &parts.attrs {
        if let Some(src) = a.bg_image_src() {
            b.set_bg_image_if_unset(src, a.bg_image());
        }
    }
    b.put_attrs(Some(&parts.attrs));
    if !parts.atomics.is_empty() {
        let atomics: Vec<InlineAtomic> = parts
            .atomics
            .iter()
            .map(|&(off, child)| {
                if let Some(c) = box_ref(child)
                    && c.parent().is_none()
                {
                    c.set_parent(Some(b));
                }
                InlineAtomic::new(off, child)
            })
            .collect();
        b.put_atomics(Some(&atomics));
    }
    b.set_text_bytes(&parts.text);
    b.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_set_open_select(select: *const NsNode) {
    state::set_open_select(select);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_set_datalist_open(open: GBoolean) {
    state::set_datalist_open(open != 0);
}

pub use block::flat_parent;
pub use media::image_dimension_attr;

pub fn abs_pending_len() -> usize {
    state::with(|st| st.abs_pending.as_ref().map_or(0, Vec::len))
}

pub fn abs_pending_entry<'a>(i: usize) -> Option<(Option<Node<'a>>, Option<StyleRef<'a>>, bool)> {
    let (dom, pseudo, fixed) = state::with(|st| {
        st.abs_pending
            .as_ref()
            .and_then(|p| p.get(i))
            .map(|e| (e.dom, e.pseudo, e.fixed))
    })?;
    Some((node(dom), style(pseudo), fixed))
}

pub fn abs_pending_clear() {
    state::with(|st| {
        if let Some(p) = st.abs_pending.as_mut() {
            p.clear();
        }
    });
}

pub fn abs_static_run<'a>(key: usize) -> Option<(BoxRef<'a>, f64, f64)> {
    let found = state::with(|st| st.abs_static.as_ref().and_then(|m| m.get(&key).copied()))?;
    box_ref(found.run).map(|run| (run, found.rel_x, found.rel_y))
}

pub fn abs_build_box(
    dom: Node<'_>,
    pseudo: Option<StyleRef<'_>>,
    table: StyleTable,
) -> Option<BoxRef<'static>> {
    let Some(ps) = pseudo else {
        state::with(|st| st.abs_force_build = true);
        let abox = block::build_block(Some(dom), table);
        state::with(|st| st.abs_force_build = false);
        return abox;
    };
    let abox = southstar_layout::BoxPool::new_box(media::KIND_BLOCK);
    abox.set_style(ps.as_ptr());
    let after = table
        .get(dom)
        .and_then(StyleRef::after)
        .is_some_and(|a| a.as_ptr() == ps.as_ptr());
    abox.set_pseudo_host(dom.as_ptr().cast(), after);
    media::collect_box_bg_image(abox, Some(ps));
    let mut generated = generated::build_pseudo_inline_for(Some(ps), Some(dom));
    if let Some(g) = generated
        && g.kind() == southstar_layout::BoxKind::Inline
        && g.text().is_some_and(CStr::is_empty)
        && g.inline_atomics().is_none()
    {
        southstar_layout::BoxPool::free_tree(Some(g));
        generated = None;
    }
    if let Some(g) = generated {
        southstar_layout_tree::append_child(abox, g);
    }
    Some(abox)
}

pub fn box_is_abs_placeholder(b: BoxRef<'_>) -> bool {
    let key = b.as_ptr() as usize;
    state::with(|st| {
        st.abs_placeholders
            .as_ref()
            .is_some_and(|m| m.contains_key(&key))
    })
}

pub fn abs_static_target(b: BoxRef<'_>) -> Option<usize> {
    let key = b.as_ptr() as usize;
    state::with(|st| {
        st.abs_static.as_ref()?;
        st.abs_placeholders
            .as_ref()
            .and_then(|m| m.get(&key).copied())
            .filter(|target| !target.is_null())
            .map(|target| target as usize)
    })
}

pub fn record_abs_static(key: usize, run: BoxRef<'_>, rel_x: f64, rel_y: f64) {
    let run = run.as_mut_ptr();
    state::with(|st| {
        if let Some(m) = st.abs_static.as_mut() {
            m.insert(key, state::AbsStatic { run, rel_x, rel_y });
        }
    });
}
