//! Southstar — struct ns_browser with its typed fields and the C ABI of src/libsouthstar.h and the page layers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod dom;
mod engine;
mod events;
mod glib;
mod js;
mod net;
mod paint;
mod trampolines;

use core::cell::{Cell, RefCell, RefMut};
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;

use southstar_css::StyleMap;
use southstar_dom::{Node, Tree, WeakNode};
use southstar_engine::CssCache;
use southstar_glib::GStr;
use southstar_layout::{BoxRef, NsBox};
use southstar_selection::{NsSelection, Selection};

pub use dom::*;
pub use engine::*;
pub use events::*;
pub use glib::*;
pub use js::Js;
pub use net::*;
pub use paint::*;

use crate::api::Page;
use crate::{build, callbacks, hit, input, open, page, query, render, scroll, settle};

#[derive(Default)]
pub struct Flag(Cell<bool>);

impl Flag {
    pub fn get(&self) -> bool {
        self.0.get()
    }

    pub fn set(&self, value: bool) {
        self.0.set(value);
    }

    pub fn take(&self) -> bool {
        self.0.replace(false)
    }
}
#[derive(Default)]
pub struct Text(RefCell<Option<Rc<CStr>>>);

impl Text {
    pub fn get(&self) -> Option<Rc<CStr>> {
        self.0.borrow().clone()
    }

    pub fn ptr(&self) -> *const c_char {
        self.0.borrow().as_deref().map_or(ptr::null(), CStr::as_ptr)
    }

    pub fn is_set(&self) -> bool {
        self.0.borrow().is_some()
    }

    pub fn put(&self, value: Option<Rc<CStr>>) {
        drop(self.0.replace(value));
    }

    pub fn adopt(&self, value: Option<GStr>) {
        self.set(value.as_deref());
    }

    pub fn set(&self, value: Option<&CStr>) {
        self.put(value.map(Rc::from));
    }

    pub fn take(&self) -> Option<Rc<CStr>> {
        self.0.take()
    }

    pub fn take_malloc(&self) -> *mut c_char {
        self.take()
            .map_or(ptr::null_mut(), |text| malloc_dup(text.to_bytes()))
    }

    pub fn clear(&self) {
        self.put(None);
    }
}

#[derive(Default)]
pub struct NodeSlot(Cell<Option<WeakNode>>);

impl NodeSlot {
    pub fn get(&self) -> Option<Node<'_>> {
        self.0.get().and_then(WeakNode::get)
    }

    pub fn set(&self, node: Option<Node<'_>>) {
        self.0.set(node.map(Node::downgrade));
    }

    pub fn is(&self, node: Option<Node<'_>>) -> bool {
        self.get() == node
    }
}

pub struct BoxSlot(Cell<*const NsBox>);

impl Default for BoxSlot {
    fn default() -> BoxSlot {
        BoxSlot(Cell::new(ptr::null()))
    }
}

impl BoxSlot {
    pub fn set(&self, b: Option<BoxRef<'_>>) {
        self.0.set(b.map_or(ptr::null(), |b| b.as_ptr()));
    }
}

pub struct LayoutSlot(Cell<*mut NsBox>);

impl Default for LayoutSlot {
    fn default() -> LayoutSlot {
        LayoutSlot(Cell::new(ptr::null_mut()))
    }
}

pub struct SelectionSlot(Cell<NsSelection>);

impl Default for SelectionSlot {
    fn default() -> SelectionSlot {
        SelectionSlot(Cell::new(NsSelection::EMPTY))
    }
}

