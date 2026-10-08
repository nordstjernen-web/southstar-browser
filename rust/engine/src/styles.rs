//! Southstar — collecting a document's style sheets (inline runs, links, imports, frames and adopted sheets), the cascade and relayout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use southstar_dom::{Kind, Node, ancestors, children};

use crate::fetch::{self, CssFetch};
use crate::ffi::{self, CssCache, SheetList, Stylesheet};

const IMPORT_MAX_DEPTH: u32 = 8;
const MAX_DEPTH: u32 = 512;
const NODE_QUIRKS: u32 = 1 << 5;
const NODE_SHEET_DISABLED: u32 = 1 << 20;
const MEDIA_MEMO_MAX: usize = 256;

static RELAYOUT_COUNT: AtomicU64 = AtomicU64::new(0);
static RELAYOUT_US: AtomicI64 = AtomicI64::new(0);
static FRAME_VIEWPORTS: Mutex<Option<HashMap<usize, (f64, f64)>>> = Mutex::new(None);

thread_local! {
    static MEDIA_MEMO: RefCell<HashMap<usize, (ffi::Bytes, bool)>> = RefCell::new(HashMap::new());
}

pub fn add_relayout(elapsed_us: i64) {
    RELAYOUT_COUNT.fetch_add(1, Ordering::Relaxed);
    RELAYOUT_US.fetch_add(elapsed_us, Ordering::Relaxed);
}

