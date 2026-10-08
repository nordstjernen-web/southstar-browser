//! Southstar — the in-process headless run: fetch, parse, script and settle a page, follow its navigations, then dump or capture it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_int};
use core::ptr;
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use southstar_dom::{Node, NsNode, ancestors_and_self};
use southstar_glib::{GHashTable, GStr};
use southstar_layout::{BoxRef, NsBox};

use crate::ffi::{
    self, Anim, Document, ImageCache, Js, NavigationTiming, PrintSetup, Relayout, Response, Video,
    VideoCache, err, out,
};
use crate::{Dump, Opts, nonempty};

static LAYOUT_DIRTY: AtomicBool = AtomicBool::new(false);
static STYLES_STALE: AtomicBool = AtomicBool::new(false);
static DOC_CHARSET: Mutex<Option<Vec<u8>>> = Mutex::new(None);

const WPT_POLL_JS: &CStr = c"(function () {    var g = globalThis;    if (g.__ns_wpt_done) return \"1\";    if (g.__ns_wpt_installed && !g.__ns_wpt_seen_harness &&        typeof g.add_completion_callback === \"function\") {        try {            g.add_completion_callback(g.__ns_wpt_oncomplete);            g.__ns_wpt_seen_harness = true;        } catch (e) {}    }    return \"0\";})()";

const WPT_HOOK: &str = concat!(include_str!("../../../data/js/wpt-hook.js"), "\0");

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn opt_cstring(bytes: &Option<Vec<u8>>) -> Option<CString> {
    bytes.as_deref().map(cstring)
}

pub fn js_log(line: &[u8]) {
    err(&[b"[js] ", line, b"\n"].concat());
}

pub fn note_mutated() {
    LAYOUT_DIRTY.store(true, Ordering::Relaxed);
}

#[derive(Default)]
pub struct NavCapture {
    pub pending_url: RefCell<Option<Vec<u8>>>,
    pub post_body: RefCell<Option<Vec<u8>>>,
    pub post_ct: RefCell<Option<Vec<u8>>>,
}

impl NavCapture {
    pub fn navigate(&self, url: &CStr) {
        if !url.is_empty() {
            *self.pending_url.borrow_mut() = Some(url.to_bytes().to_vec());
        }
    }

    pub fn set_pending(&self, url: Vec<u8>) {
        *self.pending_url.borrow_mut() = Some(url);
    }

    pub fn has_pending(&self) -> bool {
        self.pending_url.borrow().is_some()
    }

    fn clear_post(&self) {
        *self.post_body.borrow_mut() = None;
        *self.post_ct.borrow_mut() = None;
    }
}

pub fn form_submit(nav: &NavCapture, form: Node, submitter: Option<Node>) {
    let is_post = form
        .attr(c"method")
        .is_some_and(|m| m.to_bytes().eq_ignore_ascii_case(b"post"));
    let action = form
        .attr(c"action")
        .map_or(&b""[..], CStr::to_bytes)
        .to_vec();
    let root = ffi::root(form).unwrap_or(form);
    let accept = form.attr(c"accept-charset").filter(|c| !c.is_empty());
    let doc_charset = DOC_CHARSET
        .lock()
        .map(|c| c.as_deref().map(cstring))
        .unwrap_or_default();
    ffi::set_submission_charset(accept.or(doc_charset.as_deref()));
    let submitter = submitter.filter(|s| s.as_ptr() != form.as_ptr());
    let query = ffi::collect_inputs(form, root, submitter);
    ffi::set_submission_charset(None);
    nav.clear_post();
    if is_post {
        nav.set_pending(action);
        *nav.post_body.borrow_mut() = Some(query);
        *nav.post_ct.borrow_mut() = Some(b"application/x-www-form-urlencoded".to_vec());
        return;
    }
    let url = if query.is_empty() {
        action
    } else {
        let sep: &[u8] = if action.contains(&b'?') { b"&" } else { b"?" };
        [action.as_slice(), sep, query.as_slice()].concat()
    };
    nav.set_pending(url);
}

