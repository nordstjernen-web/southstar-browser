//! Southstar — the image, canvas, svg, audio and video boxes the block builder makes, and the background, border, mask and list-marker images a box carries from the layout image cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_dom::{Node, children};
use southstar_glib::{ascii_strtod, strdup};
use southstar_layout::{BoxPool, BoxRef};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::classify::{MEDIA_POSTER_ATTR, MEDIA_SRC_ATTR, node_has_media_metadata};
use crate::ffi;
use crate::state;

pub const KIND_BLOCK: u32 = 0;
pub const KIND_IMAGE: u32 = 3;
pub const KIND_VIDEO: u32 = 8;
pub const KIND_MATH: u32 = 9;
pub const KIND_SVG: u32 = 10;

pub fn peek_image(src: Option<&CStr>) -> *mut c_void {
    let Some(src) = src else {
        return ptr::null_mut();
    };
    let (cache, base) = state::with(|st| (st.image_cache, st.base_url));
    if cache.is_null() {
        return ptr::null_mut();
    }
    match ffi::url_resolve(base, src) {
        Some(abs) => ffi::image_cache_peek(cache, &abs),
        None => ffi::image_cache_peek(cache, src),
    }
}

fn dup_peek(src: &CStr) -> (*mut core::ffi::c_char, *mut c_void) {
    let copy = strdup(src.to_bytes());
    (copy, peek_image(Some(src)))
}

pub fn collect_box_bg_image(b: BoxRef<'_>, s: Option<StyleRef<'_>>) {
    if let Some(url) = s
        .and_then(|s| s.get(PropId::ListStyleImage))
        .and_then(ValueRef::url)
        && ffi::box_dom(b).is_some_and(|d| d.element_name() == Some(b"li"))
    {
        let (src, img) = dup_peek(url);
        b.ensure_media().set_marker_image(src, img);
    }
    if let Some(url) = ffi::border_image_source(s).and_then(ValueRef::url) {
        let (src, img) = dup_peek(url);
        b.ensure_media().set_border_image(src, img);
    }
    let bg = s.and_then(|s| s.get(PropId::BackgroundImage));
    let any_url = bg.is_some_and(|bg| bg.layers().any(|l| l.url().is_some()));
    let mask = s
        .and_then(|s| s.get(PropId::MaskImage))
        .and_then(ValueRef::url);
    if !any_url && mask.is_none() {
        return;
    }
    let m = b.ensure_media();
    let Some(bg) = bg.filter(|_| any_url) else {
        if let Some(mask) = mask {
            let (src, img) = dup_peek(mask);
            m.set_bg_image(src, img);
        }
        return;
    };
    if bg.next_layer().is_some() {
        let layers: Vec<_> = bg.layers().map(ValueRef::url).collect();
        ffi::set_bg_layers(m, &layers);
    }
    if let Some(url) = bg.layers().find_map(ValueRef::url) {
        let (src, img) = dup_peek(url);
        m.set_bg_image(src, img);
    }
}

pub fn image_dimension_attr(n: Option<Node<'_>>, name: &CStr) -> f64 {
    let Some(s) = n.and_then(|n| n.attr(name)).filter(|s| !s.is_empty()) else {
        return 0.0;
    };
    let (v, used) = southstar_glib::ascii_strtod_prefix(s.to_bytes());
    if used == 0 || v.is_nan() || v <= 0.0 {
        return 0.0;
    }
    v
}

fn decode_data_image(url: &CStr, key: &CStr) -> *mut c_void {
    let cache = state::with(|st| st.image_cache);
    if cache.is_null() || !url.to_bytes().starts_with(b"data:image/") {
        return ptr::null_mut();
    }
    let cached = ffi::image_cache_peek(cache, key);
    if !cached.is_null() {
        return cached;
    }
    match ffi::data_url_decode(url, 64 * 1024 * 1024) {
        Some(bytes) if !bytes.is_empty() => ffi::image_cache_insert_encoded(cache, key, &bytes),
        _ => ptr::null_mut(),
    }
}

