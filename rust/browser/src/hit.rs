//! Southstar — what is under the pointer: links, the cursor, media elements and their URLs, find-in-page and click-to-play video.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::{Kind, Node, ancestors_and_self};
use southstar_glib::GStr;
use southstar_layout::BoxRef;

use crate::callbacks;
use crate::ffi::{self, NsBrowser, VideoRef};

const LINK_PROBE_RADIUS: c_int = 6;
const MEDIA_SRC_ATTR: &CStr = c"data-nd-media-src";
const MEDIA_STREAM_ATTR: &CStr = c"data-nd-media-stream";
const KNOWN_CURSORS: [&[u8]; 35] = [
    b"default",
    b"none",
    b"context-menu",
    b"help",
    b"pointer",
    b"progress",
    b"wait",
    b"cell",
    b"crosshair",
    b"text",
    b"vertical-text",
    b"alias",
    b"copy",
    b"move",
    b"no-drop",
    b"not-allowed",
    b"grab",
    b"grabbing",
    b"all-scroll",
    b"col-resize",
    b"row-resize",
    b"n-resize",
    b"e-resize",
    b"s-resize",
    b"w-resize",
    b"ne-resize",
    b"nw-resize",
    b"se-resize",
    b"sw-resize",
    b"ew-resize",
    b"ns-resize",
    b"nesw-resize",
    b"nwse-resize",
    b"zoom-in",
    b"zoom-out",
];

pub fn hit_node(b: &NsBrowser, x: c_int, y: c_int) -> Option<Node<'_>> {
    ffi::hit_node(b.layout()?, f64::from(x), f64::from(y))
}

pub fn is_hyperlink(n: Node<'_>) -> bool {
    ffi::is_named(n, c"a") || ffi::is_named(n, c"area")
}

pub fn link_near(b: &NsBrowser, x: c_int, y: c_int, probes: usize) -> Option<GStr> {
    let layout = b.layout()?;
    let r = LINK_PROBE_RADIUS;
    let offsets = [
        (0, 0),
        (0, -r),
        (0, r),
        (-r, 0),
        (r, 0),
        (-r, -r),
        (r, -r),
        (-r, r),
        (r, r),
    ];
    for &(dx, dy) in offsets.iter().take(probes) {
        let (px, py) = (x + dx, y + dy);
        let mut href = ffi::hit_link(layout, f64::from(px), f64::from(py));
        if href.is_none_or(CStr::is_empty) {
            let mut cur = hit_node(b, px, py);
            while let Some(a) = cur {
                if href.is_some_and(|h| !h.is_empty()) {
                    break;
                }
                if is_hyperlink(a) {
                    href = a.attr(c"href");
                }
                cur = a.parent();
            }
        }
        if let Some(href) = href.filter(|h| !h.is_empty()) {
            return callbacks::resolve_navigation(b, href);
        }
    }
    None
}

fn cursor_keyword(keyword: &CStr) -> Option<&'static [u8]> {
    let mut found = None;
    for token in keyword
        .to_bytes()
        .split(|&c| matches!(c, b',' | b' ' | b'\t'))
    {
        if token.is_empty() {
            continue;
        }
        for known in KNOWN_CURSORS {
            if token.eq_ignore_ascii_case(known) {
                found = Some(known);
            }
        }
    }
    found
}

pub fn cursor_at(b: &NsBrowser, x: c_int, y: c_int) -> Option<&'static [u8]> {
    let layout = b.layout()?;
    if b.styles_raw().is_null() {
        return None;
    }
    let node = hit_node(b, x, y);
    let form_node = ffi::hit_form_dom(layout, f64::from(x), f64::from(y));
    let style = node.and_then(|n| {
        ancestors_and_self(n)
            .map(|a| b.style_for(a))
            .find(|s| !s.is_null())
    })?;
    if let Some(kw) = ffi::style_keyword_raw(style, ffi::css_prop_id(c"cursor"))
        && let Some(found) = cursor_keyword(kw)
    {
        return Some(found);
    }
    if ffi::hit_link(layout, f64::from(x), f64::from(y)).is_some() {
        return None;
    }
    if node.is_some_and(|n| {
        ancestors_and_self(n).any(|a| is_hyperlink(a) && a.attr(c"href").is_some())
    }) {
        return None;
    }
    if let Some(form_node) = form_node {
        return ffi::is_text_input(form_node).then_some(b"text");
    }
    if node.is_some_and(|n| ancestors_and_self(n).any(ffi::is_contenteditable_host)) {
        return Some(b"text");
    }
    ffi::selection_text_at(layout, f64::from(x), f64::from(y)).then_some(b"text")
}

fn has_media_attr(n: Node<'_>) -> bool {
    n.kind() == Kind::Element
        && (n.attr(MEDIA_SRC_ATTR).is_some() || n.attr(MEDIA_STREAM_ATTR).is_some())
}

