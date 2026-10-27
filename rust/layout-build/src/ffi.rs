//! Southstar — the open-select and datalist entry points, and the css, image, svg, paint and layout-crate calls the box-tree builder makes, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use southstar_image::{ImageCache, ImageRef};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_layout::{
    BoxRef, InlineAtomic, InlineAttr, LinkRange, LinkVec, MediaHandle, MediaRef,
};
use southstar_style::{StyleRef, StyleTable, ValueRef};
use std::rc::Rc;

use crate::{block, generated, media, state};

pub use southstar_svg::SvgSize;

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

pub fn image_cache_peek(cache: ImageCache, url: &CStr) -> Option<MediaHandle> {
    cache.peek(url).map(ImageRef::media_handle)
}

pub fn image_cache_insert_encoded(
    cache: ImageCache,
    key: &CStr,
    bytes: &[u8],
) -> Option<MediaHandle> {
    cache.insert_encoded(key, bytes).map(ImageRef::media_handle)
}

pub fn image_cache_begin_generation(cache: Option<ImageCache>) {
    if let Some(cache) = cache {
        cache.begin_generation();
    }
}

pub fn image_cache_collect(cache: Option<ImageCache>) {
    if let Some(cache) = cache {
        cache.collect();
    }
}

pub fn data_url_decode(url: &CStr, budget: u64) -> Option<Vec<u8>> {
    southstar_net::decode_data_url(url.to_bytes(), budget)
}

pub fn choose_img_url<'a>(n: Node<'a>) -> (Option<CString>, Option<Node<'a>>, f64) {
    let chosen = southstar_layout::choose_img(n);
    let url = chosen.url.and_then(|url| {
        let end = url.iter().position(|&b| b == 0).unwrap_or(url.len());
        CString::new(&url[..end]).ok()
    });
    (url, Some(chosen.img), chosen.density)
}

pub fn svg_intrinsic_size(n: Node<'_>) -> SvgSize {
    southstar_svg::intrinsic_size(Some(n))
}

pub fn set_bg_layers(m: MediaRef<'_>, layers: &[Option<&CStr>]) {
    let srcs = layers.iter().map(|l| l.map(Rc::from)).collect();
    let images = layers.iter().map(|l| media::peek_image(*l)).collect();
    m.set_bg_layers(srcs, images);
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

pub use southstar_paint::sync_inline_atomic_offsets;

pub use southstar_layout_query::compute_paint_bounds;

pub fn record_frame_viewports(root: Option<BoxRef<'_>>) {
    southstar_layout_query::record_frame_viewports(root);
}

pub fn li_marker(li: Node<'_>, s: Option<StyleRef<'_>>) -> Option<Vec<u8>> {
    southstar_paint::inside_list_marker(li, s, 64)
}

pub fn utf8_get_char(s: &[u8], at: usize) -> u32 {
    southstar_layout::utf8_get_char(s, at)
}

pub fn unichar_isalnum(c: u32) -> bool {
    southstar_unicode::is_alnum(c)
}

pub struct RawLink<'a> {
    pub start: usize,
    pub len: usize,
    pub href: &'a CStr,
    pub target: Option<&'a CStr>,
    pub dom: Node<'a>,
}

pub struct RunParts<'a> {
    pub text: Vec<u8>,
    pub links: Vec<RawLink<'a>>,
    pub attrs: Vec<InlineAttr>,
    pub atomics: Vec<(usize, BoxRef<'static>)>,
}

pub fn new_inline_run(parts: RunParts<'_>) -> BoxRef<'static> {
    let b = southstar_layout::BoxPool::new_inline();
    if !parts.links.is_empty() {
        let mut links = LinkVec::new();
        for l in &parts.links {
            links.push(LinkRange::new(
                l.start,
                l.len,
                Some(l.href),
                l.target,
                Some(l.dom),
            ));
        }
        b.put_links(Some(links));
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
                if child.parent().is_none() {
                    child.set_parent(Some(b));
                }
                InlineAtomic::new(off, child)
            })
            .collect();
        b.put_atomics(Some(&atomics));
    }
    b.set_text_bytes(&parts.text);
    b
}

pub fn set_open_select(select: Option<Node<'_>>) {
    state::set_open_select(Node::ptr_or_null(select));
}

pub use state::set_datalist_open;

pub use block::flat_parent;
pub use media::image_dimension_attr;

pub fn abs_pending_len() -> usize {
    state::with(|st| st.abs_pending.as_ref().map_or(0, Vec::len))
}

type PendingEntry<'a> = (Option<Node<'a>>, Option<StyleRef<'a>>, bool);

pub fn abs_pending_entry(i: usize, table: StyleTable) -> Option<PendingEntry<'static>> {
    let (dom, pseudo, fixed) = state::with(|st| {
        st.abs_pending
            .as_ref()
            .and_then(|p| p.get(i))
            .map(|e| (e.dom, e.pseudo, e.fixed))
    })?;
    let dom = dom.and_then(southstar_dom::WeakNode::get);
    let pseudo = dom.and_then(|dom| table.get(dom)).and_then(|host| {
        [host.before(), host.after()]
            .into_iter()
            .flatten()
            .find(|s| s.as_ptr() == pseudo)
    });
    Some((dom, pseudo, fixed))
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
    Some((found.run, found.rel_x, found.rel_y))
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
    abox.set_pseudo_host(dom, after);
    media::collect_box_bg_image(abox, Some(ps));
    let mut generated = generated::build_pseudo_inline_for(Some(ps), Some(dom));
    if let Some(g) = generated
        && g.kind() == southstar_layout::BoxKind::Inline
        && g.text().is_some_and(|t| t.is_empty())
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

pub fn record_abs_static(key: usize, run: BoxRef<'static>, rel_x: f64, rel_y: f64) {
    state::with(|st| {
        if let Some(m) = st.abs_static.as_mut() {
            m.insert(key, state::AbsStatic { run, rel_x, rel_y });
        }
    });
}