#[derive(Default)]
pub struct NsBrowser {
    doc: RefCell<Option<Tree>>,
    layout: LayoutSlot,
    styles: RefCell<Option<Rc<StyleMap>>>,
    js: Cell<Option<Js>>,
    anim: Cell<Option<Anim>>,
    images: Cell<Option<Images>>,
    videos: Cell<Option<Videos>>,
    css_cache: Cell<Option<CssCache>>,
    pub base_url: Text,
    pub doc_charset: Text,
    pub doc_language: Text,
    pub vw: Cell<c_int>,
    pub vh: Cell<f64>,
    pub images_fetched: Flag,
    pub has_deferred_lazy: Flag,
    pub bfcache_ok: Flag,
    pub cur_scroll_x: Cell<f64>,
    pub cur_scroll_y: Cell<f64>,
    pub cur_scale: Cell<f64>,
    pub js_scroll_x: Cell<f64>,
    pub js_scroll_y: Cell<f64>,
    pub cur_viewport_h: Cell<f64>,
    pub pending_scroll_x: Cell<c_int>,
    pub pending_scroll_y: Cell<c_int>,
    pub pending_scroll: Flag,
    pub scroll_anchor: NodeSlot,
    pub scroll_anchor_y: Cell<c_int>,
    img_sessions: RefCell<Vec<Session>>,
    pub image_arrivals_since_layout: Cell<c_uint>,
    pub media_events_source: Cell<c_uint>,
    pub images_arrived_since_layout: Flag,
    pub load_delay_deadline_us: Cell<i64>,
    img_requested: RefCell<HashSet<CString>>,
    pub dirty: Flag,
    pub dppx: Cell<f64>,
    pub cascade_dirty: Flag,
    pub relaying: Flag,
    pub pending_nav: Text,
    pub soft_nav_pushed: Flag,
    pub pending_download: Text,
    pub pending_clipboard: Text,
    pub pending_window_action: Text,
    pub pending_audio: StrBuf,
    pub refresh_url: Text,
    pub refresh_due_us: Cell<i64>,
    pending_post_body: RefCell<Option<Vec<u8>>>,
    pending_post_ct: Text,
    pub caret_byte: Cell<usize>,
    pub sel_anchor_byte: Cell<usize>,
    pub caret_blink_node: NodeSlot,
    pub caret_blink_byte: Cell<usize>,
    pub caret_blink_anchor: Cell<usize>,
    pub caret_blink_epoch_us: Cell<i64>,
    pub caret_blink_active: Flag,
    pub caret_paint_visible: Flag,
    selection: SelectionSlot,
    pub selection_dragged: Flag,
    pub hover_node: NodeSlot,
    pub open_select: NodeSlot,
    pub datalist_suppressed: Flag,
    pub press_node: NodeSlot,
    pub press_x: Cell<c_int>,
    pub press_y: Cell<c_int>,
    pub press_mods: Cell<c_int>,
    pub press_active: Flag,
    pub keydown_prevented: Flag,
    pub search_query: Text,
    pub search_case: Flag,
    pub search_active: BoxSlot,
    pub console_buf: StrBuf,
    pub layout_sig: Cell<[u64; 2]>,
    pub layout_osc: Cell<c_int>,
    pub last_layout_us: Cell<i64>,
    pub damp_until_us: Cell<i64>,
    pub damp_logged: Flag,
    pub hover_relayout_us: Cell<i64>,
    pub relayout_cost_us: Cell<i64>,
    pub hover_restyle_pending: Flag,
    sb_box: LayoutSlot,
    pub sb_node: NodeSlot,
    pub sb_grab: Cell<f64>,
    pub sb_dragging: Flag,
    pub security: Cell<c_int>,
    pub remote_ip: Text,
}

impl NsBrowser {
    pub fn as_ptr(&self) -> *mut NsBrowser {
        ptr::from_ref(self).cast_mut()
    }

