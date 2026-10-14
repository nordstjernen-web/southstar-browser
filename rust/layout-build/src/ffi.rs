//! Southstar — the ns_layout_* entry points the box-tree builder implements, and the css, net, image, svg, paint and layout-crate calls it makes, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{
    GBoolean, GHashTable, GPtrArray, boolean, g_array_append_vals, g_free, g_ptr_array_add,
    g_ptr_array_new, g_ptr_array_new_with_free_func, strdup,
};
use southstar_layout::{
    BoxRef, Edges, InlineAtomic, InlineAttr, LinkRange, MediaRef, NsBox, Style,
};
use southstar_paint::ffi as paint;
use southstar_paint::ffi::pango::{AttrList, Context, Layout};
use southstar_style::{NsCssValue, StyleRef, StyleTable, ValueRef};

use crate::{block, build, classify, generated, geom, hit, media, state};

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: u32,
}

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
    fn ns_css_calc_is_math_fn(v: *const NsCssValue) -> GBoolean;
    fn ns_css_calc_math_fn_px(v: *const NsCssValue, basis: f64) -> f64;
    fn ns_css_writing_mode(s: *const Style) -> c_int;
    fn ns_css_alignment_base(keyword: *const c_char) -> *const c_char;
    fn ns_css_border_image_source(s: *const Style) -> *const NsCssValue;
    fn ns_css_font_stretch_rank(v: *const NsCssValue) -> c_int;
    fn ns_css_set_container_dims(inline_px: f64, block_px: f64);
    fn ns_layout_clear_kind_of(s: *const Style) -> c_int;
    fn ns_layout_float_side_of(s: *const Style) -> c_int;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_image_cache_peek(cache: *mut c_void, url: *const c_char) -> *mut c_void;
    fn ns_image_cache_insert_encoded(
        cache: *mut c_void,
        url: *const c_char,
        data: *const u8,
        len: usize,
    ) -> *mut c_void;
    fn ns_image_cache_begin_generation(cache: *mut c_void);
    fn ns_image_cache_collect(cache: *mut c_void);
    fn ns_data_url_decode(
        url: *const c_char,
        out: *mut GByteArray,
        budget: u64,
        out_content_type: *mut *mut c_char,
        too_large: *mut GBoolean,
    ) -> GBoolean;
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_free(array: *mut GByteArray, free_segment: GBoolean) -> *mut u8;
    fn ns_layout_choose_img_url(
        n: *const NsNode,
        img_out: *mut *const NsNode,
        density: *mut f64,
    ) -> *mut c_char;
    fn ns_svg_intrinsic_size(svg: *const NsNode, out: *mut RawSvgSize);
    fn ns_layout_inline_merge_prefix(prefix: *mut NsBox, suffix: *mut NsBox) -> *mut NsBox;
    fn ns_layout_is_table_box(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
    fn ns_layout_node_is_table_internal(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
    fn ns_layout_build_table(n: *const NsNode, styles: *mut GHashTable) -> *mut NsBox;
    fn ns_layout_build_anonymous_table(
        start: *const NsNode,
        end: *const NsNode,
        styles: *mut GHashTable,
    ) -> *mut NsBox;
    fn ns_layout_reset_container_queries();
    fn ns_layout_block(b: *mut NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_apply_position_offsets(b: *mut NsBox, parent_w: f64, parent_h: f64);
    fn ns_layout_process_absolute_boxes(root: *mut NsBox, styles: *mut GHashTable, vw: f64);
    fn ns_paint_sync_inline_atomic_offsets(root: *mut NsBox);
    fn ns_layout_compute_paint_bounds(root: *mut NsBox);
    fn ns_layout_record_frame_viewports(root: *const NsBox);
    fn ns_layout_shift_box_tree(b: *mut NsBox, dx: f64, dy: f64);
    fn ns_layout_box_clips_children(b: *const NsBox) -> GBoolean;
    fn ns_inline_apply_atomic_shapes(list: *mut c_void, b: *const NsBox);
    fn ns_layout_apply_inline_spacing(list: *mut c_void, s: *const Style, text: *const c_char);
    fn ns_layout_apply_inline_layout_attrs(list: *mut c_void, b: *const NsBox);
    fn ns_inline_layout_set_attrs(layout: *mut c_void, list: *mut c_void, b: *const NsBox);
    fn ns_layout_style_blocks_hit_testing(s: *const Style) -> GBoolean;
    fn ns_layout_node_is_form_hit_target(n: *const NsNode) -> GBoolean;
    fn ns_layout_inline_attr_control_width(r: *const InlineAttr, b: *const NsBox) -> f64;
    fn ns_paint_li_is_inside(s: *const Style) -> GBoolean;
    fn ns_paint_li_marker_text(
        li: *const NsNode,
        s: *const Style,
        out: *mut c_char,
        out_sz: usize,
    ) -> GBoolean;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_isalnum(c: u32) -> GBoolean;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
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

fn value<'a>(v: *const NsCssValue) -> Option<ValueRef<'a>> {
    unsafe { ValueRef::from_ptr(v) }
}

pub fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

pub fn box_dom<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    node(b.dom_ptr().cast())
}

pub fn box_style<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    style(b.style())
}

fn styles(t: *mut GHashTable) -> StyleTable {
    unsafe { StyleTable::from_ptr(t) }
}

fn style_ptr(s: Option<StyleRef<'_>>) -> *const Style {
    s.map_or(ptr::null(), StyleRef::as_ptr)
}

fn box_ptr(b: Option<BoxRef<'_>>) -> *mut NsBox {
    b.map_or(ptr::null_mut(), BoxRef::as_mut_ptr)
}

pub fn calc_is_math_fn(v: ValueRef<'_>) -> bool {
    unsafe { ns_css_calc_is_math_fn(v.as_ptr()) != 0 }
}

pub fn calc_math_fn_px(v: ValueRef<'_>, basis: f64) -> f64 {
    unsafe { ns_css_calc_math_fn_px(v.as_ptr(), basis) }
}

pub fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

pub fn container_w() -> f64 {
    southstar_css::container_unit_dims().0
}

pub fn container_h() -> f64 {
    southstar_css::container_unit_dims().1
}

pub fn writing_mode(s: Option<StyleRef<'_>>) -> c_int {
    unsafe { ns_css_writing_mode(style_ptr(s)) }
}

pub fn alignment_base(kw: &CStr) -> &CStr {
    let base = unsafe { ns_css_alignment_base(kw.as_ptr()) };
    c_str(base).unwrap_or(kw)
}

pub fn border_image_source<'a>(s: Option<StyleRef<'a>>) -> Option<ValueRef<'a>> {
    value(unsafe { ns_css_border_image_source(style_ptr(s)) })
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> c_int {
    unsafe { ns_css_font_stretch_rank(v.map_or(ptr::null(), ValueRef::as_ptr)) }
}