pub struct Ctx {
    pub doc: *mut NsNode,
    pub js: Option<Js>,
    pub base: Option<CString>,
    pub vw: c_int,
    pub vh: f64,
    pub image_cache: Option<ImageCache>,
    pub video_cache: Option<VideoCache>,
    pub anim: Option<Anim>,
    pub css_cache: *mut GHashTable,
    pub styles: Cell<*mut GHashTable>,
    pub layout: Cell<*mut NsBox>,
    pub focused: Cell<*const NsNode>,
    pub caret: Cell<usize>,
    pub anchor: Cell<usize>,
    relaying: Cell<bool>,
}

impl Ctx {
    pub fn doc(&self) -> Option<Node<'_>> {
        ffi::node_from_ptr(self.doc)
    }

    pub fn layout(&self) -> Option<BoxRef<'_>> {
        ffi::box_from_ptr(self.layout.get())
    }

    pub fn focused(&self) -> Option<Node<'_>> {
        ffi::node_from_ptr(self.focused.get())
    }

    pub fn set_focused(&self, n: Option<Node>) {
        self.focused.set(Node::ptr_or_null(n));
    }

    fn focus_for_layout(&self) -> *const NsNode {
        let from_js = self.js.and_then(|js| js.focused_node());
        Node::ptr_or_null(from_js.or_else(|| self.focused()))
    }
}

pub fn relayout(c: &Ctx) {
    if c.relaying.get() {
        LAYOUT_DIRTY.store(true, Ordering::Relaxed);
        return;
    }
    c.relaying.set(true);
    if std::env::var_os("NS_ANIM_DEBUG").is_some() {
        err(b"[anim] headless_relayout\n");
    }
    if let (Some(js), false) = (c.js, c.layout.get().is_null()) {
        js.set_layout_root(ptr::null());
    }
    if !c.layout.get().is_null() {
        ffi::free_layout(c.layout.replace(ptr::null_mut()));
    }
    if let (Some(js), false) = (c.js, c.styles.get().is_null()) {
        js.set_style_table(ptr::null_mut());
    }
    if !c.styles.get().is_null() {
        ffi::destroy_table(c.styles.replace(ptr::null_mut()));
    }
    let args = Relayout {
        doc: c.doc,
        base: c.base.as_deref(),
        vw: c.vw,
        vh: c.vh,
        images: ffi::image_cache_raw(c.image_cache),
        anim: ffi::anim_raw(c.anim),
        js: ffi::js_raw(c.js),
        css_cache: c.css_cache,
        focused: c.focus_for_layout(),
        caret: c.caret.get(),
        anchor: c.anchor.get(),
    };
    c.styles.set(ffi::relayout(&args, c.layout.as_ptr()));
    c.relaying.set(false);
}

pub fn flush_layout(c: &Ctx) {
    let Some(js) = c.js else {
        return;
    };
    let mutated = ffi::consume_mutated(Some(js));
    let dirty = c.layout.get().is_null()
        || mutated
        || LAYOUT_DIRTY.load(Ordering::Relaxed)
        || STYLES_STALE.load(Ordering::Relaxed);
    if !dirty {
        return;
    }
    LAYOUT_DIRTY.store(false, Ordering::Relaxed);
    STYLES_STALE.store(false, Ordering::Relaxed);
    relayout(c);
}

pub struct Settle<'a> {
    ctx: &'a Ctx,
    last_flush_us: Cell<i64>,
    pending_mutation: Cell<bool>,
}

