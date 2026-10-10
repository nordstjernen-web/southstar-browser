//! Southstar — the surface drawImage() and createPattern() read from an image source: an ImageBitmap, a canvas, an <img> or a <video> poster, and whether it is origin-clean.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_js_engine::quickjs::{self, JSContext};
use southstar_js_engine::{Scope, Value};
use southstar_layout::{BoxRef, NsBox};

use super::cairo::Surface;
use super::objects::NsNode;
use super::state::NsJs;
use crate::bitmap::{ImageBitmap, Source};

unsafe extern "C" {
    fn ns_unwrap_element(val: quickjs::JSValue) -> *const NsNode;
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_js_box_for_node(js: *mut NsJs, node: *const NsNode) -> *const NsBox;
    fn ns_js_image_for_node(js: *mut NsJs, node: *const NsNode) -> *const c_void;
    fn ns_js_resource_origin_clean(
        js: *mut NsJs,
        ctx: *mut JSContext,
        url: *const c_char,
        cors_allow_origin: *const c_char,
    ) -> c_int;
    fn ns_video_poster_texture(video: *const c_void, url: *mut *const c_char) -> *mut c_void;
    fn ns_texture_get_width(texture: *mut c_void) -> c_int;
    fn ns_texture_get_height(texture: *mut c_void) -> c_int;
    fn ns_texture_download(texture: *mut c_void, dst: *mut u8, dst_stride: usize);
}

struct Found<'a> {
    texture: *mut c_void,
    url: *const c_char,
    cors_allow_origin: *const c_char,
    cacheable: Option<ImageRef<'a>>,
}

fn from_image(image: ImageRef<'_>, cacheable: bool) -> Option<Found<'_>> {
    let texture = image.texture();
    (!texture.is_null()).then(|| Found {
        texture,
        url: image.source_url(),
        cors_allow_origin: image.cors_allow_origin(),
        cacheable: (cacheable && !image.is_animated()).then_some(image),
    })
}

fn from_box<'a>(js: *mut NsJs, node: *const NsNode, name: &[u8]) -> Option<Found<'a>> {
    let b = unsafe { BoxRef::from_ptr(ns_js_box_for_node(js, node)) }?;
    let media = b.media()?;
    match name {
        b"img" => from_image(unsafe { ImageRef::from_ptr(media.image()) }?, false),
        b"video" if !media.video().is_null() => {
            let mut url = ptr::null();
            let texture = unsafe { ns_video_poster_texture(media.video(), &mut url) };
            Some(Found {
                texture,
                url,
                cors_allow_origin: ptr::null(),
                cacheable: None,
            })
        }
        _ => None,
    }
}

fn texture_surface(texture: *mut c_void) -> Option<Surface> {
    let (w, h) = unsafe {
        (
            ns_texture_get_width(texture),
            ns_texture_get_height(texture),
        )
    };
    if w <= 0 || h <= 0 {
        return None;
    }
    let surface = Surface::image(w, h);
    if !surface.is_ok() {
        return None;
    }
    surface.write_pixels(|data, stride| unsafe {
        ns_texture_download(texture, data.as_mut_ptr(), stride);
    });
    Some(surface)
}

fn canvas_source(scope: &Scope<'_>, node: *const NsNode) -> Option<Source> {
    let js = super::state::js_of(scope);
    let st = crate::state::state_for(js, super::state::Node::from_addr(node as usize))?;
    let st = unsafe { &*st };
    let surface = unsafe { Surface::from_borrowed(st.surf) }?;
    Some(Source {
        surface,
        size: (st.w, st.h),
        origin_clean: st.origin_clean != 0,
    })
}

pub(crate) fn drawimage_source(scope: &mut Scope<'_>, src: &Value) -> Option<Source> {
    if !src.is_object() {
        return None;
    }
    if let Some(source) = super::with_bitmap(src, ImageBitmap::source).flatten() {
        return Some(source);
    }
    let raw = quickjs::raw(src);
    let mut node = super::objects::offscreen_node(src);
    if node.is_null() {
        node = unsafe { ns_unwrap_element(raw) };
    }
    let name = unsafe { Node::from_ptr(node.cast()) }?.name()?.to_bytes();
    let js: *mut NsJs = quickjs::context_opaque(scope).cast();
    if js.is_null() {
        return None;
    }
    if name == b"canvas" {
        return canvas_source(scope, node);
    }
    let cors_requested = !unsafe { ns_element_get_attr(node, c"crossorigin".as_ptr()) }.is_null();
    let found = from_box(js, node, name)
        .filter(|f| !f.texture.is_null())
        .or_else(|| {
            if name != b"img" {
                return None;
            }
            from_image(
                unsafe { ImageRef::from_ptr(ns_js_image_for_node(js, node)) }?,
                true,
            )
        })?;
    let allow = if cors_requested {
        found.cors_allow_origin
    } else {
        ptr::null()
    };
    let ctx = quickjs::raw_context(scope);
    let origin_clean = unsafe { ns_js_resource_origin_clean(js, ctx, found.url, allow) } != 0;
    if let Some(image) = found.cacheable {
        if let Some(cached) = unsafe { Surface::from_borrowed(image.render_surface()) } {
            let size = cached.size();
            return Some(Source {
                surface: cached,
                size,
                origin_clean,
            });
        }
    }
    let surface = texture_surface(found.texture)?;
    if let Some(image) = found.cacheable {
        image.set_render_surface(surface.reference().into_raw());
    }
    let size = surface.size();
    Some(Source {
        surface,
        size,
        origin_clean,
    })
}