pub fn set_container_dims(inline_px: f64, block_px: f64) {
    unsafe { ns_css_set_container_dims(inline_px, block_px) };
}

pub fn clear_kind_of(s: Option<StyleRef<'_>>) -> c_int {
    unsafe { ns_layout_clear_kind_of(style_ptr(s)) }
}

pub fn float_side_of(s: Option<StyleRef<'_>>) -> c_int {
    unsafe { ns_layout_float_side_of(style_ptr(s)) }
}

fn take_cstring(p: *mut c_char) -> Option<CString> {
    let s = c_str(p)?.to_owned();
    unsafe { g_free(p.cast()) };
    Some(s)
}

pub fn url_resolve(base: *const c_char, href: &CStr) -> Option<CString> {
    if base.is_null() {
        return None;
    }
    take_cstring(unsafe { ns_url_resolve(base, href.as_ptr()) })
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
    unsafe {
        let bytes = g_byte_array_new();
        let mut ctype: *mut c_char = ptr::null_mut();
        let mut too_large: GBoolean = 0;
        let ok = ns_data_url_decode(url.as_ptr(), bytes, budget, &mut ctype, &mut too_large);
        g_free(ctype.cast());
        let out = (ok != 0 && too_large == 0)
            .then(|| core::slice::from_raw_parts((*bytes).data, (*bytes).len as usize).to_vec());
        g_byte_array_free(bytes, 1);
        out
    }
}

pub fn choose_img_url<'a>(n: Node<'a>) -> (*mut c_char, Option<Node<'a>>, f64) {
    let mut img: *const NsNode = n.as_ptr();
    let mut density = 1.0;
    let url = unsafe { ns_layout_choose_img_url(n.as_ptr(), &mut img, &mut density) };
    (url, node(img), density)
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
    box_ref(unsafe { ns_layout_inline_merge_prefix(prefix.as_mut_ptr(), box_ptr(suffix)) })
}