    pub fn doc(&self) -> Option<Node<'_>> {
        self.doc.borrow().as_ref().map(Tree::root)
    }

    pub fn layout(&self) -> Option<BoxRef<'_>> {
        // SAFETY: the layout slot is null or holds the page's live box tree, which is freed only after it leaves the slot.
        unsafe { BoxRef::from_ptr(self.layout.0.get()) }
    }

    pub fn js(&self) -> Option<Js> {
        self.js.get()
    }

    pub fn anim(&self) -> Option<Anim> {
        self.anim.get()
    }

    pub fn images(&self) -> Option<Images> {
        self.images.get()
    }

    pub fn videos(&self) -> Option<Videos> {
        self.videos.get()
    }

    pub fn take_layout(&self) {
        self.free_layout();
        self.sb_box.0.set(ptr::null_mut());
        self.sb_node.set(None);
        self.sb_dragging.set(false);
    }

    fn free_layout(&self) {
        let layout = self.layout.0.replace(ptr::null_mut());
        if !layout.is_null() {
            paint_3d_invalidate();
            // SAFETY: the pointer came out of the page's layout slot, which owned the tree, and nothing else frees it.
            unsafe { box_free(layout) };
        }
    }

    pub fn drop_styles(&self) {
        drop(self.styles.take());
    }

    fn selection<'a>(&'a self) -> Selection<'a> {
        // SAFETY: the page clears its selection before it frees or replaces the layout, so the boxes it names belong to the live layout tree.
        unsafe { self.selection.0.get().load() }
    }

    pub fn update_selection<'a>(&'a self, f: impl FnOnce(&mut Selection<'a>) -> bool) -> bool {
        let mut selection = self.selection();
        let changed = f(&mut selection);
        self.selection.0.set(NsSelection::store(&selection));
        changed
    }

    pub fn selection_clear(&self) {
        self.selection.0.set(NsSelection::EMPTY);
    }

    pub fn selection_has_range(&self) -> bool {
        self.selection().has_range()
    }

    pub fn selection_select_all(&self, layout: BoxRef<'_>) -> bool {
        self.update_selection(|s| s.select_all(Some(layout)))
    }

    pub fn selection_text(&self) -> Option<CString> {
        let mut text = self.selection().collect_text(self.layout())?;
        if let Some(nul) = text.iter().position(|&b| b == 0) {
            text.truncate(nul);
        }
        CString::new(text).ok()
    }

    pub fn selection_bounds(&self) -> (bool, [f64; 4]) {
        match self.selection().bounds(self.layout()) {
            Some((x, y, w, h)) => (true, [x, y, w, h]),
            None => (false, [0.0; 4]),
        }
    }

    pub fn sessions(&self) -> Sessions<'_> {
        Sessions(&self.img_sessions)
    }

    pub fn requested_images(&self) -> RefMut<'_, HashSet<CString>> {
        self.img_requested.borrow_mut()
    }

    pub fn set_post(&self, body: StrBufOwned, content_type: &CStr) {
        self.pending_post_body.replace(Some(body.into_bytes()));
        self.pending_post_ct.set(Some(content_type));
    }

    pub fn take_post(&self) -> Option<(Vec<u8>, Option<Rc<CStr>>)> {
        let body = self.pending_post_body.take()?;
        Some((body, self.pending_post_ct.take()))
    }

    pub fn release(&self) {
        let source = self.media_events_source.get();
        if source != 0 {
            source_remove(source);
        }
        self.sessions().close_all();
        self.img_requested.borrow_mut().clear();
        css_set_active_node(None);
        paint_set_anim(None);
        if let Some(js) = self.js() {
            js.set_layout_root(None);
            js.set_style_table(None);
        }
        if let Some(anim) = self.anim.take() {
            // SAFETY: the handle is out of the page's slot, paint was detached from it above, and nothing samples animations or runs script before the script engine's copy is released below.
            unsafe { anim.free() };
        }
        self.free_layout();
        self.drop_styles();
        if let Some(cache) = self.css_cache.take() {
            cache.free();
        }
        if let Some(js) = self.js() {
            js.free();
        }
        drop(self.doc.borrow_mut().take());
        if let Some(videos) = self.videos.take() {
            // SAFETY: the handle is out of the page's slot, and script, whose media callbacks reach the cache through that slot, was released above.
            unsafe { videos.free() };
        }
        if let Some(images) = self.images.take() {
            // SAFETY: the handle is out of the page's slot, and script, layout and every image session that held a copy were released above.
            unsafe { images.free() };
        }
        for text in [
            &self.base_url,
            &self.doc_charset,
            &self.doc_language,
            &self.pending_nav,
            &self.pending_download,
            &self.pending_clipboard,
            &self.pending_window_action,
        ] {
            text.clear();
        }
        self.pending_audio.free();
        for text in [
            &self.refresh_url,
            &self.pending_post_ct,
            &self.search_query,
            &self.remote_ip,
        ] {
            text.clear();
        }
        self.pending_post_body.take();
        self.console_buf.free();
    }
}

pub struct Sessions<'a>(&'a RefCell<Vec<Session>>);

impl Sessions<'_> {
    pub fn push(&self, session: Session) {
        self.0.borrow_mut().push(session);
    }

    pub fn outstanding(&self) -> c_int {
        let mut total = 0;
        let mut i = 0;
        loop {
            let mut sessions = self.0.borrow_mut();
            let Some(outstanding) = sessions.get(i).map(Session::outstanding) else {
                break;
            };
            if outstanding == 0 {
                let finished = sessions.swap_remove(i);
                drop(sessions);
                finished.close();
                continue;
            }
            total += outstanding;
            i += 1;
        }
        total
    }

    fn close_all(&self) {
        let sessions = self.0.take();
        for session in sessions {
            session.close();
        }
    }
}

pub fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: callers pass null or a NUL-terminated string that outlives the borrow: an embedder's argument for the length of its call, or a callback argument for the length of the callback.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn bool_int(value: bool) -> c_int {
    c_int::from(value)
}

