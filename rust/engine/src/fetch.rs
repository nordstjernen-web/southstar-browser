//! Southstar — stylesheet fetches with their failure markers, linked-stylesheet text, speculative preloads and wanted images.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::Mutex;

use southstar_dom::{Node, ancestors, children};
use southstar_layout::{BoxKind, BoxRef};

use crate::ffi::{self, CssCache, Dest, ImageCache, Wanted};

const LINKED_CSS_MAX: usize = 128;
const LAZY_IMAGE_MARGIN_PX: f64 = 4000.0;
const MAX_DEPTH: u32 = 512;

static LINKED_CSS: Mutex<Option<HashMap<Vec<u8>, Vec<u8>>>> = Mutex::new(None);

pub fn content_type_is_css(ct: Option<&[u8]>) -> bool {
    let Some(ct) = ct.filter(|c| !c.is_empty()) else {
        return true;
    };
    let start = ct
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(ct.len());
    let ct = &ct[start..];
    ct.len() >= 8
        && ct[..8].eq_ignore_ascii_case(b"text/css")
        && matches!(ct.get(8), None | Some(b';' | b' ' | b'\t'))
}

pub fn rel_has_token(rel: Option<&[u8]>, token: &[u8]) -> bool {
    let Some(rel) = rel else {
        return false;
    };
    !token.is_empty()
        && rel
            .split(|b| b" \t\r\n\x0c".contains(b))
            .any(|part| !part.is_empty() && part.eq_ignore_ascii_case(token))
}

pub fn rel_is_stylesheet(rel: Option<&[u8]>) -> bool {
    rel_has_token(rel, b"stylesheet") && !rel_has_token(rel, b"alternate")
}

pub fn remember_linked_css(url: &[u8], css: &[u8]) {
    if url.is_empty() {
        return;
    }
    let Ok(mut guard) = LINKED_CSS.lock() else {
        return;
    };
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() >= LINKED_CSS_MAX && !map.contains_key(url) {
        map.clear();
    }
    map.insert(url.to_vec(), css.to_vec());
}

pub fn linked_css_text(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() {
        return None;
    }
    let guard = LINKED_CSS.lock().ok()?;
    let css = guard.as_ref()?.get(url)?;
    let end = css.iter().position(|&b| b == 0).unwrap_or(css.len());
    (!css.is_empty()).then(|| css[..end].to_vec())
}

pub struct CssFetch<'a> {
    pub url: &'a CStr,
    pub top_url: Option<&'a CStr>,
    pub strict_mime: bool,
    pub initiator: &'static CStr,
    pub render_blocking: bool,
    pub in_frame: bool,
}

pub fn fetch_css_bytes(f: &CssFetch, cache: Option<CssCache>) -> Option<ffi::Bytes> {
    if f.url.is_empty() {
        return None;
    }
    let mut attempts = 0u8;
    if let Some(hit) = cache.and_then(|c| c.lookup(f.url)) {
        let data = hit.data();
        let fail_marker = data.is_empty() || (data.len() == 1 && data[0] <= 8);
        if !fail_marker {
            return Some(hit.clone_ref());
        }
        attempts = if data.len() == 1 { data[0] } else { 1 };
        if attempts >= 3 {
            return None;
        }
    }
    let start_us = ffi::monotonic_us();
    let resp = ffi::fetch_blocking_with_headers(f.url, f.top_url, Dest::Style);
    let enforce_mime = f.strict_mime || resp.as_ref().is_some_and(|r| r.nosniff());
    let mime_ok = !enforce_mime
        || resp
            .as_ref()
            .is_none_or(|r| content_type_is_css(r.content_type()));
    let body = resp
        .as_ref()
        .filter(|r| r.error().is_none() && r.status() < 400 && mime_ok)
        .and_then(|r| r.body())
        .filter(|b| !b.is_empty());
    let bytes = body.map(ffi::Bytes::new);
    match (&bytes, cache) {
        (Some(bytes), Some(cache)) => cache.insert(f.url, bytes.clone_ref()),
        (None, Some(cache)) => cache.insert(f.url, ffi::Bytes::new(&[attempts.wrapping_add(1)])),
        _ => {}
    }
    if let Some(resp) = resp {
        ffi::record_timing(f, start_us, resp);
    }
    bytes
}

pub struct Preloads {
    base: CString,
    include_images: bool,
    pub urls: Vec<(Vec<u8>, Dest)>,
    seen: HashSet<Vec<u8>>,
    pub connects: Vec<Vec<u8>>,
    connect_seen: HashSet<Vec<u8>>,
}

fn destination_for(rel: Option<&[u8]>, kind: Option<&CStr>) -> Dest {
    if rel_has_token(rel, b"modulepreload") {
        return Dest::Script;
    }
    match kind.map(CStr::to_bytes).filter(|k| !k.is_empty()) {
        Some(k) if k.eq_ignore_ascii_case(b"script") => Dest::Script,
        Some(k) if k.eq_ignore_ascii_case(b"style") => Dest::Style,
        _ => Dest::Default,
    }
}

impl Preloads {
    pub fn new(base: &CStr, include_images: bool) -> Preloads {
        Preloads {
            base: base.to_owned(),
            include_images,
            urls: Vec::new(),
            seen: HashSet::new(),
            connects: Vec::new(),
            connect_seen: HashSet::new(),
        }
    }