pub fn layout_perf() -> (u64, f64) {
    (
        RELAYOUT_COUNT.load(Ordering::Relaxed),
        RELAYOUT_US.load(Ordering::Relaxed) as f64 / 1000.0,
    )
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

fn starts_with_ci(s: &[u8], at: usize, word: &[u8]) -> bool {
    s.len() >= at + word.len() && s[at..at + word.len()].eq_ignore_ascii_case(word)
}

pub fn css_has_viewport_media(css: &[u8]) -> bool {
    let mut from = 0;
    while let Some(p) = find(css, b"@media", from) {
        let Some(end) = css[p..].iter().position(|&b| b == b'{').map(|i| i + p) else {
            break;
        };
        for q in p..end {
            if [&b"width"[..], b"height", b"aspect-ratio", b"orientation"]
                .iter()
                .any(|w| starts_with_ci(css, q, w))
            {
                return true;
            }
        }
        from = p + 6;
    }
    false
}

fn bytes_have_viewport_media(bytes: &ffi::Bytes) -> bool {
    MEDIA_MEMO.with_borrow_mut(|memo| {
        let key = bytes.addr();
        if let Some((_, seen)) = memo.get(&key) {
            return *seen;
        }
        if memo.len() >= MEDIA_MEMO_MAX {
            memo.clear();
        }
        let data = bytes.data();
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        let seen = css_has_viewport_media(&data[..end]);
        memo.insert(key, (bytes.clone_ref(), seen));
        seen
    })
}

fn style_attr_dimension(frame: Node, prop: &[u8]) -> Option<f64> {
    let style = frame.attr(c"style")?.to_bytes();
    let mut from = 0;
    while let Some(p) = find(style, prop, from) {
        from = p + prop.len();
        if p > 0 && (style[p - 1].is_ascii_alphanumeric() || style[p - 1] == b'-') {
            continue;
        }
        let mut q = p + prop.len();
        while q < style.len() && (style[q] == b' ' || style[q] == b'\t') {
            q += 1;
        }
        if style.get(q) != Some(&b':') {
            continue;
        }
        q += 1;
        while q < style.len() && (style[q] == b' ' || style[q] == b'\t') {
            q += 1;
        }
        let (v, consumed) = southstar_glib::ascii_strtod_prefix(&style[q..]);
        let end = q + consumed;
        let unit_ok =
            starts_with_ci(style, end, b"px") || matches!(style.get(end), None | Some(b';' | b' '));
        if consumed > 0 && v >= 0.0 && unit_ok {
            return Some(v);
        }
    }
    None
}

fn attr_dimension(frame: Node, attr: &CStr) -> Option<f64> {
    let av = frame.attr(attr).filter(|a| !a.is_empty())?.to_bytes();
    let (v, consumed) = southstar_glib::ascii_strtod_prefix(av);
    (consumed > 0 && (v.is_nan() || v >= 0.0)).then_some(v)
}

fn frame_dimensions(frame: Node) -> (Option<f64>, Option<f64>) {
    let mut w = style_attr_dimension(frame, b"width");
    let mut h = style_attr_dimension(frame, b"height");
    if w.is_none() || h.is_none() {
        if let Some((lw, lh)) = ffi::layout_frame_viewport(frame) {
            w = w.or(Some(lw));
            h = h.or(Some(lh));
        }
    }
    (w, h)
}

fn frame_viewport_px(frame: Node) -> (f64, f64) {
    let (w, h) = frame_dimensions(frame);
    (
        w.or_else(|| attr_dimension(frame, c"width"))
            .unwrap_or(300.0),
        h.or_else(|| attr_dimension(frame, c"height"))
            .unwrap_or(150.0),
    )
}

pub fn frame_viewport_measured(frame: Node) -> (f64, f64) {
    let (w, h) = frame_dimensions(frame);
    let w = w.or_else(|| attr_dimension(frame, c"width"));
    let h = h.or_else(|| attr_dimension(frame, c"height"));
    match (w, h) {
        (Some(w), Some(h)) => (w, h),
        _ => (0.0, 0.0),
    }
}

fn frame_viewport_record(frame: Node, w: f64, h: f64) {
    if let Ok(mut guard) = FRAME_VIEWPORTS.lock() {
        if let Some(map) = guard.as_mut() {
            map.insert(frame.as_ptr() as usize, (w, h));
        }
    }
}

fn frame_viewports_reset() {
    if let Ok(mut guard) = FRAME_VIEWPORTS.lock() {
        guard.get_or_insert_with(HashMap::new).clear();
    }
}

pub fn frame_viewports_disagree_with_layout() -> bool {
    let recorded: Vec<(usize, (f64, f64))> = match FRAME_VIEWPORTS.lock() {
        Ok(guard) => guard
            .as_ref()
            .map_or(Vec::new(), |m| m.iter().map(|(k, v)| (*k, *v)).collect()),
        Err(_) => return false,
    };
    recorded.into_iter().any(
        |(frame, (w, h))| match ffi::layout_frame_viewport_raw(frame) {
            Some((lw, lh)) => (w - lw).abs() > 0.01 || (h - lh).abs() > 0.01,
            None => false,
        },
    )
}

fn sheet_type_is_css(kind: Option<&CStr>) -> bool {
    let Some(kind) = kind.map(CStr::to_bytes).filter(|k| !k.is_empty()) else {
        return true;
    };
    let mut n = kind.iter().position(|&b| b == b';').unwrap_or(kind.len());
    while n > 0 && is_space(kind[n - 1]) {
        n -= 1;
    }
    n == 8 && kind[..8].eq_ignore_ascii_case(b"text/css")
}

fn style_sheet_enabled(style: Node) -> bool {
    style.flags() & NODE_SHEET_DISABLED == 0 && sheet_type_is_css(style.attr(c"type"))
}

fn link_sheet_enabled(link: Node) -> bool {
    link.attr(c"disabled").is_none() && sheet_type_is_css(link.attr(c"type"))
}

fn node_in_head(n: Node) -> bool {
    for p in ancestors(n) {
        if !p.is_element() {
            return false;
        }
        if ffi::is_named(p, c"head") {
            return true;
        }
    }
    false
}

fn is_frame(n: Node) -> bool {
    ffi::is_named(n, c"iframe") || ffi::is_named(n, c"frame") || ffi::is_named(n, c"object")
}

pub struct Expand<'a> {
    pub top_url: Option<&'a CStr>,
    pub cache: Option<CssCache>,
    pub render_blocking: bool,
    pub in_frame: bool,
}

fn append_expanded(
    out: &SheetList,
    sh: Stylesheet,
    base: Option<&CStr>,
    x: &Expand,
    seen: &mut HashSet<Vec<u8>>,
    depth: u32,
) {
    if depth < IMPORT_MAX_DEPTH {
        for import in sh.imports() {
            let Some(url) = import.url.filter(|u| !u.is_empty()) else {
                continue;
            };
            if import
                .media
                .is_some_and(|m| !m.is_empty() && !ffi::media_query_matches(m))
            {
                continue;
            }
            let Some(abs) = ffi::url_resolve_opt(base, url) else {
                continue;
            };
            if !seen.insert(abs.to_bytes().to_vec()) {
                continue;
            }
            let f = CssFetch {
                url: &abs,
                top_url: x.top_url,
                strict_mime: true,
                initiator: c"css",
                render_blocking: x.render_blocking,
                in_frame: x.in_frame,
            };
            if let Some(bytes) = fetch::fetch_css_bytes(&f, x.cache) {
                if let Some(child) = ffi::parse_import_cached(&abs, import.layer_name, &bytes) {
                    append_expanded(out, child, Some(&abs), x, seen, depth + 1);
                }
            }
        }
    }
    sh.resolve_urls(base);
    out.push(sh.as_ptr());
}