impl<'a> Settle<'a> {
    pub fn new(ctx: &'a Ctx) -> Settle<'a> {
        Settle {
            ctx,
            last_flush_us: Cell::new(ffi::monotonic_us()),
            pending_mutation: Cell::new(false),
        }
    }

    pub fn tick(&self) {
        let fc = self.ctx;
        let now = ffi::monotonic_us();
        if let Some(cache) = fc.image_cache {
            cache.tick(now);
        }
        if let Some(cache) = fc.video_cache {
            if !fc.layout.get().is_null() {
                cache.discover(fc.layout.get(), fc.doc, now);
            }
            cache.tick(now);
        }
        if let Some(anim) = fc.anim.filter(|a| a.tick(now)) {
            STYLES_STALE.store(true, Ordering::Relaxed);
            if anim.needs_layout() {
                LAYOUT_DIRTY.store(true, Ordering::Relaxed);
            }
        }
        if let (Some(anim), Some(js)) = (fc.anim, fc.js) {
            js.dispatch_anim_events(anim);
        }
        if let Some(js) = fc.js {
            js.run_animation_frame();
        }
        if fc.js.is_some() && ffi::consume_mutated(fc.js) {
            self.pending_mutation.set(true);
            STYLES_STALE.store(true, Ordering::Relaxed);
        }
        if LAYOUT_DIRTY.load(Ordering::Relaxed) {
            self.pending_mutation.set(true);
        }
        if self.pending_mutation.get() && now - self.last_flush_us.get() >= 200_000 {
            LAYOUT_DIRTY.store(false, Ordering::Relaxed);
            STYLES_STALE.store(false, Ordering::Relaxed);
            relayout(fc);
            self.pending_mutation.set(false);
            self.last_flush_us.set(ffi::monotonic_us());
        }
    }
}

pub fn settle(ms: c_int, fc: &Ctx) {
    if ms <= 0 {
        return;
    }
    let state = Settle::new(fc);
    ffi::run_loop_for(ms, &state);
}

fn wpt_results_ready(js: Js) -> bool {
    ffi::eval_source(Some(js), WPT_POLL_JS, c"wpt-poll").is_some_and(|r| r.to_bytes() == b"1")
}

fn wpt_eval(js: Js, src: &CStr) -> Option<GStr> {
    let r = ffi::eval_source(Some(js), src, c"wpt-report");
    ffi::consume_mutated(Some(js));
    r
}

pub struct WptWait<'a> {
    pub settle: Settle<'a>,
    pub js: Js,
}

impl WptWait<'_> {
    pub fn ready(&self) -> bool {
        wpt_results_ready(self.js)
    }
}

fn wpt_finish(fc: &Ctx, o: &Opts) -> c_int {
    let Some(js) = fc.js else {
        return 2;
    };
    let timeout_ms = if o.wpt_timeout_ms > 0 {
        o.wpt_timeout_ms
    } else {
        15000
    };
    let mut done = wpt_results_ready(js);
    if !done {
        let wait = WptWait {
            settle: Settle::new(fc),
            js,
        };
        done = ffi::run_wpt_wait(timeout_ms, &wait);
    }
    if !done {
        let seen = wpt_eval(js, c"globalThis.__ns_wpt_seen_harness ? \"1\" : \"0\"");
        let why: &str = if seen.as_ref().is_some_and(|s| s.to_bytes() == b"1") {
            "tests did not complete before the timeout"
        } else {
            "testharness.js never registered"
        };
        drop(seen);
        out(format!("WPT HARNESS TIMEOUT | {why}\n").as_bytes());
        out(b"WPT SUMMARY total=0 pass=0 fail=0 timeout=0 notrun=0 precondition_failed=0\n");
        out(format!(
            "WPT JSON {{\"harness\":\"TIMEOUT\",\"message\":\"{why}\",\"subtests\":[]}}\n"
        )
        .as_bytes());
        ffi::flush();
        return 2;
    }
    let report = wpt_eval(js, c"globalThis.__ns_wpt_report || \"\"");
    let json = wpt_eval(js, c"globalThis.__ns_wpt_json || \"{}\"");
    let fails = wpt_eval(js, c"String(globalThis.__ns_wpt_failures || 0)");
    if let Some(report) = &report {
        out(report.to_bytes());
    }
    let json = json
        .as_ref()
        .map(|j| j.to_bytes())
        .filter(|j| !j.is_empty())
        .unwrap_or(b"{}");
    out(&[b"WPT JSON ", json, b"\n"].concat());
    ffi::flush();
    match fails {
        Some(f) if ffi::c_atoi(&f) <= 0 => 0,
        _ => 1,
    }
}

