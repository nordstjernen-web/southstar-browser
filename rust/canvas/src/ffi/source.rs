//! Southstar — the surface drawImage() and createPattern() read from an image source: an ImageBitmap, a canvas, an <img> or a <video> poster, and whether it is origin-clean.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};
use std::rc::Rc;

use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api::Page;
use southstar_texture::TextureRef;
use southstar_video::BoxVideo;

use super::cairo::Surface;
use crate::bitmap::{ImageBitmap, Source};

struct Found<'a> {
    texture: Option<TextureRef>,
    url: Option<CString>,
    cors_allow_origin: Option<CString>,
    cacheable: Option<ImageRef<'a>>,
}

fn from_image(image: ImageRef<'_>, cacheable: bool) -> Option<Found<'_>> {
    let texture = image.texture();
    texture.is_some().then(|| Found {
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
        b"img" => from_image(ImageRef::from_media(media.image()?)?, false),
        b"video" => BoxVideo::of(b)?.with_poster(|texture, url| {
            Some(Found {
                texture: texture.cloned(),
                url: url.map(CStr::to_owned),
                cors_allow_origin: None,
                cacheable: None,
            })
        }),
        _ => None,
    }
}

fn texture_surface(texture: Option<TextureRef>) -> Option<Surface> {
    let texture = texture?;
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

fn canvas_source(scope: &Scope<'_>, node: super::state::Node) -> Option<Source> {
    let js = super::state::js_of(scope);
    let st = crate::state::state_for(js, node)?;
    Some(Source {
        surface: st.surface(),
        size: st.size(),
        origin_clean: st.origin_clean.get(),
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
        node = super::context::unwrap_element(src);
    }
    let element = node.dom()?;
    let name = element.name()?.to_bytes();
    let js = Some(Page::current(scope)?);
    if name == b"canvas" {
        return canvas_source(scope, node);
    }
    let cors_requested = element.attr(c"crossorigin").is_some();
    let found = from_box(scope, element, name)
        .filter(|f| f.texture.is_some())
        .or_else(|| {
            if name != b"img" {
                return None;
            }
            from_image(southstar_js_images::image_for_node(js, element)?, true)
        })?;
    let allow = found
        .cors_allow_origin
        .as_deref()
        .filter(|_| cors_requested)
        .map(CStr::to_bytes);
    let origin_clean = found
        .url
        .as_deref()
        .is_some_and(|url| crate::links::resource_origin_clean()(scope, url.to_bytes(), allow));
    if let Some(image) = found.cacheable
        && let Some(cached) = image
            .render_cache()
            .and_then(|c| c.downcast_ref::<Surface>().map(Surface::reference))
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
        image.set_render_cache(Rc::new(surface.reference()));
    }
    let size = surface.size();
    Some(Source {
        surface,
        size,
        origin_clean,
    })
}
