//! Southstar — choosing an <img>'s URL and density, with the device pixel ratio, sizes, media query and image type answers the choice needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;

use crate::image_source::{Chosen, Environment};

pub fn choose_img(node: Node<'_>) -> Chosen<'_> {
    let resolve_sizes =
        |sizes: Option<&[u8]>| southstar_css::sizes_resolve(sizes.unwrap_or_default());
    let media_matches = |query: &[u8]| southstar_css_media::query_matches(query);
    let supports_type = |mime: &[u8]| southstar_css::image_type_supported(mime);
    Environment {
        device_pixel_ratio: southstar_css_media::device_pixel_ratio(),
        resolve_sizes: &resolve_sizes,
        media_matches: &media_matches,
        supports_type: &supports_type,
    }
    .choose(node)
}

pub fn img_chosen_url(node: Node<'_>) -> Option<Vec<u8>> {
    choose_img(node).url
}

pub fn img_chosen_density(node: Option<Node<'_>>) -> f64 {
    node.map(|node| choose_img(node).density)
        .filter(|&density| density > 0.0)
        .unwrap_or(1.0)
}