    fn add(&mut self, reference: Option<&CStr>, dest: Dest) {
        let Some(reference) =
            reference.filter(|r| !r.is_empty() && !r.to_bytes().starts_with(b"data:"))
        else {
            return;
        };
        let Some(abs) = ffi::url_resolve(&self.base, reference) else {
            return;
        };
        let mut slot = format!("{}\x1f", dest as i32).into_bytes();
        slot.extend_from_slice(&abs);
        if !ffi::url_is_http_or_https(&abs) || self.seen.contains(&slot) {
            return;
        }
        self.seen.insert(slot);
        self.urls.push((abs, dest));
    }

    pub fn collect(&mut self, n: Node, depth: u32) {
        if depth >= MAX_DEPTH {
            return;
        }
        if self.include_images && ffi::is_named(n, c"img") {
            let lazy = n
                .attr(c"loading")
                .is_some_and(|l| l.to_bytes().eq_ignore_ascii_case(b"lazy"));
            if !lazy {
                self.add(n.attr(c"src"), Dest::Default);
            }
        } else if ffi::is_named(n, c"script") {
            self.add(n.attr(c"src"), Dest::Script);
        } else if ffi::is_named(n, c"link") {
            let rel = n.attr(c"rel").map(CStr::to_bytes);
            if rel.is_some()
                && (rel_has_token(rel, b"preconnect") || rel_has_token(rel, b"dns-prefetch"))
            {
                self.add_connect(n.attr(c"href"));
            } else if rel.is_some() && rel_has_token(rel, b"stylesheet") {
                self.add(n.attr(c"href"), Dest::Style);
            } else if rel.is_some() && rel_has_token(rel, b"prefetch") {
                self.add(n.attr(c"href"), Dest::Default);
            } else if rel.is_some()
                && (rel_has_token(rel, b"preload") || rel_has_token(rel, b"modulepreload"))
            {
                self.add(n.attr(c"href"), destination_for(rel, n.attr(c"as")));
            }
        }
        for c in children(n) {
            self.collect(c, depth + 1);
        }
    }

    fn add_connect(&mut self, href: Option<&CStr>) {
        let abs = href
            .filter(|h| !h.is_empty())
            .and_then(|h| ffi::url_resolve(&self.base, h));
        let Some(origin) = abs.and_then(|abs| ffi::url_origin(&abs)) else {
            return;
        };
        if ffi::url_is_http_or_https(&origin) && !self.connect_seen.contains(&origin) {
            self.connect_seen.insert(origin.clone());
            self.connects.push(origin);
        }
    }
}

pub fn speculative_preload(doc: Node, base: &CStr, include_images: bool) {
    if !ffi::url_is_http_or_https(base.to_bytes()) {
        return;
    }
    let mut preloads = Preloads::new(base, include_images);
    preloads.collect(doc, 0);
    ffi::preload_clear();
    for origin in &preloads.connects {
        ffi::preconnect(origin);
    }
    for (url, dest) in &preloads.urls {
        ffi::preload_request(url, base, *dest);
    }
}

fn frame_base<'a>(n: Option<Node<'a>>, dflt: &'a CStr) -> &'a CStr {
    let Some(n) = n else {
        return dflt;
    };
    for p in ancestors(n) {
        if ffi::is_named(p, c"iframe") || ffi::is_named(p, c"frame") || ffi::is_named(p, c"object")
        {
            if let Some(url) = p.attr(c"data-nd-frame-url").filter(|u| !u.is_empty()) {
                return url;
            }
        }
    }
    dflt
}

fn image_is_lazy(b: BoxRef) -> bool {
    b.kind() == BoxKind::Image
        && crate::ffi::box_dom(b)
            .and_then(|d| d.attr(c"loading"))
            .is_some_and(|l| l.to_bytes().eq_ignore_ascii_case(b"lazy"))
}

pub struct WantedImages {
    pub wanted: Wanted,
    pub deferred_any: bool,
}

pub fn collect_wanted_images(
    root: BoxRef,
    base: &CStr,
    cache: ImageCache,
    scroll_y: f64,
    viewport_h: f64,
) -> WantedImages {
    let wanted = Wanted::new();
    let mut deferred_any = false;
    let lazy_limit = if viewport_h > 0.0 {
        scroll_y + viewport_h + LAZY_IMAGE_MARGIN_PX
    } else {
        f64::MAX
    };
    for b in ffi::collect_images(root) {
        let Some(media) = b.media() else {
            continue;
        };
        if b.y() > lazy_limit && image_is_lazy(b) {
            deferred_any = true;
            continue;
        }
        let mut srcs = Vec::new();
        if let Some(src) = media.image_src() {
            srcs.push(src);
        } else if let Some(layers) = media.bg_layer_srcs() {
            srcs.extend(layers.into_iter().flatten());
        } else if let Some(src) = media.bg_image_src() {
            srcs.push(src);
        }
        srcs.extend(media.marker_image_src());
        srcs.extend(media.border_image_src());
        let box_base = frame_base(crate::ffi::box_dom(b), base);
        for src in srcs {
            if src.to_bytes().starts_with(b"nd-inline-svg:") {
                continue;
            }
            let Some(abs) = ffi::url_resolve(box_base, src) else {
                continue;
            };
            let abs = CString::new(abs).unwrap_or_default();
            if cache.peek(&abs) || wanted.contains(&abs) {
                continue;
            }
            wanted.add(&abs);
        }
    }
    WantedImages {
        wanted,
        deferred_any,
    }
}
