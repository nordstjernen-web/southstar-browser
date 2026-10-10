//! Southstar — the URL and density an <img> loads: a <picture>'s first matching <source>, the lazy-loading data-* attributes sites use, then srcset and src, preferring a real URL over a data: placeholder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Node, children};

use crate::srcset;

pub struct Environment<'a> {
    pub device_pixel_ratio: f64,
    pub resolve_sizes: &'a dyn Fn(Option<&[u8]>) -> f64,
    pub media_matches: &'a dyn Fn(&[u8]) -> bool,
    pub supports_type: &'a dyn Fn(&[u8]) -> bool,
}

pub struct Chosen<'a> {
    pub url: Option<Vec<u8>>,
    pub density: f64,
    pub img: Node<'a>,
}

fn attr<'a>(el: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    el.attr(name).map(CStr::to_bytes)
}

fn non_empty<'a>(el: Node<'a>, names: &[&CStr]) -> Option<&'a [u8]> {
    let mut found = None;
    for &name in names {
        found = attr(el, name);
        if found.is_some_and(|v| !v.is_empty()) {
            break;
        }
    }
    found
}

fn is_data(url: &[u8]) -> bool {
    url.starts_with(b"data:")
}

fn is_named(node: Option<Node<'_>>, tag: &[u8]) -> bool {
    node.is_some_and(|n| n.element_name() == Some(tag))
}

impl Environment<'_> {
    fn select(
        &self,
        srcset: Option<&[u8]>,
        sizes: Option<&[u8]>,
        src: Option<&[u8]>,
    ) -> Option<(Vec<u8>, f64)> {
        srcset::select(
            srcset,
            sizes,
            src,
            self.device_pixel_ratio,
            self.resolve_sizes,
        )
    }

    fn picture_source(&self, picture: Node<'_>, img: Option<Node<'_>>) -> Option<(Vec<u8>, f64)> {
        let mut data_fallback = None;
        for source in children(picture) {
            if img.is_some_and(|img| img.as_ptr() == source.as_ptr()) {
                break;
            }
            if source.element_name() != Some(b"source") {
                continue;
            }
            if attr(source, c"type").is_some_and(|ty| !ty.is_empty() && !(self.supports_type)(ty)) {
                continue;
            }
            if attr(source, c"media").is_some_and(|m| !m.is_empty() && !(self.media_matches)(m)) {
                continue;
            }
            let sizes = attr(source, c"sizes");
            for set in [attr(source, c"data-srcset"), attr(source, c"srcset")] {
                let Some((url, density)) = self.select(set, sizes, None) else {
                    continue;
                };
                if !is_data(&url) {
                    return Some((url, density));
                }
                data_fallback.get_or_insert((url, density));
            }
            if let Some(src) = attr(source, c"src").filter(|s| !s.is_empty()) {
                if !is_data(src) {
                    return Some((src.to_vec(), 1.0));
                }
                data_fallback.get_or_insert((src.to_vec(), 1.0));
            }
        }
        data_fallback
    }

    fn img_source(&self, img: Node<'_>) -> (Option<Vec<u8>>, f64) {
        let src = attr(img, c"src");
        let srcset = attr(img, c"srcset");
        let lazy_src = non_empty(img, &[c"data-src", c"data-original", c"data-lazy-src"])
            .filter(|s| !s.is_empty());
        let lazy_srcset = non_empty(img, &[c"data-srcset", c"data-lazy-srcset"]);
        let sizes = attr(img, c"sizes");
        if let Some((url, density)) = self.select(lazy_srcset, sizes, None)
            && (srcset::has_width_descriptor(lazy_srcset) || lazy_src.is_none())
        {
            return (Some(url), density);
        }
        if let Some(lazy_src) = lazy_src {
            return (Some(lazy_src.to_vec()), 1.0);
        }
        let placeholder = src.is_some_and(is_data);
        if let Some((url, density)) = self.select(srcset, sizes, src.filter(|_| !placeholder)) {
            return (Some(url), density);
        }
        (src.filter(|s| !s.is_empty()).map(<[u8]>::to_vec), 1.0)
    }

    pub fn choose<'a>(&self, node: Node<'a>) -> Chosen<'a> {
        let mut n = node;
        let mut img = None;
        if is_named(Some(n), b"img") && is_named(n.parent(), b"picture") {
            img = Some(node);
            n = n.parent().unwrap_or(n);
        }
        if n.name().map(CStr::to_bytes) != Some(b"picture") {
            let (url, density) = self.img_source(n);
            return Chosen {
                url,
                density,
                img: node,
            };
        }
        let img = img.or_else(|| children(n).find(|c| is_named(Some(*c), b"img")));
        let source = self.picture_source(n, img);
        if let Some((url, density)) = source.as_ref().filter(|(url, _)| !is_data(url)) {
            return Chosen {
                url: Some(url.clone()),
                density: *density,
                img: img.unwrap_or(n),
            };
        }
        let (url, density) = match img {
            Some(img) => self.img_source(img),
            None => (None, 1.0),
        };
        let (url, density) = match (url, source) {
            (None, Some((url, density))) => (Some(url), density),
            (url, _) => (url, density),
        };
        Chosen {
            url,
            density,
            img: img.unwrap_or(n),
        }
    }
}