struct Collect<'a> {
    out: &'a SheetList,
    docs: Option<&'a SheetList>,
    doc: *const southstar_dom::NsNode,
    cache: Option<CssCache>,
    run: Vec<u8>,
    run_chunks: Vec<Vec<u8>>,
    run_base: Option<*const core::ffi::c_char>,
    top_url: Option<&'a CStr>,
    strict_css_mime: bool,
    media_seen: bool,
    frame_depth: i32,
}

fn base_ptr(base: Option<&CStr>) -> *const core::ffi::c_char {
    base.map_or(core::ptr::null(), CStr::as_ptr)
}

impl Collect<'_> {
    fn run_append(&mut self, css: &[u8], base: Option<&CStr>) {
        self.run.extend_from_slice(css);
        self.run.push(b'\n');
        self.run_chunks.push(css.to_vec());
        self.run_base = Some(base_ptr(base));
    }

    fn run_base_differs(&self, base: Option<&CStr>) -> bool {
        self.run_base
            .is_some_and(|rb| !rb.is_null() && rb != base_ptr(base))
    }

    fn drop_repeats(&mut self) {
        let n = self.run_chunks.len();
        if n < 2 {
            return;
        }
        let mut later: HashSet<&[u8]> = HashSet::new();
        let mut drop = vec![false; n];
        let mut any = false;
        for i in (0..n).rev() {
            let chunk = self.run_chunks[i].as_slice();
            if chunk.contains(&b'@') {
                continue;
            }
            if later.contains(chunk) {
                drop[i] = true;
                any = true;
            } else {
                later.insert(chunk);
            }
        }
        if any {
            let mut run = Vec::new();
            for (i, chunk) in self.run_chunks.iter().enumerate() {
                if !drop[i] {
                    run.extend_from_slice(chunk);
                    run.push(b'\n');
                }
            }
            self.run = run;
        }
    }

    fn flush(&mut self) {
        if self.run.is_empty() {
            return;
        }
        self.drop_repeats();
        self.run_chunks.clear();
        let base = self.run_base.and_then(ffi::c_str_ptr);
        if let Some(sh) = ffi::merged_styles_cached(&self.run, base) {
            let x = Expand {
                top_url: self.top_url,
                cache: self.cache,
                render_blocking: false,
                in_frame: self.frame_depth > 0,
            };
            append_expanded(self.out, sh, base, &x, &mut HashSet::new(), 0);
        }
        self.run.clear();
        self.run_base = None;
    }

    fn docs_sync(&mut self) {
        self.flush();
        if let Some(docs) = self.docs {
            while docs.len() < self.out.len() {
                docs.push(self.doc.cast_mut().cast());
            }
        }
    }

    fn frame_children<'n>(&mut self, frame: Node<'n>, base: Option<&'n CStr>, depth: u32) {
        for c in children(frame) {
            if c.kind() != Kind::Document {
                self.walk(c, base, depth + 1);
                continue;
            }
            self.docs_sync();
            let outer = self.doc;
            self.doc = c.as_ptr();
            self.walk(c, base, depth + 1);
            self.docs_sync();
            self.doc = outer;
        }
    }

    fn adopted(&mut self, root: Node, base: Option<&CStr>) {
        if !root.is_element() {
            return;
        }
        let Some(css) = ffi::shadow_adopted_css(root) else {
            return;
        };
        if css_has_viewport_media(&css) {
            self.media_seen = true;
        }
        let alone = !ffi::is_self_contained(&css);
        if alone || self.run_base_differs(base) {
            self.flush();
        }
        self.run_append(&css, base);
        if alone {
            self.flush();
        }
    }

    fn walk<'n>(&mut self, n: Node<'n>, mut base: Option<&'n CStr>, depth: u32) {
        if depth >= MAX_DEPTH || ffi::is_named(n, c"noscript") {
            return;
        }
        if is_frame(n) {
            self.flush();
            if let Some(frame_url) = n.attr(c"data-nd-frame-url").filter(|u| !u.is_empty()) {
                base = Some(frame_url);
            }
            if children(n).any(|c| c.kind() == Kind::Document) {
                let (fw, fh) = frame_viewport_px(n);
                let outer_media = self.media_seen;
                self.media_seen = false;
                ffi::media_viewport_push(fw, fh);
                self.frame_depth += 1;
                self.frame_children(n, base, depth);
                self.flush();
                self.frame_depth -= 1;
                let frame_media = self.media_seen;
                ffi::media_viewport_pop();
                self.media_seen = outer_media || frame_media;
                if frame_media {
                    frame_viewport_record(n, fw, fh);
                }
                return;
            }
        }
        if ffi::is_named(n, c"style") && style_sheet_enabled(n) {
            self.inline_style(n, base);
        } else if ffi::is_named(n, c"link") && base.is_some() {
            self.flush();
            self.link(n, base);
        }
        for c in children(n) {
            self.walk(c, base, depth + 1);
        }
        self.adopted(n, base);
    }

    fn inline_style(&mut self, n: Node, base: Option<&CStr>) {
        let Some(css) = ffi::style_element_text(n) else {
            return;
        };
        if css_has_viewport_media(&css) {
            self.media_seen = true;
        }
        if self.run_base_differs(base) {
            self.flush();
        }
        if find(&css, b"@import", 0).is_some() || !ffi::is_self_contained(&css) {
            self.flush();
            if let Some(sh) = ffi::stylesheet_from_style_element_cached(n) {
                let x = Expand {
                    top_url: self.top_url,
                    cache: self.cache,
                    render_blocking: node_in_head(n),
                    in_frame: self.frame_depth > 0,
                };
                append_expanded(self.out, sh, base, &x, &mut HashSet::new(), 0);
            }
        } else {
            self.run_append(&css, base);
        }
    }

    fn link(&mut self, n: Node, base: Option<&CStr>) {
        let rel = n.attr(c"rel").map(CStr::to_bytes);
        let Some(href) = n.attr(c"href").filter(|h| !h.is_empty()) else {
            return;
        };
        let media_ok = || {
            n.attr(c"media")
                .filter(|m| !m.is_empty())
                .is_none_or(ffi::media_query_matches)
        };
        if !(fetch::rel_is_stylesheet(rel) && link_sheet_enabled(n) && media_ok()) {
            return;
        }
        let abs = ffi::url_resolve_opt(base, href);
        let blocking = node_in_head(n);
        let Some(abs) = abs else {
            return;
        };
        let f = CssFetch {
            url: &abs,
            top_url: self.top_url,
            strict_mime: self.strict_css_mime,
            initiator: c"link",
            render_blocking: blocking,
            in_frame: self.frame_depth > 0,
        };
        let Some(bytes) = fetch::fetch_css_bytes(&f, self.cache) else {
            return;
        };
        fetch::remember_linked_css(abs.to_bytes(), bytes.data());
        if bytes_have_viewport_media(&bytes) {
            self.media_seen = true;
        }
        if let Some(sh) = ffi::parse_url_cached(&abs, bytes.data()) {
            let mut seen = HashSet::new();
            seen.insert(abs.to_bytes().to_vec());
            let x = Expand {
                top_url: self.top_url,
                cache: self.cache,
                render_blocking: blocking,
                in_frame: self.frame_depth > 0,
            };
            append_expanded(self.out, sh, Some(&abs), &x, &mut seen, 0);
        }
    }
}

pub fn collect_stylesheets(
    doc: Option<Node>,
    base: Option<&CStr>,
    out: &SheetList,
    docs: Option<&SheetList>,
    cache: Option<CssCache>,
) {
    frame_viewports_reset();
    let mut cc = Collect {
        out,
        docs,
        doc: doc.map_or(core::ptr::null(), |d| d.as_ptr()),
        cache,
        run: Vec::new(),
        run_chunks: Vec::new(),
        run_base: None,
        top_url: base,
        strict_css_mime: doc.is_some_and(|d| d.flags() & NODE_QUIRKS == 0),
        media_seen: false,
        frame_depth: 0,
    };
    if let Some(doc) = doc {
        cc.walk(doc, base, 0);
    }
    cc.docs_sync();
    ffi::style_element_cache_end();
}