fn reveal_fragment(doc: Node, frag: &CStr) {
    let Some(target) = ffi::fragment_target(doc, frag) else {
        return;
    };
    enum Reveal {
        Hidden,
        Details,
    }
    let mut items = Vec::new();
    for cur in ancestors_and_self(target) {
        if ffi::hidden_until_found(cur) {
            items.push((cur, Reveal::Hidden));
        }
        if let Some(parent) = cur.parent() {
            if ffi::details_fragment_needs_open(parent, cur) {
                items.push((parent, Reveal::Details));
            }
        }
        if cur.as_ptr() == doc.as_ptr() {
            break;
        }
    }
    for (el, kind) in items {
        if ffi::root(el).map(|r| r.as_ptr()) != Some(doc.as_ptr()) {
            break;
        }
        match kind {
            Reveal::Hidden => {
                if ffi::hidden_until_found(el) {
                    ffi::remove_attr(el, c"hidden");
                }
            }
            Reveal::Details => {
                if el.attr(c"open").is_none() {
                    ffi::set_attr(el, c"open", c"");
                }
            }
        }
    }
}

fn is_inline_video(src: &[u8]) -> bool {
    let n = src
        .iter()
        .position(|b| b"?#".contains(b))
        .unwrap_or(src.len());
    let head = &src[..n];
    let ends = |ext: &[u8]| {
        head.len() >= ext.len() && head[head.len() - ext.len()..].eq_ignore_ascii_case(ext)
    };
    ends(b".mpg") || ends(b".m1v") || ends(b".mpeg") || (cfg!(feature = "libav") && ends(b".webm"))
}

fn fetch_videos_into_layout(fc: &Ctx, base: Option<&CStr>) {
    let Some(base) = base else {
        return;
    };
    for idx in 0.. {
        let Some(layout) = fc.layout() else {
            return;
        };
        let vids = ffi::collect_videos(layout);
        let Some(&b) = vids.get(idx) else {
            return;
        };
        let (want_src, want_poster) = match ffi::video_sources(b) {
            Some((src, poster)) => (
                src.and_then(|s| ffi::url_resolve(base, s)),
                poster.and_then(|p| ffi::url_resolve(base, p)),
            ),
            None => (None, None),
        };
        drop(vids);

        let mut made = None;
        if let Some(src) = want_src.as_deref().filter(|s| is_inline_video(s)) {
            let src = cstring(src);
            if let Some(resp) = ffi::fetch(&src, Some(base)) {
                if let (None, Some(body)) =
                    (&resp.error, resp.body.as_deref().filter(|b| !b.is_empty()))
                {
                    made = Video::from_player(&src, body);
                }
            }
        }
        if made.is_none() {
            if let Some(poster) = want_poster.as_deref() {
                let poster = cstring(poster);
                if let Some(resp) = ffi::fetch(&poster, Some(base)) {
                    if let (None, Some(body)) =
                        (&resp.error, resp.body.as_deref().filter(|b| !b.is_empty()))
                    {
                        made = Video::from_poster(&poster, body);
                    }
                }
            }
        }

        if let Some(video) = made {
            let target = fc
                .layout()
                .and_then(|l| ffi::collect_videos(l).get(idx).copied());
            let unattached = match target {
                Some(now) => video.attach(now).err(),
                None => Some(video),
            };
            if let Some(video) = unattached {
                video.discard();
            }
        }
    }
}