fn put<T: Copy>(out: Option<&Cell<T>>, value: T) {
    if let Some(out) = out {
        out.set(value);
    }
}

fn malloc_or_null(s: Option<Rc<CStr>>) -> *mut c_char {
    s.map_or(ptr::null_mut(), |s| malloc_dup(s.to_bytes()))
}

fn take_or_null(s: Option<GStr>) -> *mut c_char {
    s.map_or(ptr::null_mut(), |s| {
        let raw = s.as_ptr().cast_mut();
        core::mem::forget(s);
        raw
    })
}

pub struct Created<'a> {
    pub doc: ParsedDoc,
    pub base: Option<GStr>,
    pub view: &'a open::Viewport,
    pub bfcache_ok: bool,
    pub refresh_header: Option<GStr>,
    pub doc_language: Option<GStr>,
    pub csp_header: Option<GStr>,
    pub doc_charset: Option<GStr>,
    pub url: Option<&'a CStr>,
    pub timing: Option<&'a NavigationTiming>,
}

pub fn create_browser(c: Created<'_>) -> Page {
    let page = Page::adopt(Box::default());
    let b = page.browser();
    b.doc.replace(c.doc.0);
    b.base_url.adopt(c.base);
    b.doc_charset.adopt(c.doc_charset);
    b.doc_language.adopt(c.doc_language);
    let setup = build::Setup {
        viewport_width: c.view.width,
        viewport_height: c.view.height,
        settle_ms: c.view.settle_ms,
        bfcache_ok: c.bfcache_ok,
        refresh_header: c.refresh_header,
        csp_header: c.csp_header,
        url: c.url,
        navigation_timing: c.timing.copied(),
    };
    build::build(b, setup);
    page
}

pub fn attach_js(b: &NsBrowser, navigation_timing: Option<NavigationTiming>) -> Option<Js> {
    let js = trampolines::new_js(b, navigation_timing);
    b.js.set(js);
    js
}

pub fn attach_caches(b: &NsBrowser) {
    b.css_cache.set(Some(CssCache::create()));
    b.images.set(Some(Images::create()));
    b.videos.set(Some(Videos::create()));
}

pub fn set_anim(b: &NsBrowser, anim: Anim) {
    b.anim.set(Some(anim));
}

pub fn wire_js_callbacks(b: &NsBrowser, js: Js) {
    trampolines::wire_js(b, js);
}

pub fn wire_video_callbacks(b: &NsBrowser, videos: Videos) {
    trampolines::wire_videos(b, videos);
}

pub fn schedule_media_events(b: &NsBrowser) {
    if b.media_events_source.get() == 0 {
        b.media_events_source
            .set(trampolines::add_media_events_timeout(b));
    }
}

pub fn start_image_session(b: &NsBrowser, viewport_h: f64) -> (Option<Session>, bool) {
    trampolines::start_image_session(b, viewport_h)
}