pub fn is_table_box(n: Node<'_>, st: StyleTable) -> bool {
    unsafe { ns_layout_is_table_box(n.as_ptr(), st.as_ptr()) != 0 }
}

pub fn node_is_table_internal(n: Option<Node<'_>>, st: StyleTable) -> bool {
    unsafe { ns_layout_node_is_table_internal(Node::ptr_or_null(n), st.as_ptr()) != 0 }
}

pub fn build_table(n: Node<'_>, st: StyleTable) -> Option<BoxRef<'static>> {
    box_ref(unsafe { ns_layout_build_table(n.as_ptr(), st.as_ptr()) })
}

pub fn build_anonymous_table(
    start: Node<'_>,
    end: Option<Node<'_>>,
    st: StyleTable,
) -> Option<BoxRef<'static>> {
    box_ref(unsafe {
        ns_layout_build_anonymous_table(start.as_ptr(), Node::ptr_or_null(end), st.as_ptr())
    })
}

pub fn reset_container_queries() {
    unsafe { ns_layout_reset_container_queries() };
}

pub fn layout_block(root: BoxRef<'_>, width: f64) {
    unsafe { ns_layout_block(root.as_mut_ptr(), width, ptr::null()) };
}

pub fn apply_position_offsets(root: BoxRef<'_>, w: f64, h: f64) {
    unsafe { ns_layout_apply_position_offsets(root.as_mut_ptr(), w, h) };
}

pub fn process_absolute_boxes(root: BoxRef<'_>, st: StyleTable, vw: f64) {
    unsafe { ns_layout_process_absolute_boxes(root.as_mut_ptr(), st.as_ptr(), vw) };
}

pub fn sync_inline_atomic_offsets(root: BoxRef<'_>) {
    unsafe { ns_paint_sync_inline_atomic_offsets(root.as_mut_ptr()) };
}

pub fn compute_paint_bounds(root: BoxRef<'_>) {
    unsafe { ns_layout_compute_paint_bounds(root.as_mut_ptr()) };
}

pub fn record_frame_viewports(root: *mut NsBox) {
    unsafe { ns_layout_record_frame_viewports(root) };
}

pub fn new_layout(parent_style: *const Style) -> Layout {
    let layout = Layout::new(unsafe { Context::from_raw(paint::ns_paint_text_context()) });
    unsafe { paint::ns_paint_apply_inline_font(layout.raw(), parent_style) };
    layout
}

pub fn wrap_mode_for(s: *const Style) -> c_int {
    unsafe { paint::ns_paint_wrap_mode_for(s) }
}

pub fn apply_css_line_spacing(layout: &Layout, s: *const Style) {
    unsafe { paint::ns_paint_apply_css_line_spacing(layout.raw(), s) };
}

pub fn apply_inline_attrs(layout: &Layout, i18n: &AttrList, b: BoxRef<'_>, ps: *const Style) {
    unsafe {
        paint::ns_paint_apply_i18n(layout.raw(), i18n.raw(), b.as_ptr());
        paint::ns_paint_apply_font_features(i18n.raw(), ps, 0, u32::MAX);
        ns_inline_apply_atomic_shapes(i18n.raw(), b.as_ptr());
        ns_layout_apply_inline_spacing(i18n.raw(), ps, b.text_ptr());
        ns_layout_apply_inline_layout_attrs(i18n.raw(), b.as_ptr());
        ns_inline_layout_set_attrs(layout.raw(), i18n.raw(), b.as_ptr());
    }
}

pub fn start_align_overflow(layout: &Layout) {
    unsafe { paint::ns_paint_start_align_overflow(layout.raw()) };
}

pub fn style_blocks_hit_testing(s: Option<StyleRef<'_>>) -> bool {
    unsafe { ns_layout_style_blocks_hit_testing(style_ptr(s)) != 0 }
}

pub fn node_is_form_hit_target(n: Option<Node<'_>>) -> bool {
    unsafe { ns_layout_node_is_form_hit_target(Node::ptr_or_null(n)) != 0 }
}

pub fn inline_attr_control_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_inline_attr_control_width(r, b.as_ptr()) }
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