pub fn build_image_box(n: Node<'_>) -> BoxRef<'static> {
    let (url_ptr, img, density) = ffi::choose_img_url(n);
    let url = ffi::c_str(url_ptr);
    let b = BoxPool::new_box(KIND_IMAGE);
    b.set_dom(Node::ptr_or_null(img).cast());
    let m = b.ensure_media();
    m.set_image_src(url_ptr);
    m.set_image_density(if density > 0.0 { density } else { 1.0 });
    b.set_content_width(image_dimension_attr(img, c"width"));
    b.set_content_height(image_dimension_attr(img, c"height"));
    let file_w = image_dimension_attr(img, c"data-file-width");
    let file_h = image_dimension_attr(img, c"data-file-height");
    if b.content_width() <= 0.0 && b.content_height() > 0.0 && file_w > 0.0 && file_h > 0.0 {
        b.set_content_width(b.content_height() * file_w / file_h);
    }
    if b.content_height() <= 0.0 && b.content_width() > 0.0 && file_w > 0.0 && file_h > 0.0 {
        b.set_content_height(b.content_width() * file_h / file_w);
    }
    let declared = b.content_width() > 0.0 && b.content_height() > 0.0;
    m.set_declared_image_size(declared);
    let mut image = ptr::null_mut();
    let (cache, base) = state::with(|st| (st.image_cache, st.base_url));
    if !cache.is_null() {
        let abs = url.and_then(|u| ffi::url_resolve(base, u));
        let key = abs.as_deref().or(url);
        if let (Some(u), Some(k)) = (url, key) {
            image = decode_data_image(u, k);
        }
        if image.is_null() {
            image = key.map_or(ptr::null_mut(), |k| ffi::image_cache_peek(cache, k));
        }
        m.set_image(image);
    }
    if !declared
        && url.is_some()
        && image.is_null()
        && b.content_width() <= 0.0
        && b.content_height() <= 0.0
    {
        b.set_content_width(200.0);
        b.set_content_height(150.0);
        m.set_placeholder_image_size(true);
    }
    b
}

fn first_media_source_url<'a>(n: Node<'a>) -> Option<&'a CStr> {
    for name in [MEDIA_SRC_ATTR, c"src", c"data-mp4", c"data-webm"] {
        if let Some(src) = n.attr(name).filter(|s| !s.is_empty()) {
            return Some(src);
        }
    }
    children(n)
        .filter(|c| c.element_name() == Some(b"source"))
        .find_map(|c| c.attr(c"src").filter(|s| !s.is_empty()))
}

fn strtod_or(s: Option<&CStr>, dflt: f64) -> f64 {
    s.map_or(dflt, |s| ascii_strtod(s.to_bytes()))
}