fn write_capture(root: *const NsBox, path: Option<&CStr>, kind: Dump, setup: &PrintSetup) -> c_int {
    match kind {
        Dump::Print => ffi::write_pdf_paged(root, path, setup),
        Dump::Pdf => ffi::write_pdf(root, path),
        _ => ffi::write_png(root, path),
    }
}

struct Page {
    final_url: Option<Vec<u8>>,
    content_type: Option<Vec<u8>>,
    content_language: Option<Vec<u8>>,
    body: Option<Vec<u8>>,
    timing: Option<ffi::Timing>,
}

const HTML_UTF8: &[u8] = b"text/html; charset=utf-8";

fn is_capture(d: Dump) -> bool {
    matches!(d, Dump::Png | Dump::Pdf | Dump::Print)
}

fn load_page(
    o: &Opts,
    fetch_url: &CStr,
    hop: c_int,
    top_url: Option<&CStr>,
    post: Option<(&[u8], &CStr)>,
) -> Option<Page> {
    let opts_url = o.url.map(CStr::to_bytes);
    let resp = match ffi::navigate(fetch_url, top_url, post, hop == 0) {
        Ok(resp) => resp,
        Err(message) => {
            let emsg = message.unwrap_or_else(|| b"unknown error".to_vec());
            err(&[b"headless: fetch failed: ", emsg.as_slice(), b"\n"].concat());
            if !is_capture(o.dump) {
                return None;
            }
            let body = ffi::error_page(o.url, 0, Some(&cstring(&emsg)));
            return Some(Page {
                final_url: Some(opts_url.unwrap_or(b"").to_vec()),
                content_type: Some(HTML_UTF8.to_vec()),
                content_language: None,
                body: Some(body),
                timing: None,
            });
        }
    };
    let Response {
        status,
        final_url,
        mut content_type,
        content_language,
        mut body,
        error,
        timing,
    } = resp;
    let page_or_opts =
        |final_url: &Option<Vec<u8>>| opt_cstring(final_url).or_else(|| o.url.map(CStr::to_owned));
    if let Some(error) = error {
        err(&[b"headless: fetch error: ", error.as_slice(), b"\n"].concat());
        if !is_capture(o.dump) {
            return None;
        }
        body = Some(ffi::error_page(
            page_or_opts(&final_url).as_deref(),
            status,
            Some(&cstring(&error)),
        ));
        content_type = Some(HTML_UTF8.to_vec());
    } else if status >= 400 {
        let is_html = content_type.as_deref().is_some_and(|ct| {
            (ct.len() >= 9 && ct[..9].eq_ignore_ascii_case(b"text/html"))
                || (ct.len() >= 17 && ct[..17].eq_ignore_ascii_case(b"application/xhtml"))
        });
        let useful = body.as_ref().is_some_and(|b| b.len() > 64) && is_html;
        if !useful && is_capture(o.dump) {
            body = Some(ffi::error_page(
                page_or_opts(&final_url).as_deref(),
                status,
                None,
            ));
            content_type = Some(HTML_UTF8.to_vec());
        }
    }

    let doc_url = |final_url: &Option<Vec<u8>>| {
        opt_cstring(final_url).unwrap_or_else(|| fetch_url.to_owned())
    };
    let has_body = body.as_ref().is_some_and(|b| !b.is_empty());
    let is_image = content_type
        .as_deref()
        .is_some_and(|ct| ct.len() >= 6 && ct[..6].eq_ignore_ascii_case(b"image/"));
    if is_image && has_body {
        body = Some(ffi::image_document(Some(&doc_url(&final_url))));
        content_type = Some(HTML_UTF8.to_vec());
    }

    if let (Some(ct), Some(b)) = (
        content_type.clone(),
        body.as_deref().filter(|b| !b.is_empty()),
    ) {
        let has = |needle: &[u8]| ct.windows(needle.len()).any(|w| w == needle);
        let is_json = has(b"json");
        let is_xml = !has(b"xhtml")
            && !has(b"svg")
            && (ct.starts_with(b"text/xml") || ct.starts_with(b"application/xml") || has(b"+xml"));
        if is_json || is_xml {
            let ct_c = cstring(&ct);
            let decoded = ffi::decode_body_text(b, Some(&ct_c));
            let url = doc_url(&final_url);
            let html = if is_json {
                ffi::json_document(Some(&url), decoded.as_deref())
            } else {
                ffi::xml_document(Some(&url), decoded.as_deref())
            };
            if let Some(html) = html {
                body = Some(html);
                content_type = Some(HTML_UTF8.to_vec());
            }
        }
    }
    Some(Page {
        final_url,
        content_type,
        content_language,
        body,
        timing: Some(timing),
    })
}