fn media_box(hit: Option<BoxRef<'_>>) -> Option<BoxRef<'_>> {
    let mut cur = hit;
    while let Some(b) = cur {
        let has_media_url = b
            .media()
            .is_some_and(|m| m.video_src().is_some() || m.video_audio_src().is_some());
        if let Some(dom) = ffi::box_dom(b)
            && (ffi::is_named(dom, c"video")
                || ffi::is_named(dom, c"audio")
                || has_media_url
                || has_media_attr(dom))
        {
            return Some(b);
        }
        cur = b.parent();
    }
    None
}

pub struct MediaHit {
    pub url: GStr,
    pub is_video: bool,
    pub stream: bool,
}

pub fn media_at(b: &NsBrowser, x: c_int, y: c_int) -> Option<MediaHit> {
    let layout = b.layout()?;
    let media = media_box(ffi::hit_test(layout, f64::from(x), f64::from(y)))?;
    let dom = ffi::box_dom(media)?;
    if let Some(video) = VideoRef::of(media) {
        if video.is_camera() {
            return None;
        }
        if video.has_player() {
            video.toggle(b.videos(), ffi::monotonic_us());
            return None;
        }
    }
    let element = dom.kind() == Kind::Element;
    if element
        && dom
            .attr(MEDIA_STREAM_ATTR)
            .is_some_and(|k| k.to_bytes() == b"camera")
    {
        return None;
    }
    let is_video = ffi::is_named(dom, c"video")
        || media.media().is_some_and(|m| m.video_src().is_some())
        || has_media_attr(dom);
    let force_stream = element && dom.attr(MEDIA_STREAM_ATTR).is_some();
    let mut msrc = media.media().and_then(|m| {
        if is_video {
            m.video_src().or_else(|| m.video_audio_src())
        } else {
            m.video_audio_src()
        }
    });
    if msrc.is_none_or(|s| s.is_empty()) && element {
        msrc = dom.attr(MEDIA_SRC_ATTR);
    }
    let mut abs = match msrc {
        Some(src) if !force_stream => ffi::url_resolve(b.base_url.get(), src),
        _ => None,
    };
    let stream = force_stream
        || abs.as_deref().is_none_or(|a| {
            a.to_bytes().starts_with(b"blob:") || a.to_bytes().starts_with(b"data:")
        });
    if !stream && is_video && abs.as_deref().is_some_and(ffi::video_url_is_inline) {
        return None;
    }
    if stream {
        abs = b.base_url.get().and_then(ffi::dup_text);
    }
    let abs = abs?;
    if abs.to_bytes().starts_with(b"file://")
        && !b
            .base_url
            .get()
            .is_some_and(|base| base.to_bytes().starts_with(b"file://"))
    {
        return None;
    }
    Some(MediaHit {
        url: abs,
        is_video,
        stream,
    })
}

pub struct Found {
    pub total: c_int,
    pub current: c_int,
    pub y: c_int,
}

pub fn find(
    b: &NsBrowser,
    query: Option<&CStr>,
    case_sensitive: bool,
    direction: c_int,
    from_y: c_int,
) -> Option<Found> {
    let layout = b.layout()?;
    let mut found = Found {
        total: 0,
        current: 0,
        y: 0,
    };
    let Some(query) = query.filter(|q| !q.is_empty()) else {
        b.search_query.clear();
        b.search_active.set(None);
        b.search_case.set(case_sensitive);
        return Some(found);
    };
    if b.search_query.get() != Some(query) || b.search_case.get() != case_sensitive {
        b.search_query.set(Some(query));
        b.search_active.set(None);
    }
    b.search_case.set(case_sensitive);
    let total = ffi::count_matches(layout, query, case_sensitive);
    found.total = total as c_int;
    if total == 0 {
        b.search_active.set(None);
        return Some(found);
    }
    let cur_y = b.search_active_y().unwrap_or(f64::from(from_y));
    let first = |y: f64, above: bool| ffi::first_match(layout, query, y, case_sensitive, above);
    let target = match direction {
        2 => first(cur_y, true).or_else(|| first(f64::MAX, true)),
        1 => first(cur_y + 2.0, false).or_else(|| first(-1.0, false)),
        _ => first(f64::from(from_y) - 1.0, false).or_else(|| first(-1.0, false)),
    };
    b.search_active.set(target);
    if let Some(target) = target {
        found.y = target.y() as c_int;
        found.current = ffi::match_ordinal(layout, query, target, case_sensitive) as c_int;
    }
    Some(found)
}

pub fn video_click_toggle(b: &NsBrowser, x: c_int, y: c_int) {
    let (Some(layout), Some(videos)) = (b.layout(), b.videos()) else {
        return;
    };
    let mut cur = ffi::hit_test(layout, f64::from(x), f64::from(y));
    while let Some(bx) = cur {
        if let Some(video) = VideoRef::of(bx) {
            if video.is_camera() || !video.has_player() {
                return;
            }
            if let (Some(js), Some(dom)) = (b.js(), ffi::box_dom(bx))
                && js.node_has_click_handler(dom)
            {
                return;
            }
            video.toggle(Some(videos), ffi::monotonic_us());
            b.dirty.set(true);
            return;
        }
        cur = bx.parent();
    }
}