pub fn build_audio_box(n: Node<'_>) -> BoxRef<'static> {
    let b = BoxPool::new_box(KIND_VIDEO);
    b.set_dom(n.as_ptr().cast());
    let m = b.ensure_media();
    if let Some(src) = first_media_source_url(n) {
        m.set_video_audio_src(strdup(src.to_bytes()));
    }
    if n.attr(c"controls").is_some() {
        b.set_content_width(strtod_or(n.attr(c"width"), 250.0));
        b.set_content_height(strtod_or(n.attr(c"height"), 32.0));
    }
    b
}

pub fn build_video_box(n: Node<'_>) -> BoxRef<'static> {
    let src = first_media_source_url(n);
    let b = BoxPool::new_box(KIND_VIDEO);
    b.set_dom(n.as_ptr().cast());
    let m = b.ensure_media();
    if let Some(src) = src {
        m.set_video_src(strdup(src.to_bytes()));
    }
    let mut poster = n.attr(c"poster");
    let data_poster = n.attr(c"data-poster");
    let fallback_poster = n.attr(MEDIA_POSTER_ATTR);
    if poster.is_none_or(|p| p.is_empty() || p.to_bytes().starts_with(b"data:image/"))
        && data_poster.is_some_and(|p| !p.is_empty())
    {
        poster = data_poster;
    }
    if poster.is_none_or(|p| p.is_empty()) && fallback_poster.is_some_and(|p| !p.is_empty()) {
        poster = fallback_poster;
    }
    if let Some(p) = poster.filter(|p| !p.is_empty()) {
        m.set_video_poster(strdup(p.to_bytes()));
    }
    let metadata = node_has_media_metadata(Some(n));
    b.set_content_width(strtod_or(
        n.attr(c"width"),
        if metadata { 640.0 } else { 300.0 },
    ));
    b.set_content_height(strtod_or(
        n.attr(c"height"),
        if metadata { 360.0 } else { 150.0 },
    ));
    if let Some(audio) = n.attr(c"data-audio-src").filter(|a| !a.is_empty()) {
        m.set_video_audio_src(strdup(audio.to_bytes()));
    }
    b
}

pub fn build_svg_box(n: Node<'_>, s: Option<StyleRef<'_>>, styles: StyleTable) -> BoxRef<'static> {
    let b = BoxPool::new_box(KIND_SVG);
    b.set_dom(n.as_ptr().cast());
    b.set_style(s.map_or(ptr::null(), StyleRef::as_ptr));
    b.set_svg_styles(styles.as_ptr());
    let m = b.ensure_media();
    let size = ffi::svg_intrinsic_size(n);
    let px = |p| match s.and_then(|s| s.get(p)).and_then(ValueRef::length) {
        Some((v, 0)) => v,
        _ => -1.0,
    };
    let (css_w, css_h) = (px(PropId::Width), px(PropId::Height));
    let mut w = if css_w > 0.0 {
        css_w
    } else if size.has_width {
        size.width
    } else {
        0.0
    };
    let mut h = if css_h > 0.0 {
        css_h
    } else if size.has_height {
        size.height
    } else {
        0.0
    };
    if w > 0.0 && h <= 0.0 && size.has_ratio {
        h = w / size.ratio;
    } else if h > 0.0 && w <= 0.0 && size.has_ratio {
        w = h * size.ratio;
    }
    let root_document = n
        .parent()
        .is_none_or(|p| p.kind() == southstar_dom::Kind::Document);
    if root_document {
        let (vw, vh) = (ffi::viewport_w(), ffi::viewport_h());
        if w <= 0.0 && vw > 0.0 {
            w = vw;
        }
        if h <= 0.0 && vh > 0.0 {
            h = vh;
        }
    }
    if w <= 0.0 && h <= 0.0 && size.has_ratio && !root_document {
        m.set_intrinsic_ratio_only(true);
        w = 0.0;
        h = 0.0;
    } else if w <= 0.0 && h <= 0.0 {
        w = 300.0;
        h = 150.0;
    } else if w <= 0.0 {
        w = 300.0;
    } else if h <= 0.0 {
        h = 150.0;
    }
    b.set_content_width(w);
    b.set_content_height(h);
    m.set_declared_image_size(w > 0.0 && h > 0.0);
    b
}

pub fn build_canvas_box(n: Node<'_>, s: Option<StyleRef<'_>>) -> BoxRef<'static> {
    let b = BoxPool::new_box(KIND_IMAGE);
    b.set_dom(n.as_ptr().cast());
    b.set_style(s.map_or(ptr::null(), StyleRef::as_ptr));
    let m = b.ensure_media();
    b.set_content_width(image_dimension_attr(Some(n), c"width"));
    b.set_content_height(image_dimension_attr(Some(n), c"height"));
    if b.content_width() <= 0.0 {
        b.set_content_width(300.0);
    }
    if b.content_height() <= 0.0 {
        b.set_content_height(150.0);
    }
    m.set_declared_image_size(true);
    b
}