pub fn run_one(
    o: &Opts,
    fetch_url: &CStr,
    hop: c_int,
    top_url: Option<&CStr>,
    post: Option<(&[u8], &CStr)>,
) -> c_int {
    let Some(page) = load_page(o, fetch_url, hop, top_url, post) else {
        return 1;
    };
    let content_type = opt_cstring(&page.content_type);
    let final_url = opt_cstring(&page.final_url);
    let (decoded, charset) = ffi::decode_body(page.body.as_deref(), content_type.as_deref());
    if let Ok(mut c) = DOC_CHARSET.lock() {
        *c = charset.map(|c| c.to_bytes().to_vec());
    }
    let scripting = ffi::javascript_enabled();
    let Some(doc) = Document::parse(decoded.as_deref(), scripting) else {
        return 1;
    };
    let page_url = final_url.clone().or_else(|| o.url.map(CStr::to_owned));

    let mut print_setup = ffi::print_setup_default();
    ffi::css_set_print_media(o.dump == Dump::Print);

    let mut vw = if o.viewport_width > 0 {
        o.viewport_width
    } else {
        1000
    };
    let mut vh = if o.viewport_height > 0 {
        o.viewport_height as f64
    } else {
        vw as f64 * 0.75
    };
    if o.dump == Dump::Print && o.viewport_width <= 0 {
        let ps = &print_setup;
        vw = (ps.width - ps.margin_left - ps.margin_right) as c_int;
        vh = ps.height - ps.margin_top - ps.margin_bottom;
    }
    ffi::css_set_viewport(vw as f64, vh);
    let target_frag = o
        .url
        .and_then(|u| {
            u.to_bytes()
                .iter()
                .position(|&b| b == b'#')
                .map(|i| &u.to_bytes()[i + 1..])
        })
        .filter(|f| !f.is_empty())
        .map(cstring);
    ffi::css_set_target_fragment(target_frag.as_deref());
    ffi::css_set_doc_language(opt_cstring(&page.content_language).as_deref());
    if let Some(frag) = &target_frag {
        reveal_fragment(doc.node(), frag);
    }
    let css_cache = ffi::new_css_cache();
    let styles = ffi::compute_cascade(&doc, page_url.as_deref(), css_cache);

    let anim = Anim::new();
    if let Some(anim) = anim {
        anim.load_keyframes(&doc, page_url.as_deref(), css_cache);
        anim.observe(styles, ffi::monotonic_us());
    }

    let nav = NavCapture::default();
    let timing = page
        .timing
        .as_ref()
        .map_or_else(NavigationTiming::default, |t| NavigationTiming {
            origin_us: t.request_start_us,
            origin_real_ms: t.request_start_real_ms,
            domain_lookup_start_ms: 0.0,
            domain_lookup_end_ms: t.domain_lookup_ms,
            connect_start_ms: t.domain_lookup_ms,
            connect_end_ms: t.connect_ms,
            secure_connection_start_ms: if t.connect_ms < t.tls_ms {
                t.connect_ms
            } else {
                0.0
            },
            request_start_ms: t.pretransfer_ms,
            response_start_ms: t.response_start_ms,
            response_end_ms: t.response_end_ms,
            ..NavigationTiming::default()
        });
    let js = ffi::new_js(&nav, &timing);
    if let Some(js) = js {
        js.bind_form_submit(&nav);
    }
    let image_cache = ImageCache::new();
    let video_cache = VideoCache::new();
    let flush_base = page_url.clone();
    let ctx = Ctx {
        doc: doc.as_ptr(),
        js,
        base: flush_base.clone(),
        vw,
        vh,
        image_cache,
        video_cache,
        anim,
        css_cache,
        styles: Cell::new(styles),
        layout: Cell::new(ptr::null_mut()),
        focused: Cell::new(ptr::null()),
        caret: Cell::new(0),
        anchor: Cell::new(0),
        relaying: Cell::new(false),
    };
    if let Some(js) = js {
        js.set_style_table(ctx.styles.get());
        js.set_image_cache(image_cache);
        js.set_anim(anim);
        js.bind_layout_flush(&ctx);
        js.bind_media(video_cache);
        ffi::bind_video_events(video_cache, Some(js));
        if let Some(cache) = video_cache {
            cache.set_base(flush_base.as_deref());
        }
        if o.wpt {
            js.set_early_inject_src(CStr::from_bytes_with_nul(WPT_HOOK.as_bytes()).unwrap_or(c""));
        }
        if scripting {
            js.run_scripts_in_doc(doc.as_ptr(), final_url.as_deref());
        }
    }

    if o.settle_ms > 0 {
        settle(o.settle_ms, &ctx);
    }
    if let Some(actions) = nonempty(o.actions) {
        crate::input::run_actions(&ctx, &nav, actions.to_bytes());
    }

    let pending = nav.pending_url.borrow_mut().take();
    if let (Some(pending), true) = (pending, hop < 4 && !o.wpt) {
        let base = final_url.clone().unwrap_or_else(|| fetch_url.to_owned());
        let next = if pending.windows(3).any(|w| w == b"://") {
            pending.clone()
        } else {
            ffi::url_resolve(&base, &cstring(&pending)).unwrap_or_else(|| pending.clone())
        };
        let next_post = nav.post_body.borrow_mut().take();
        let next_ct = nav.post_ct.borrow_mut().take();
        let marker: &[u8] = if next_post.is_some() { b" POST" } else { b"" };
        err(&[b"[headless follow", marker, b" ", next.as_slice(), b"]\n"].concat());
        if let Some(js) = js {
            js.unbind_layout_flush();
            js.set_layout_root(ptr::null());
            js.set_style_table(ptr::null_mut());
        }
        if let Some(anim) = anim {
            anim.free();
        }
        if !ctx.layout.get().is_null() {
            ffi::free_layout(ctx.layout.get());
        }
        if !ctx.styles.get().is_null() {
            ffi::destroy_table(ctx.styles.get());
        }
        ffi::destroy_table(css_cache);
        if let Some(js) = js {
            js.free();
        }
        doc.free();
        if let Some(cache) = image_cache {
            cache.free();
        }
        if let Some(cache) = video_cache {
            cache.free();
        }
        drop(decoded);
        let next_opts = Opts {
            actions: None,
            ..*o
        };
        let ct = next_ct.as_deref().map(cstring);
        let post = match (&next_post, &ct) {
            (Some(body), Some(ct)) => Some((body.as_slice(), ct.as_c_str())),
            (Some(body), None) => Some((body.as_slice(), c"")),
            _ => None,
        };
        return run_one(&next_opts, &cstring(&next), hop + 1, Some(&base), post);
    }

    relayout(&ctx);
    if js.is_some() && o.settle_ms > 0 {
        settle(o.settle_ms, &ctx);
        relayout(&ctx);
    }

    let mut rc = 0;
    if js.is_some() && o.wpt {
        rc = wpt_finish(&ctx, o);
    }

    if let (Some(js), Some(src)) = (js, nonempty(o.eval)) {
        if let Some(result) = ffi::eval_source(Some(js), src, c"headless-eval") {
            out(&[b"eval: ", result.to_bytes(), b"\n"].concat());
        }
        if ffi::consume_mutated(Some(js)) {
            relayout(&ctx);
        }
    }

    match o.dump {
        Dump::Text => out(&ffi::dump_text(ctx.layout.get())),
        Dump::Dom => out(&ffi::node_dump(doc.node())),
        Dump::Layout => out(&ffi::dump_layout(ctx.layout.get())),
        Dump::Png | Dump::Pdf | Dump::Print => {
            let base = page_url.as_deref();
            ffi::fetch_images(ctx.layout.get(), base, image_cache);
            relayout(&ctx);
            if o.dump == Dump::Print {
                ffi::apply_render_page_rule(&mut print_setup);
                let ps = &print_setup;
                let w = ps.width - ps.margin_left - ps.margin_right;
                if w > 0.0 && o.viewport_width <= 0 && w as c_int != vw {
                    vw = w as c_int;
                    vh = ps.height - ps.margin_top - ps.margin_bottom;
                    ffi::css_set_viewport(vw as f64, vh);
                    relayout(&ctx);
                }
            }
            ffi::paint_set_js(ffi::js_raw(js));
            fetch_videos_into_layout(&ctx, base);

            let time_ms = if o.time_ms >= 0 { o.time_ms } else { 1000 };
            if let Some(anim) = anim {
                anim.rebase(0);
                anim.tick(0);
            }
            ffi::paint_set_anim(anim);
            let initial = ffi::suffix_before_ext(o.out_path, c"-initial");
            rc = write_capture(ctx.layout.get(), initial.as_deref(), o.dump, &print_setup);
            let shown = initial.as_ref().map_or(&b"(null)"[..], |p| p.to_bytes());
            err(&[b"[headless] initial render -> ", shown, b"\n"].concat());
            drop(initial);

            if let Some(js) = js {
                js.fire_media_load_events(ctx.layout.get());
            }
            settle(time_ms, &ctx);
            relayout(&ctx);
            fetch_videos_into_layout(&ctx, base);
            if let Some(anim) = anim {
                anim.rebase(0);
                let end = time_ms as i64 * 1000;
                let mut t = 0;
                while t <= end {
                    anim.tick(t);
                    t += 16000;
                }
                anim.tick(end);
            }
            ffi::paint_set_anim(anim);
            let rc2 = write_capture(ctx.layout.get(), o.out_path, o.dump, &print_setup);
            let path = o.out_path.map_or(&b"(null)"[..], CStr::to_bytes);
            err(&[
                format!("[headless] after {time_ms}ms -> ").as_bytes(),
                path,
                b"\n",
            ]
            .concat());
            if rc == 0 {
                rc = rc2;
            }
        }
        Dump::None | Dump::Unknown => {}
    }

    if let Some(report) = crate::inspect::report(
        ctx.layout(),
        ctx.doc(),
        ffi::StyleTable::from_table(ctx.styles.get()),
        o.inspect,
        o.inspect_at,
    ) {
        out(&report);
    }

    drop(decoded);
    drop(nav.pending_url.borrow_mut().take());
    nav.clear_post();
    ffi::paint_set_anim(None);
    if let Some(anim) = anim {
        anim.free();
    }
    if let Some(js) = js {
        js.set_layout_root(ptr::null());
        js.set_style_table(ptr::null_mut());
    }
    if !ctx.layout.get().is_null() {
        ffi::free_layout(ctx.layout.get());
    }
    if !ctx.styles.get().is_null() {
        ffi::destroy_table(ctx.styles.get());
    }
    ffi::destroy_table(css_cache);
    if let Some(js) = js {
        js.free();
    }
    doc.free();
    if let Some(cache) = image_cache {
        cache.free();
    }
    if let Some(cache) = video_cache {
        cache.free();
    }
    rc
}