pub fn run_settle_loop(b: &NsBrowser, settle_ms: c_int) {
    trampolines::run_settle_loop(b, settle_ms);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_audio(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| b.pending_audio.take_text())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_video_helper_event(
    b: Option<&NsBrowser>,
    token: *const c_char,
    kind: *const c_char,
) -> c_int {
    let Some(videos) = b.and_then(NsBrowser::videos) else {
        return 0;
    };
    let Some(kind) = c_str(kind) else {
        return 0;
    };
    bool_int(
        c_str(token).is_some_and(|token| videos.helper_event(token.to_bytes(), kind.to_bytes())),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_init() -> c_int {
    build::init();
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_sandbox(self_exe: *const c_char) {
    sandbox(c_str(self_exe));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_shutdown() {
    shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_render_text(b: Option<&NsBrowser>) -> *mut c_char {
    match b.and_then(query::dump_text) {
        Some(text) => malloc_dup(&text.into_bytes()),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_dump_dom(b: Option<&NsBrowser>) -> *mut c_char {
    match b.and_then(query::dump_dom) {
        Some(dom) => southstar_glib::strdup(&dom),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_dump_layout(b: Option<&NsBrowser>) -> *mut c_char {
    b.and_then(query::dump_layout)
        .map_or(ptr::null_mut(), StrBufOwned::into_text)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_dump_performance(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| query::dump_performance(b).into_text())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_image(
    b: Option<&NsBrowser>,
    path: *const c_char,
) -> c_int {
    match (b, c_str(path)) {
        (Some(b), Some(path)) => query::render_image(b, path),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_tick(b: Option<&NsBrowser>, budget_ms: c_int) -> c_int {
    b.map_or(-1, |b| bool_int(settle::tick(b, budget_ms)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_animating(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(settle::animating(b)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_viewport(
    b: Option<&NsBrowser>,
    css_width: c_int,
    css_height: f64,
) -> c_int {
    b.map_or(-1, |b| page::set_viewport(b, css_width, css_height))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_device_pixel_ratio(b: Option<&NsBrowser>, dppx: f64) -> c_int {
    page::set_device_pixel_ratio(b, dppx)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_viewport_width(b: Option<&NsBrowser>, css_width: c_int) -> c_int {
    ns_browser_set_viewport(b, css_width, f64::from(css_width) * 0.75)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_window_action_applied(b: Option<&NsBrowser>) {
    if let Some(js) = b.and_then(NsBrowser::js) {
        js.window_action_applied();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_page_size(
    b: Option<&NsBrowser>,
    out_width: Option<&Cell<c_int>>,
    out_height: Option<&Cell<c_int>>,
) -> c_int {
    let Some((w, h)) = b.and_then(page::page_size) else {
        return -1;
    };
    put(out_width, w);
    put(out_height, h);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_console_drain(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| b.console_buf.take_text())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_post(
    b: Option<&NsBrowser>,
    out_len: Option<&Cell<usize>>,
    out_ct: Option<&Cell<*mut c_char>>,
) -> *mut c_char {
    put(out_len, 0);
    put(out_ct, ptr::null_mut());
    let Some((body, content_type)) = b.and_then(NsBrowser::take_post) else {
        return ptr::null_mut();
    };
    put(out_len, body.len());
    if let Some(out) = out_ct {
        out.set(malloc_or_null(content_type));
    }
    malloc_dup(&body)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_focused_editable(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(query::focused_editable(b)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_caret_blink_active(b: Option<&NsBrowser>, active: c_int) -> c_int {
    b.map_or(0, |b| {
        bool_int(query::set_caret_blink_active(b, active != 0))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_caret_blinking(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(b.caret_blink_active.get()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_focused_editable_value(
    b: Option<&NsBrowser>,
    out_caret: Option<&Cell<usize>>,
    out_anchor: Option<&Cell<usize>>,
) -> *mut c_char {
    put(out_caret, 0);
    put(out_anchor, 0);
    let Some((value, caret, anchor)) = b.and_then(query::focused_editable_value) else {
        return ptr::null_mut();
    };
    put(out_caret, caret);
    put(out_anchor, anchor);
    malloc_dup(&value)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_focused_editable_selection(
    b: Option<&NsBrowser>,
    caret: usize,
    anchor: usize,
) -> c_int {
    b.map_or(0, |b| {
        bool_int(query::set_focused_editable_selection(b, caret, anchor))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_title(b: Option<&NsBrowser>) -> *mut c_char {
    match b.and_then(query::title) {
        Some(title) => malloc_dup(&title),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_url(b: Option<&NsBrowser>) -> *mut c_char {
    malloc_or_null(b.and_then(|b| b.base_url.get()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_security(
    b: Option<&NsBrowser>,
    out_ip: Option<&Cell<*const c_char>>,
) -> c_int {
    put(out_ip, b.map_or(ptr::null(), |b| b.remote_ip.ptr()));
    b.map_or(0, |b| b.security.get())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_nav(b: Option<&NsBrowser>) -> *mut c_char {
    malloc_or_null(b.and_then(|b| b.pending_nav.take()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_soft_nav_pushed(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(b.soft_nav_pushed.take()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_scroll(
    b: Option<&NsBrowser>,
    out_scroll_x: Option<&Cell<c_int>>,
    out_scroll_y: Option<&Cell<c_int>>,
) -> c_int {
    let Some((x, y)) = b.and_then(scroll::take_pending) else {
        return 0;
    };
    put(out_scroll_x, x);
    put(out_scroll_y, y);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_scroll_y(
    b: Option<&NsBrowser>,
    out_scroll_y: Option<&Cell<c_int>>,
) -> c_int {
    ns_browser_take_pending_scroll(b, None, out_scroll_y)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_webgl(_b: Option<&NsBrowser>) -> *mut c_char {
    webgl_take_pending_origin().map_or(ptr::null_mut(), |origin| southstar_glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_has_pending_clipboard(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(b.pending_clipboard.is_set()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_clipboard(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| b.pending_clipboard.take_malloc())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_download(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| b.pending_download.take_malloc())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_window_action(b: Option<&NsBrowser>) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| b.pending_window_action.take_malloc())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_webgl(
    _b: Option<&NsBrowser>,
    origin: *const c_char,
    allow: c_int,
) {
    if let Some(origin) = c_str(origin) {
        webgl_set_decision(origin, allow != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_camera(_b: Option<&NsBrowser>) -> *mut c_char {
    southstar_camera::take_pending_origin()
        .map_or(ptr::null_mut(), |origin| southstar_glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_camera(
    b: Option<&NsBrowser>,
    origin: *const c_char,
    allow: c_int,
) {
    callbacks::resolve_camera(b, c_str(origin), allow != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_links(b: Option<&NsBrowser>) -> *mut c_char {
    match b.and_then(query::links) {
        Some(links) => malloc_dup(&links),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_favicon_url(b: Option<&NsBrowser>) -> *mut c_char {
    match b.and_then(query::favicon_url) {
        Some(url) => malloc_dup(&url),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_bfcache_eligible(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(b.bfcache_ok.get()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_bfcache_park(b: Option<&NsBrowser>) {
    if let Some(b) = b {
        page::bfcache_park(b);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_bfcache_restore(
    b: Option<&NsBrowser>,
    viewport_width: c_int,
    viewport_height: f64,
) {
    if let Some(b) = b {
        page::bfcache_restore(b, viewport_width, viewport_height);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_busy(b: Option<&NsBrowser>) -> c_int {
    b.map_or(0, |b| bool_int(settle::busy(b)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_close(b: *mut NsBrowser) {
    // SAFETY: the embedder hands back a page an ns_browser_open call returned, which it no longer uses.
    drop(unsafe { Page::from_raw(b) });
}

pub fn browser_for(ud: *mut c_void) -> Option<&'static NsBrowser> {
    // SAFETY: callback user data is the address of the page that registered the callback, and the page unregisters its callbacks before it is freed.
    unsafe { ud.cast::<NsBrowser>().as_ref() }
}

fn view(width: c_int, height: f64, settle_ms: c_int) -> open::Viewport {
    open::Viewport {
        width,
        height,
        settle_ms,
    }
}

fn open_with(url: Option<&CStr>, view: open::Viewport, post: Option<&Post<'_>>) -> *mut NsBrowser {
    url.and_then(|url| open::open(url, &view, post))
        .map_or(ptr::null_mut(), Page::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_next_referrer(url: *const c_char) {
    open::set_next_referrer(c_str(url));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_next_user_activated(user_activated: c_int) {
    open::set_next_user_activated(user_activated != 0);
}

pub fn set_color_scheme(dark: bool) {
    css_set_color_scheme(dark);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_color_scheme(dark: c_int) {
    set_color_scheme(dark != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_reduced_motion(reduce: c_int) {
    css_set_reduced_motion(reduce != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_open(
    url: *const c_char,
    viewport_width: c_int,
    settle_ms: c_int,
) -> *mut NsBrowser {
    open_with(c_str(url), view(viewport_width, 0.0, settle_ms), None)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_open_viewport(
    url: *const c_char,
    viewport_width: c_int,
    viewport_height: f64,
    settle_ms: c_int,
) -> *mut NsBrowser {
    open_with(
        c_str(url),
        view(viewport_width, viewport_height, settle_ms),
        None,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_open_post(
    url: *const c_char,
    viewport_width: c_int,
    settle_ms: c_int,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
) -> *mut NsBrowser {
    // SAFETY: the embedder's arguments carry the same guarantees ns_browser_open_post_viewport requires.
    unsafe {
        ns_browser_open_post_viewport(
            url,
            viewport_width,
            0.0,
            settle_ms,
            body,
            body_len,
            content_type,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_open_post_viewport(
    url: *const c_char,
    viewport_width: c_int,
    viewport_height: f64,
    settle_ms: c_int,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
) -> *mut NsBrowser {
    let post = (!body.is_null()).then(|| {
        // SAFETY: the embedder passes body_len readable bytes at body, live for the call.
        let body = unsafe { southstar_glib::slice(body.cast(), body_len) };
        Post {
            body,
            content_type: c_str(content_type),
        }
    });
    open_with(
        c_str(url),
        view(viewport_width, viewport_height, settle_ms),
        post.as_ref(),
    )
}

/// # Safety
///
/// `out` is null or points to `stride * height` writable bytes that nothing else uses for `'a`.
unsafe fn pixel_buf<'a>(
    out: *mut u8,
    width: c_int,
    height: c_int,
    stride: c_int,
) -> Option<PixelBuf<'a>> {
    let len = usize::try_from(stride)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    if out.is_null() {
        return None;
    }
    // SAFETY: the caller passes `stride * height` writable bytes at `out`, unaliased for 'a.
    let pixels = unsafe { core::slice::from_raw_parts_mut(out, len) };
    PixelBuf::from_slice(pixels, width, height, stride)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_rgba(
    b: Option<&NsBrowser>,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    out: *mut u8,
    stride: c_int,
) -> c_int {
    match (b, unsafe { pixel_buf(out, width, height, stride) }) {
        (Some(b), Some(buf)) => render::render_rgba(b, scroll_x, scroll_y, scale, &buf),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_argb32(
    b: Option<&NsBrowser>,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    out: *mut u8,
    stride: c_int,
) -> c_int {
    match (b, unsafe { pixel_buf(out, width, height, stride) }) {
        (Some(b), Some(buf)) => render::render_argb32(b, scroll_x, scroll_y, scale, &buf),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_snap_document(
    b: Option<&NsBrowser>,
    viewport_w: f64,
    viewport_h: f64,
    prev_x: c_int,
    prev_y: c_int,
    scroll_x: Option<&Cell<c_int>>,
    scroll_y: Option<&Cell<c_int>>,
) -> c_int {
    let (Some(b), Some(sx), Some(sy)) = (b, scroll_x, scroll_y) else {
        return 0;
    };
    match render::snap_document(
        b,
        (viewport_w, viewport_h),
        (prev_x, prev_y),
        (sx.get(), sy.get()),
    ) {
        Some((x, y)) => {
            sx.set(x);
            sy.set(y);
            1
        }
        None => 0,
    }
}

fn gstrdup(bytes: &[u8]) -> *mut c_char {
    take_or_null(gstr_from(bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_link_at(b: Option<&NsBrowser>, x: c_int, y: c_int) -> *mut c_char {
    take_or_null(b.and_then(|b| hit::link_near(b, x, y, 9)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_link_under(b: Option<&NsBrowser>, x: c_int, y: c_int) -> *mut c_char {
    take_or_null(b.and_then(|b| hit::link_near(b, x, y, 1)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_cursor_at(b: Option<&NsBrowser>, x: c_int, y: c_int) -> *mut c_char {
    match b.and_then(|b| hit::cursor_at(b, x, y)) {
        Some(cursor) => gstrdup(cursor),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_select(
    b: Option<&NsBrowser>,
    kind: c_int,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    match b.and_then(|b| input::select(b, kind, x, y)) {
        Some(input::Selected::Text(text)) => gstrdup(&text),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_hover(b: Option<&NsBrowser>, x: c_int, y: c_int) -> c_int {
    b.map_or(-1, |b| input::hover(b, x, y))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_scroll_at(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
) -> c_int {
    ns_browser_scroll_at_full(b, x, y, dx, dy, None)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_scroll_at_full(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
    out_snapped: Option<&Cell<c_int>>,
) -> c_int {
    put(out_snapped, 0);
    let Some(b) = b else {
        return 0;
    };
    let (consumed, snapped) = input::scroll_at(b, x, y, dx, dy);
    if consumed != 0 {
        put(out_snapped, bool_int(snapped));
    }
    consumed
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_scrollbar_press(b: Option<&NsBrowser>, x: c_int, y: c_int) -> c_int {
    b.map_or(0, |b| input::scrollbar_press(b, x, y))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_scrollbar_drag(b: Option<&NsBrowser>, _x: c_int, y: c_int) -> c_int {
    b.map_or(0, |b| input::scrollbar_drag(b, y))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_scrollbar_release(b: Option<&NsBrowser>) {
    if let Some(b) = b {
        input::scrollbar_release(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_drop_files(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    paths: *const *const c_char,
    n_paths: c_int,
) -> c_int {
    let Some(b) = b else {
        return 0;
    };
    if paths.is_null() || n_paths <= 0 {
        return 0;
    }
    // SAFETY: the embedder passes n_paths string pointers at paths, live for the call.
    let paths = unsafe { core::slice::from_raw_parts(paths, n_paths as usize) };
    let list: Vec<&CStr> = paths.iter().filter_map(|&path| c_str(path)).collect();
    input::drop_files(b, x, y, &list)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_eval(b: Option<&NsBrowser>, src: *const c_char) -> *mut c_char {
    match (b, c_str(src)) {
        (Some(b), Some(src)) => take_or_null(input::eval(b, src)),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_contextmenu_full(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    out_edit: Option<&Cell<c_int>>,
) -> c_int {
    put(out_edit, 0);
    let Some(b) = b else {
        return 0;
    };
    let (prevented, edit_state) = input::contextmenu(b, x, y);
    put(out_edit, edit_state);
    prevented
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_contextmenu(b: Option<&NsBrowser>, x: c_int, y: c_int) -> c_int {
    ns_browser_contextmenu_full(b, x, y, None)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_media_at(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    out_is_video: Option<&Cell<c_int>>,
    out_stream: Option<&Cell<c_int>>,
) -> *mut c_char {
    put(out_is_video, 0);
    put(out_stream, 0);
    let Some(found) = b.and_then(|b| hit::media_at(b, x, y)) else {
        return ptr::null_mut();
    };
    put(out_is_video, bool_int(found.is_video));
    put(out_stream, bool_int(found.stream));
    take_or_null(Some(found.url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_find(
    b: Option<&NsBrowser>,
    query: *const c_char,
    case_sensitive: c_int,
    direction: c_int,
    from_y: c_int,
    out_total: Option<&Cell<c_int>>,
    out_current: Option<&Cell<c_int>>,
    out_y: Option<&Cell<c_int>>,
) -> c_int {
    let outs = [out_total, out_current, out_y];
    for out in outs {
        put(out, 0);
    }
    let Some(found) =
        b.and_then(|b| hit::find(b, c_str(query), case_sensitive != 0, direction, from_y))
    else {
        return -1;
    };
    for (out, value) in outs.into_iter().zip([found.total, found.current, found.y]) {
        put(out, value);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_press(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    mods: c_int,
) -> *mut c_char {
    b.map_or(ptr::null_mut(), |b| {
        malloc_or_null(input::press(b, x, y, mods))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_release_click(
    b: Option<&NsBrowser>,
    out_changed: Option<&Cell<c_int>>,
) -> *mut c_char {
    let Some(b) = b else {
        put(out_changed, -1);
        return ptr::null_mut();
    };
    put(out_changed, 0);
    let (nav, changed) = input::release_click(b);
    if changed {
        put(out_changed, 1);
    }
    malloc_or_null(nav)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_release(b: Option<&NsBrowser>) -> c_int {
    b.map_or(-1, |b| bool_int(input::release_click(b).1))
}

fn is_nav(nav: Option<&Rc<CStr>>) -> bool {
    nav.is_some_and(|nav| !nav.is_empty())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_click(
    b: Option<&NsBrowser>,
    x: c_int,
    y: c_int,
    mods: c_int,
) -> *mut c_char {
    let Some(b) = b else {
        return ptr::null_mut();
    };
    let nav = input::press(b, x, y, mods);
    if is_nav(nav.as_ref()) {
        b.press_node.set(None);
        b.press_active.set(false);
        css_set_active_node(None);
        return malloc_or_null(nav);
    }
    let (nav, _) = input::release_click(b);
    if !is_nav(nav.as_ref()) {
        hit::video_click_toggle(b, x, y);
    }
    malloc_or_null(nav)
}

fn key(
    b: Option<&NsBrowser>,
    event: &input::KeyEvent<'_>,
    out_prevented: Option<&Cell<c_int>>,
) -> *mut c_char {
    put(out_prevented, 0);
    let Some(b) = b else {
        return ptr::null_mut();
    };
    let (nav, prevented) = input::key(b, event);
    if prevented {
        put(out_prevented, 1);
    }
    malloc_or_null(nav)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_key_full(
    b: Option<&NsBrowser>,
    kind: c_int,
    key_name: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
    out_prevented: Option<&Cell<c_int>>,
) -> *mut c_char {
    let event = input::KeyEvent {
        kind,
        key: c_str(key_name),
        code: c_str(code),
        keycode,
        mods,
    };
    key(b, &event, out_prevented)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_key(
    b: Option<&NsBrowser>,
    kind: c_int,
    key_name: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
) -> *mut c_char {
    let event = input::KeyEvent {
        kind,
        key: c_str(key_name),
        code: c_str(code),
        keycode,
        mods,
    };
    key(b, &event, None)
}
