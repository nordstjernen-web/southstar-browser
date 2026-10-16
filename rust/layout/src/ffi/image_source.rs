//! Southstar — the C ABI of choosing an <img>'s URL and density, with the device pixel ratio, sizes, media query and image type answers the choice needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use crate::image_source::{Chosen, Environment};

unsafe extern "C" {
    fn ns_image_supports_mime(mime: *const c_char) -> GBoolean;
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

fn choose(node: Node<'_>) -> Chosen<'_> {
    let resolve_sizes =
        |sizes: Option<&[u8]>| southstar_css::sizes_resolve(sizes.unwrap_or_default());
    let media_matches = |query: &[u8]| southstar_css_media::query_matches(query);
    let supports_type =
        |mime: &[u8]| unsafe { ns_image_supports_mime(c_string(mime).as_ptr()) != 0 };
    Environment {
        device_pixel_ratio: southstar_css_media::device_pixel_ratio(),
        resolve_sizes: &resolve_sizes,
        media_matches: &media_matches,
        supports_type: &supports_type,
    }
    .choose(node)
}

fn url_ptr(url: Option<Vec<u8>>) -> *mut c_char {
    url.map_or(ptr::null_mut(), |url| glib::strdup(&url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_choose_img_url(
    n: *const NsNode,
    img_out: *mut *const NsNode,
    density: *mut f64,
) -> *mut c_char {
    let Some(node) = (unsafe { Node::from_ptr(n) }) else {
        return ptr::null_mut();
    };
    let chosen = choose(node);
    unsafe {
        if let Some(img_out) = img_out.as_mut() {
            *img_out = chosen.img.as_ptr();
        }
        if let Some(density) = density.as_mut() {
            *density = chosen.density;
        }
    }
    url_ptr(chosen.url)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_img_chosen_url(n: *const NsNode) -> *mut c_char {
    unsafe { Node::from_ptr(n) }.map_or(ptr::null_mut(), |node| url_ptr(choose(node).url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_img_chosen_density(n: *const NsNode) -> f64 {
    unsafe { Node::from_ptr(n) }
        .map(|node| choose(node).density)
        .filter(|&density| density > 0.0)
        .unwrap_or(1.0)
}