pub fn format_g(v: f64) -> Vec<u8> {
    let p = unsafe { g_strdup_printf(c"%g".as_ptr(), v) };
    take_cstring(p).map_or_else(Vec::new, CString::into_bytes)
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
pub unsafe extern "C" fn ns_layout_build(
    doc: *const NsNode,
    table: *mut GHashTable,
    viewport_width: f64,
    focused_input: *const NsNode,
    focused_caret_byte: usize,
    focused_sel_anchor_byte: usize,
    image_cache: *mut c_void,
    base_url: *const c_char,
) -> *mut NsBox {
    build::layout_build(build::BuildInput {
        doc: node(doc),
        styles: styles(table),
        viewport_width,
        focused_input: node(focused_input),
        caret_byte: focused_caret_byte,
        anchor_byte: focused_sel_anchor_byte,
        image_cache,
        base_url,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_set_open_select(select: *const NsNode) {
    state::set_open_select(select);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_set_datalist_open(open: GBoolean) {
    state::set_datalist_open(open != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_vertical_stack_text(text: *const c_char) -> *mut c_char {
    let Some(text) = c_str(text) else {
        return strdup(b"");
    };
    let bytes = text.to_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 2);
    let mut p = 0;
    while p < bytes.len() {
        let next = (p + southstar_layout_content::text_input::utf8_skip(bytes[p])).min(bytes.len());
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(&bytes[p..next]);
        p = next;
    }
    strdup(&out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_text_indent_px(s: *const Style, basis: f64) -> f64 {
    geom::text_indent_px(style(s), basis)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_text_indent_px(
    run: *const NsBox,
    s: *const Style,
    basis: f64,
) -> f64 {
    if box_ref(run).is_some_and(BoxRef::inline_split_tail) {
        return 0.0;
    }
    geom::text_indent_px(style(s), basis)
}

fn box_clips_for_page_height(b: BoxRef<'_>) -> bool {
    if b.parent().is_none() {
        return false;
    }
    if box_dom(b)
        .and_then(Node::name)
        .is_some_and(|n| matches!(n.to_bytes(), b"html" | b"body"))
    {
        return false;
    }
    unsafe { ns_layout_box_clips_children(b.as_ptr()) != 0 }
}

fn walk_max_bottom(b: BoxRef<'_>, out: &mut f64) {
    let bottom = b.y() + b.content_height();
    if bottom > *out {
        *out = bottom;
    }
    if box_clips_for_page_height(b) {
        return;
    }
    for c in southstar_layout::children(b) {
        walk_max_bottom(c, out);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_max_bottom(root: *const NsBox, seed: f64) -> f64 {
    let mut out = seed;
    if let Some(root) = box_ref(root) {
        walk_max_bottom(root, &mut out);
    }
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_peek_image(src: *const c_char) -> *mut c_void {
    media::peek_image(c_str(src))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_length_resolve(
    v: *const NsCssValue,
    basis: f64,
    fallback: f64,
) -> f64 {
    geom::length_resolve(value(v), basis, fallback)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_value_is_percent(v: *const NsCssValue) -> GBoolean {
    boolean(geom::value_is_percent(value(v)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_edges_from_style(
    s: *const Style,
    basis: f64,
    margin: *mut Edges,
    padding: *mut Edges,
    border: *mut Edges,
) {
    let (m, p, b) = geom::edges_from_style(style(s), basis);
    unsafe {
        *margin = m;
        *padding = p;
        *border = b;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_resolve_used_height(
    b: *const NsBox,
    hv: *const NsCssValue,
    width_basis: f64,
    fallback: f64,
) -> f64 {
    geom::resolve_used_height(box_ref(b), value(hv), width_basis, fallback)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_specified_height_to_content(b: *const NsBox, h: f64) -> f64 {
    box_ref(b).map_or(h, |b| geom::specified_height_to_content(b, h))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_clamp_height_minmax_px(s: *const Style, h: f64) -> f64 {
    geom::clamp_height_minmax_px(style(s), h)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_self_start_is_far_side(
    s: *const Style,
    horizontal_axis: GBoolean,
) -> GBoolean {
    boolean(geom::self_start_is_far_side(style(s), horizontal_axis != 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_translate_subtree(b: *mut NsBox, dx: f64, dy: f64) {
    if b.is_null() || (dx == 0.0 && dy == 0.0) {
        return;
    }
    unsafe { ns_layout_shift_box_tree(b, dx, dy) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_resolve_height_with_basis(
    hv: *const NsCssValue,
    width_basis: f64,
    height_basis: f64,
    fallback: f64,
) -> f64 {
    geom::resolve_height_with_basis(value(hv), width_basis, height_basis, fallback)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_containing_block_definite_height(b: *const NsBox) -> f64 {
    geom::containing_block_definite_height(box_ref(b))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_height_keyword_stretches(v: *const NsCssValue) -> GBoolean {
    boolean(geom::height_keyword_stretches(value(v)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_read_definite_height(b: *const NsBox) -> f64 {
    box_ref(b).map_or(0.0, BoxRef::read_definite_height)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_style_is_absolute_or_fixed(s: *const Style) -> GBoolean {
    boolean(geom::style_is_absolute_or_fixed(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_style_is_flex_container(s: *const Style) -> GBoolean {
    boolean(geom::style_is_flex_container(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_keyword_or(
    s: *const Style,
    p: c_int,
    fallback: *const c_char,
) -> *const c_char {
    let kw = usize::try_from(p).ok().and_then(|p| {
        style(s)
            .and_then(|s| s.value_at(p))
            .and_then(ValueRef::keyword_text)
    });
    match kw {
        Some(kw) => unsafe { ns_css_alignment_base(kw.as_ptr()) },
        None => fallback,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_aspect_ratio_number(
    v: *const NsCssValue,
    with_auto: *mut GBoolean,
) -> f64 {
    let (ratio, auto) = geom::aspect_ratio_number(value(v));
    if let Some(w) = unsafe { with_auto.as_mut() } {
        *w = boolean(auto);
    }
    ratio
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_item_align(
    c: *const NsBox,
    container_align: *const c_char,
) -> *const c_char {
    hit::flex_item_align(box_ref(c), container_align)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_align_is_baseline(align: *const c_char) -> GBoolean {
    boolean(hit::flex_align_is_baseline(c_str(align)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_item_baseline(c: *const NsBox, fallback: f64) -> f64 {
    box_ref(c).map_or(fallback, |c| hit::flex_item_baseline(c, fallback))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_box_form_hit(
    b: *const NsBox,
    local_x: f64,
    local_y: f64,
    parent_style: *const Style,
) -> *const NsNode {
    let hit =
        box_ref(b).and_then(|b| hit::inline_box_form_hit(b, local_x, local_y, style(parent_style)));
    Node::ptr_or_null(hit)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_style_creates_fixed_cb(s: *const Style) -> GBoolean {
    boolean(geom::style_creates_fixed_cb(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flex_direction_of(s: *const Style) -> *const c_char {
    geom::flex_direction_of(style(s)).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_flat_parent(n: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(block::flat_parent(node(n)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_abs_pending_len() -> u32 {
    state::with(|st| st.abs_pending.as_ref().map_or(0, |p| p.len() as u32))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_abs_pending_entry(
    i: u32,
    dom: *mut *const NsNode,
    pseudo: *mut *const Style,
    fixed: *mut GBoolean,
) -> GBoolean {
    let entry = state::with(|st| {
        st.abs_pending
            .as_ref()
            .and_then(|p| p.get(i as usize))
            .map(|e| (e.dom, e.pseudo, e.fixed))
    });
    let Some((d, p, f)) = entry else {
        return 0;
    };
    unsafe {
        *dom = d;
        *pseudo = p;
        *fixed = boolean(f);
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_abs_pending_clear() {
    state::with(|st| {
        if let Some(p) = st.abs_pending.as_mut() {
            p.clear();
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_abs_static_run(
    dom: *const NsNode,
    rel_x: *mut f64,
    rel_y: *mut f64,
) -> *mut NsBox {
    let found = state::with(|st| {
        st.abs_static
            .as_ref()
            .and_then(|m| m.get(&(dom as usize)).copied())
    });
    let Some(st) = found.filter(|st| !st.run.is_null()) else {
        return ptr::null_mut();
    };
    unsafe {
        *rel_x = st.rel_x;
        *rel_y = st.rel_y;
    }
    st.run
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_abs_build_box(
    dom: *const NsNode,
    pseudo: *const Style,
    table: *mut GHashTable,
) -> *mut NsBox {
    let Some(ps) = style(pseudo) else {
        state::with(|st| st.abs_force_build = true);
        let abox = block::build_block(node(dom), styles(table));
        state::with(|st| st.abs_force_build = false);
        return box_ptr(abox);
    };
    let abox = southstar_layout::BoxPool::new_box(media::KIND_BLOCK);
    abox.set_style(ps.as_ptr());
    media::collect_box_bg_image(abox, Some(ps));
    let mut generated = generated::build_pseudo_inline_for(Some(ps), node(dom));
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
    abox.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_first_baseline(b: *const NsBox, out: *mut f64) -> GBoolean {
    match box_ref(b).and_then(hit::box_first_baseline) {
        Some(v) => {
            unsafe { *out = v };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_box_is_abs_placeholder(b: *const NsBox) -> GBoolean {
    boolean(state::with(|st| {
        st.abs_placeholders
            .as_ref()
            .is_some_and(|m| m.contains_key(&(b as usize)))
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_abs_static_target(b: *const NsBox) -> *const NsNode {
    state::with(|st| {
        if st.abs_static.is_none() {
            return ptr::null();
        }
        st.abs_placeholders
            .as_ref()
            .and_then(|m| m.get(&(b as usize)).copied())
            .unwrap_or(ptr::null())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_record_abs_static(
    dom: *const NsNode,
    run: *mut NsBox,
    rel_x: f64,
    rel_y: f64,
) {
    state::with(|st| {
        if let Some(m) = st.abs_static.as_mut() {
            m.insert(dom as usize, state::AbsStatic { run, rel_x, rel_y });
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_image_dimension_attr(
    n: *const NsNode,
    name: *const c_char,
) -> f64 {
    match c_str(name) {
        Some(name) => media::image_dimension_attr(node(n), name),
        None => 0.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_block(
    n: *const NsNode,
    table: *mut GHashTable,
) -> *mut NsBox {
    box_ptr(block::build_block(node(n), styles(table)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_inline_run(
    first: *const NsNode,
    last_excl: *const NsNode,
    table: *mut GHashTable,
) -> *mut NsBox {
    crate::run::build_inline_run(node(first), last_excl, styles(table), true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_pseudo_block_for(
    ps: *const Style,
    host: *const NsNode,
) -> *mut NsBox {
    box_ptr(generated::build_pseudo_block_for(style(ps), node(host)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_pseudo_inline_for(
    ps: *const Style,
    host: *const NsNode,
) -> *mut NsBox {
    box_ptr(generated::build_pseudo_inline_for(style(ps), node(host)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_register_abs_pseudo(host: *const NsNode, ps: *const Style) {
    generated::register_abs_pseudo(node(host), style(ps));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_queue_absolute_node(n: *const NsNode, s: *const Style) {
    let fixed = southstar_layout_content::controls::keyword_is(
        style(s).and_then(|s| s.get(southstar_style::PropId::Position)),
        c"fixed",
    );
    state::queue_absolute(n, fixed);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_collect_box_bg_image(b: *mut NsBox, s: *const Style) {
    if let Some(b) = box_ref(b) {
        media::collect_box_bg_image(b, style(s));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_append_display_contents_children(
    b: *mut NsBox,
    n: *const NsNode,
    table: *mut GHashTable,
    blockify_children: GBoolean,
    pending_before: *mut *mut NsBox,
) {
    let (Some(b), Some(n)) = (box_ref(b), node(n)) else {
        return;
    };
    let Some(slot) = (unsafe { pending_before.as_mut() }) else {
        block::append_display_contents_children(b, n, styles(table), blockify_children != 0, None);
        return;
    };
    let mut pending = box_ref(*slot);
    block::append_display_contents_children(
        b,
        n,
        styles(table),
        blockify_children != 0,
        Some(&mut pending),
    );
    *slot = box_ptr(pending);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_append_generated_after(
    b: *mut NsBox,
    generated_box: *mut NsBox,
) {
    if let (Some(b), Some(g)) = (box_ref(b), box_ref(generated_box)) {
        generated::append_generated_after(b, g);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_is_inline_dom(
    n: *const NsNode,
    table: *mut GHashTable,
) -> GBoolean {
    boolean(classify::is_inline_dom(node(n), styles(table)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_continues_inline_run(
    n: *const NsNode,
    table: *mut GHashTable,
) -> GBoolean {
    boolean(classify::continues_inline_run(node(n), styles(table)))
}
