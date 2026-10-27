//! Southstar — the surface drawImage() and createPattern() read from an image source: an ImageBitmap, a canvas, an <img> or a <video> poster, and whether it is origin-clean.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;

use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_js_engine::{Scope, Value};
use southstar_texture::Texture;
use southstar_video::BoxVideo;

use super::cairo::Surface;
use super::objects::NsNode;
use super::state::NsJs;
use crate::bitmap::{ImageBitmap, Source};

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

fn from_box(scope: &Scope<'_>, node: Node<'_>, name: &[u8]) -> Option<Found<'static>> {
    let b = crate::links::box_for_node(scope, node)?;
    let media = b.media()?;
    match name {
        b"img" => from_image(unsafe { ImageRef::from_ptr(media.image()) }?, false),
        b"video" => BoxVideo::of(b)?.with_poster(|texture, url| {
            Some(Found {
                texture: texture.map_or(ptr::null_mut(), |t| t.as_ptr().cast()),
                url: url.map_or(ptr::null(), CStr::as_ptr),
                cors_allow_origin: ptr::null(),
                cacheable: None,
            })
        }),
        _ => None,
    }
}

fn texture_surface(texture: *mut c_void) -> Option<Surface> {
    // SAFETY: texture is null or the image's or video's texture, which its owner keeps alive here.
    let texture = unsafe { Texture::borrow(texture.cast()) }?;
    let (w, h) = (texture.width(), texture.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let surface = Surface::image(w, h);
    if !surface.is_ok() {
        return None;
    }
    surface.write_pixels(|data, stride| texture.download(data, stride));
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
    let mut node = super::objects::offscreen_node(src);
    if node.is_null() {
        node = super::context::unwrap_element(src).ptr().cast();
    }
    let element = unsafe { Node::from_ptr(node.cast()) }?;
    let name = element.name()?.to_bytes();
    let js: *mut NsJs = scope.context_opaque().cast();
    if js.is_null() {
        return None;
    }
    if name == b"canvas" {
        return canvas_source(scope, node);
    }
    let cors_requested = element.attr(c"crossorigin").is_some();
    let found = from_box(scope, element, name)
        .filter(|f| !f.texture.is_null())
        .or_else(|| {
            if name != b"img" {
                return None;
            }
            from_image(southstar_js_images::image_for_node(js, element)?, true)
        })?;
    let allow = if cors_requested {
        found.cors_allow_origin
    } else {
        ptr::null()
    };
    let origin_clean = crate::links::resource_origin_clean();
    // SAFETY: the found URL and allowed origin are null or strings their image or video keeps alive here.
    let origin_clean = unsafe { origin_clean(scope, found.url, allow) };
    if let Some(image) = found.cacheable
        && let Some(cached) = unsafe { Surface::from_borrowed(image.render_surface()) }
    {
        let size = cached.size();
        return Some(Source {
            surface: cached,
            size,
            origin_clean,
        });
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
