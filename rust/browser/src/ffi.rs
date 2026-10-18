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
use core::mem::{MaybeUninit, offset_of};
use core::ptr;
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, Tree, WeakNode};
use southstar_engine::CssCache;
use southstar_glib::{GBoolean, GHashTable, GPtrArray, GStr};
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

#[repr(transparent)]
pub struct Flag(Cell<GBoolean>);

impl Flag {
    pub fn get(&self) -> bool {
        self.0.get() != 0
    }

    pub fn set(&self, value: bool) {
        self.0.set(southstar_glib::boolean(value));
    }

    pub fn take(&self) -> bool {
        self.0.replace(southstar_glib::FALSE) != 0
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

#[repr(transparent)]
pub struct BoxSlot(Cell<*const NsBox>);

impl BoxSlot {
    pub fn set(&self, b: Option<BoxRef<'_>>) {
        self.0.set(b.map_or(ptr::null(), |b| b.as_ptr()));
    }
}

pub struct NsBrowser {
    doc: RefCell<Option<Tree>>,
    layout: Cell<*mut NsBox>,
    styles: Cell<*mut GHashTable>,
    js: Cell<*mut c_void>,
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
    selection: Cell<NsSelection>,
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
    sb_box: Cell<*mut NsBox>,
    pub sb_node: NodeSlot,
    pub sb_grab: Cell<f64>,
    pub sb_dragging: Flag,
    pub security: Cell<c_int>,
    pub remote_ip: Text,
}

const TEXT_FIELDS: [usize; 11] = [
    offset_of!(NsBrowser, base_url),
    offset_of!(NsBrowser, doc_charset),
    offset_of!(NsBrowser, doc_language),
    offset_of!(NsBrowser, pending_nav),
    offset_of!(NsBrowser, pending_download),
    offset_of!(NsBrowser, pending_clipboard),
    offset_of!(NsBrowser, pending_window_action),
    offset_of!(NsBrowser, refresh_url),
    offset_of!(NsBrowser, pending_post_ct),
    offset_of!(NsBrowser, search_query),
    offset_of!(NsBrowser, remote_ip),
];

const NODE_FIELDS: [usize; 6] = [
    offset_of!(NsBrowser, scroll_anchor),
    offset_of!(NsBrowser, caret_blink_node),
    offset_of!(NsBrowser, hover_node),
    offset_of!(NsBrowser, open_select),
    offset_of!(NsBrowser, press_node),
    offset_of!(NsBrowser, sb_node),
];

impl NsBrowser {
    fn allocate() -> Page {
        let mut slot = Box::new(MaybeUninit::<NsBrowser>::zeroed());
        let raw = slot.as_mut_ptr();
        for offset in TEXT_FIELDS {
            let field = raw.wrapping_byte_add(offset).cast::<Text>();
            // SAFETY: `field` is the aligned address of one of the Text fields inside the allocation.
            unsafe { field.write(Text::default()) };
        }
        for offset in NODE_FIELDS {
            let field = raw.wrapping_byte_add(offset).cast::<NodeSlot>();
            // SAFETY: `field` is the aligned address of one of the NodeSlot fields inside the allocation.
            unsafe { field.write(NodeSlot::default()) };
        }
        let body = raw
            .wrapping_byte_add(offset_of!(NsBrowser, pending_post_body))
            .cast::<RefCell<Option<Vec<u8>>>>();
        // SAFETY: `body` is the aligned address of the POST body field inside the allocation.
        unsafe { body.write(RefCell::new(None)) };
        let doc = raw
            .wrapping_byte_add(offset_of!(NsBrowser, doc))
            .cast::<RefCell<Option<Tree>>>();
        // SAFETY: `doc` is the aligned address of the document field inside the allocation.
        unsafe { doc.write(RefCell::new(None)) };
        let sessions = raw
            .wrapping_byte_add(offset_of!(NsBrowser, img_sessions))
            .cast::<RefCell<Vec<Session>>>();
        // SAFETY: `sessions` is the aligned address of the image session field inside the allocation.
        unsafe { sessions.write(RefCell::new(Vec::new())) };
        let requested = raw
            .wrapping_byte_add(offset_of!(NsBrowser, img_requested))
            .cast::<RefCell<HashSet<CString>>>();
        // SAFETY: `requested` is the aligned address of the requested-image field inside the allocation.
        unsafe { requested.write(RefCell::new(HashSet::new())) };
        // SAFETY: the Text, NodeSlot, POST body, document, image session and requested-image fields were written above; every other field is a cell of integers, floats, raw pointers or optional handles around a non-null pointer, for which all-zero bytes are a valid value.
        Page::adopt(unsafe { slot.assume_init() })
    }

    pub fn as_ptr(&self) -> *mut NsBrowser {
        ptr::from_ref(self).cast_mut()
    }

    pub fn doc(&self) -> Option<Node<'_>> {
        self.doc.borrow().as_ref().map(Tree::root)
    }

    pub fn layout(&self) -> Option<BoxRef<'_>> {
        unsafe { BoxRef::from_ptr(self.layout.get()) }
    }

    pub fn js(&self) -> Option<Js> {
        unsafe { Js::from_ptr(self.js.get()) }
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
        let layout = self.layout.replace(ptr::null_mut());
        if !layout.is_null() {
            paint_3d_invalidate();
            unsafe { box_free(layout) };
        }
        self.sb_box.set(ptr::null_mut());
        self.sb_node.set(None);
        self.sb_dragging.set(false);
    }

    pub fn drop_styles(&self) {
        let styles = self.styles.replace(ptr::null_mut());
        if !styles.is_null() {
            unsafe { southstar_glib::g_hash_table_destroy(styles) };
        }
    }

    fn selection<'a>(&'a self) -> Selection<'a> {
        // SAFETY: the page clears its selection before it frees or replaces the layout, so the boxes it names belong to the live layout tree.
        unsafe { self.selection.get().load() }
    }

    pub fn update_selection<'a>(&'a self, f: impl FnOnce(&mut Selection<'a>) -> bool) -> bool {
        let mut selection = self.selection();
        let changed = f(&mut selection);
        self.selection.set(NsSelection::store(&selection));
        changed
    }

    pub fn selection_clear(&self) {
        self.selection.set(NsSelection::EMPTY);
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
        self.pending_post_body.replace(Some(body.bytes().to_vec()));
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
        let layout = self.layout.replace(ptr::null_mut());
        if !layout.is_null() {
            paint_3d_invalidate();
            unsafe { box_free(layout) };
        }
        self.drop_styles();
        if let Some(cache) = self.css_cache.take() {
            // SAFETY: the cache is out of the page's slot and the engine holds no copy between calls.
            unsafe { cache.free() };
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
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

unsafe fn browser<'a>(b: *mut NsBrowser) -> Option<&'a NsBrowser> {
    unsafe { b.as_ref() }
}

fn bool_int(value: bool) -> c_int {
    c_int::from(value)
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
    let page = NsBrowser::allocate();
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
    b.js.set(js.map_or(ptr::null_mut(), Js::raw));
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
pub unsafe extern "C" fn ns_browser_take_pending_audio(b: *mut NsBrowser) -> *mut c_char {
    match unsafe { browser(b) } {
        Some(b) => b.pending_audio.take_text(),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_video_helper_event(
    b: *mut NsBrowser,
    token: *const c_char,
    kind: *const c_char,
) -> c_int {
    let Some(videos) = (unsafe { browser(b) }).and_then(NsBrowser::videos) else {
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
pub unsafe extern "C" fn ns_browser_render_text(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(query::dump_text) {
        Some(text) => malloc_dup(text.bytes()),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_dump_dom(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(query::dump_dom) {
        Some(dom) => southstar_glib::strdup(&dom),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_dump_layout(b: *mut NsBrowser) -> *mut c_char {
    (unsafe { browser(b) })
        .and_then(query::dump_layout)
        .map_or(ptr::null_mut(), StrBufOwned::into_text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_dump_performance(b: *mut NsBrowser) -> *mut c_char {
    match unsafe { browser(b) } {
        Some(b) => query::dump_performance(b).into_text(),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_image(b: *mut NsBrowser, path: *const c_char) -> c_int {
    match (unsafe { browser(b) }, c_str(path)) {
        (Some(b), Some(path)) => query::render_image(b, path),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_print_pages(
    b: *mut NsBrowser,
    out_setup: *mut PrintSetup,
) -> *mut GPtrArray {
    let (Some(b), Some(out)) = (unsafe { browser(b) }, unsafe { out_setup.as_mut() }) else {
        return ptr::null_mut();
    };
    let Some((pages, setup)) = query::print_pages(b) else {
        return ptr::null_mut();
    };
    *out = setup;
    pages.into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_tick(b: *mut NsBrowser, budget_ms: c_int) -> c_int {
    match unsafe { browser(b) } {
        Some(b) => bool_int(settle::tick(b, budget_ms)),
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_animating(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(settle::animating(b)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_viewport(
    b: *mut NsBrowser,
    css_width: c_int,
    css_height: f64,
) -> c_int {
    match unsafe { browser(b) } {
        Some(b) => page::set_viewport(b, css_width, css_height),
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_device_pixel_ratio(b: *mut NsBrowser, dppx: f64) -> c_int {
    page::set_device_pixel_ratio(unsafe { browser(b) }, dppx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_viewport_width(
    b: *mut NsBrowser,
    css_width: c_int,
) -> c_int {
    unsafe { ns_browser_set_viewport(b, css_width, f64::from(css_width) * 0.75) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_window_action_applied(b: *mut NsBrowser) {
    if let Some(js) = (unsafe { browser(b) }).and_then(NsBrowser::js) {
        js.window_action_applied();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_page_size(
    b: *mut NsBrowser,
    out_width: *mut c_int,
    out_height: *mut c_int,
) -> c_int {
    let Some((w, h)) = (unsafe { browser(b) }).and_then(page::page_size) else {
        return -1;
    };
    unsafe {
        if let Some(out) = out_width.as_mut() {
            *out = w;
        }
        if let Some(out) = out_height.as_mut() {
            *out = h;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_console_drain(b: *mut NsBrowser) -> *mut c_char {
    match unsafe { browser(b) } {
        Some(b) => b.console_buf.take_text(),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_post(
    b: *mut NsBrowser,
    out_len: *mut usize,
    out_ct: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        if let Some(out) = out_len.as_mut() {
            *out = 0;
        }
        if let Some(out) = out_ct.as_mut() {
            *out = ptr::null_mut();
        }
    }
    let Some((body, content_type)) = (unsafe { browser(b) }).and_then(NsBrowser::take_post) else {
        return ptr::null_mut();
    };
    if let Some(out) = unsafe { out_len.as_mut() } {
        *out = body.len();
    }
    if let Some(out) = unsafe { out_ct.as_mut() } {
        *out = malloc_or_null(content_type);
    }
    malloc_dup(&body)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_focused_editable(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(query::focused_editable(b)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_caret_blink_active(
    b: *mut NsBrowser,
    active: c_int,
) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| {
        bool_int(query::set_caret_blink_active(b, active != 0))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_caret_blinking(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(b.caret_blink_active.get()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_focused_editable_value(
    b: *mut NsBrowser,
    out_caret: *mut usize,
    out_anchor: *mut usize,
) -> *mut c_char {
    unsafe {
        if let Some(out) = out_caret.as_mut() {
            *out = 0;
        }
        if let Some(out) = out_anchor.as_mut() {
            *out = 0;
        }
    }
    let Some((value, caret, anchor)) =
        (unsafe { browser(b) }).and_then(query::focused_editable_value)
    else {
        return ptr::null_mut();
    };
    unsafe {
        if let Some(out) = out_caret.as_mut() {
            *out = caret;
        }
        if let Some(out) = out_anchor.as_mut() {
            *out = anchor;
        }
    }
    malloc_dup(&value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_focused_editable_selection(
    b: *mut NsBrowser,
    caret: usize,
    anchor: usize,
) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| {
        bool_int(query::set_focused_editable_selection(b, caret, anchor))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_title(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(query::title) {
        Some(title) => malloc_dup(&title),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_url(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(|b| b.base_url.get()) {
        Some(url) => malloc_dup(url.to_bytes()),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_security(
    b: *mut NsBrowser,
    out_ip: *mut *const c_char,
) -> c_int {
    let b = unsafe { browser(b) };
    if let Some(out) = unsafe { out_ip.as_mut() } {
        *out = b.map_or(ptr::null(), |b| b.remote_ip.ptr());
    }
    b.map_or(0, |b| b.security.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_nav(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(|b| b.pending_nav.take()) {
        Some(nav) => malloc_dup(nav.to_bytes()),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_soft_nav_pushed(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(b.soft_nav_pushed.take()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_scroll(
    b: *mut NsBrowser,
    out_scroll_x: *mut c_int,
    out_scroll_y: *mut c_int,
) -> c_int {
    let Some((x, y)) = (unsafe { browser(b) }).and_then(scroll::take_pending) else {
        return 0;
    };
    unsafe {
        if let Some(out) = out_scroll_x.as_mut() {
            *out = x;
        }
        if let Some(out) = out_scroll_y.as_mut() {
            *out = y;
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_scroll_y(
    b: *mut NsBrowser,
    out_scroll_y: *mut c_int,
) -> c_int {
    unsafe { ns_browser_take_pending_scroll(b, ptr::null_mut(), out_scroll_y) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_webgl(_b: *mut NsBrowser) -> *mut c_char {
    webgl_take_pending_origin().map_or(ptr::null_mut(), |origin| southstar_glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_has_pending_clipboard(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(b.pending_clipboard.is_set()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_clipboard(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_clipboard.take_malloc())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_download(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_download.take_malloc())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_window_action(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_window_action.take_malloc())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_webgl(
    _b: *mut NsBrowser,
    origin: *const c_char,
    allow: c_int,
) {
    if let Some(origin) = c_str(origin) {
        webgl_set_decision(origin, allow != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_camera(_b: *mut NsBrowser) -> *mut c_char {
    southstar_camera::take_pending_origin()
        .map_or(ptr::null_mut(), |origin| southstar_glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_camera(
    b: *mut NsBrowser,
    origin: *const c_char,
    allow: c_int,
) {
    callbacks::resolve_camera(unsafe { browser(b) }, c_str(origin), allow != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_links(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(query::links) {
        Some(links) => malloc_dup(&links),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_favicon_url(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(query::favicon_url) {
        Some(url) => malloc_dup(&url),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_bfcache_eligible(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(b.bfcache_ok.get()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_bfcache_park(b: *mut NsBrowser) {
    if let Some(b) = unsafe { browser(b) } {
        page::bfcache_park(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_bfcache_restore(
    b: *mut NsBrowser,
    viewport_width: c_int,
    viewport_height: f64,
) {
    if let Some(b) = unsafe { browser(b) } {
        page::bfcache_restore(b, viewport_width, viewport_height);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_busy(b: *const NsBrowser) -> c_int {
    unsafe { b.as_ref() }.map_or(0, |b| bool_int(settle::busy(b)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_close(b: *mut NsBrowser) {
    // SAFETY: the embedder hands back a page an ns_browser_open call returned, which it no longer uses.
    drop(unsafe { Page::from_raw(b) });
}

pub fn browser_for(ud: *mut c_void) -> Option<&'static NsBrowser> {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_rgba(
    b: *mut NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    out: *mut u8,
    stride: c_int,
) -> c_int {
    match unsafe { browser(b) } {
        Some(b) if !out.is_null() => render::render_rgba(b, scroll_x, scroll_y, scale, &unsafe {
            PixelBuf::new(out, width, height, stride)
        }),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_argb32(
    b: *mut NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    out: *mut u8,
    stride: c_int,
) -> c_int {
    match unsafe { browser(b) } {
        Some(b) if !out.is_null() => render::render_argb32(b, scroll_x, scroll_y, scale, &unsafe {
            PixelBuf::new(out, width, height, stride)
        }),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_snap_document(
    b: *mut NsBrowser,
    viewport_w: f64,
    viewport_h: f64,
    prev_x: c_int,
    prev_y: c_int,
    scroll_x: *mut c_int,
    scroll_y: *mut c_int,
) -> c_int {
    let (Some(b), Some(sx), Some(sy)) = (
        unsafe { browser(b) },
        unsafe { scroll_x.as_mut() },
        unsafe { scroll_y.as_mut() },
    ) else {
        return 0;
    };
    match render::snap_document(b, (viewport_w, viewport_h), (prev_x, prev_y), (*sx, *sy)) {
        Some((x, y)) => {
            *sx = x;
            *sy = y;
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_note_viewport(
    b: *mut NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    height: c_int,
    scale: f64,
) {
    if let Some(b) = unsafe { browser(b) } {
        render::note_composited_viewport(b, scroll_x, scroll_y, height, scale);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_flush_video_rects(b: *mut NsBrowser) {
    if let Some(b) = unsafe { browser(b) } {
        render::flush_video_rects(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_layers_prepare(
    b: *mut NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    plan: *mut LayerPlan,
) -> c_int {
    match (unsafe { browser(b) }, unsafe { plan.as_mut() }) {
        (Some(b), Some(plan)) => {
            render::layers_prepare(b, scroll_x, scroll_y, width, height, scale, plan)
        }
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_doc_tile(
    b: *mut NsBrowser,
    plan: *const LayerPlan,
    scroll_x: c_int,
    tile_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    bufs: *const *mut u8,
    stride: c_int,
    upper_used: *mut GBoolean,
) -> c_int {
    let (Some(b), Some(plan)) = (unsafe { browser(b) }, unsafe { plan.as_ref() }) else {
        return -1;
    };
    if bufs.is_null() {
        return -1;
    }
    let target = TileTarget {
        bufs,
        used: upper_used,
        width,
        height,
        stride,
        scroll_x,
        tile_y,
        scale,
    };
    render::render_doc_tile(b, plan, target)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_vp_layer_info(
    b: *mut NsBrowser,
    plan: *const LayerPlan,
    index: c_int,
    out: *mut VpLayerInfo,
) -> c_int {
    let out = unsafe { &mut *out };
    *out = VpLayerInfo::default();
    match unsafe { browser(b) } {
        Some(b) => render::vp_layer_info(b, unsafe { plan.as_ref() }, index, out),
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_vp_layer(
    b: *mut NsBrowser,
    plan: *const LayerPlan,
    index: c_int,
    scroll_x: c_int,
    scroll_y: c_int,
    origin_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    out: *mut u8,
    stride: c_int,
) -> c_int {
    let (Some(b), Some(plan)) = (unsafe { browser(b) }, unsafe { plan.as_ref() }) else {
        return -1;
    };
    if out.is_null() {
        return -1;
    }
    let r = render::VpRender {
        index,
        scroll_x,
        scroll_y,
        origin_y,
        scale,
    };
    render::render_vp_layer(b, plan, &r, &unsafe {
        PixelBuf::new(out, width, height, stride)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scroller_rects(
    b: *mut NsBrowser,
    out: *mut GString,
    max_rects: c_int,
) {
    if out.is_null() {
        return;
    }
    if let Some(rects) = (unsafe { browser(b) }).and_then(|b| render::scroller_rects(b, max_rects))
    {
        unsafe { append_to(out, &rects) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_canvas_color(
    b: *mut NsBrowser,
    rgba_out: *mut f64,
) -> GBoolean {
    let Some(layout) = (unsafe { browser(b) }).and_then(NsBrowser::layout) else {
        return 0;
    };
    let mut rgba = [0.0f64; 4];
    unsafe { ptr::copy_nonoverlapping(rgba_out, rgba.as_mut_ptr(), 4) };
    let ok = canvas_color(layout, &mut rgba);
    unsafe { ptr::copy_nonoverlapping(rgba.as_ptr(), rgba_out, 4) };
    southstar_glib::boolean(ok)
}

fn gstrdup(bytes: &[u8]) -> *mut c_char {
    take_or_null(gstr_from(bytes))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_link_at(b: *mut NsBrowser, x: c_int, y: c_int) -> *mut c_char {
    take_or_null(unsafe { browser(b) }.and_then(|b| hit::link_near(b, x, y, 9)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_link_under(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    take_or_null(unsafe { browser(b) }.and_then(|b| hit::link_near(b, x, y, 1)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_cursor_at(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    match unsafe { browser(b) }.and_then(|b| hit::cursor_at(b, x, y)) {
        Some(cursor) => gstrdup(cursor),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_select(
    b: *mut NsBrowser,
    kind: c_int,
    x: c_int,
    y: c_int,
) -> *mut c_char {
    match unsafe { browser(b) }.and_then(|b| input::select(b, kind, x, y)) {
        Some(input::Selected::Text(text)) => gstrdup(&text),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_hover(b: *mut NsBrowser, x: c_int, y: c_int) -> c_int {
    unsafe { browser(b) }.map_or(-1, |b| input::hover(b, x, y))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scroll_at(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
) -> c_int {
    unsafe { ns_browser_scroll_at_full(b, x, y, dx, dy, ptr::null_mut()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scroll_at_full(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    dx: c_int,
    dy: c_int,
    out_snapped: *mut c_int,
) -> c_int {
    if let Some(out) = unsafe { out_snapped.as_mut() } {
        *out = 0;
    }
    let Some(b) = (unsafe { browser(b) }) else {
        return 0;
    };
    let (consumed, snapped) = input::scroll_at(b, x, y, dx, dy);
    if consumed != 0
        && let Some(out) = unsafe { out_snapped.as_mut() }
    {
        *out = bool_int(snapped);
    }
    consumed
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scrollbar_press(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| input::scrollbar_press(b, x, y))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scrollbar_drag(
    b: *mut NsBrowser,
    _x: c_int,
    y: c_int,
) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| input::scrollbar_drag(b, y))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_scrollbar_release(b: *mut NsBrowser) {
    if let Some(b) = unsafe { browser(b) } {
        input::scrollbar_release(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_drop_files(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    paths: *const *const c_char,
    n_paths: c_int,
) -> c_int {
    let Some(b) = (unsafe { browser(b) }) else {
        return 0;
    };
    if paths.is_null() || n_paths <= 0 {
        return 0;
    }
    let list: Vec<&CStr> = (0..n_paths as usize)
        .filter_map(|i| c_str(unsafe { *paths.add(i) }))
        .collect();
    input::drop_files(b, x, y, &list)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_eval(b: *mut NsBrowser, src: *const c_char) -> *mut c_char {
    match (unsafe { browser(b) }, c_str(src)) {
        (Some(b), Some(src)) => take_or_null(input::eval(b, src)),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_contextmenu_full(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    out_edit: *mut c_int,
) -> c_int {
    if let Some(out) = unsafe { out_edit.as_mut() } {
        *out = 0;
    }
    let Some(b) = (unsafe { browser(b) }) else {
        return 0;
    };
    let (prevented, edit_state) = input::contextmenu(b, x, y);
    if let Some(out) = unsafe { out_edit.as_mut() } {
        *out = edit_state;
    }
    prevented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_contextmenu(b: *mut NsBrowser, x: c_int, y: c_int) -> c_int {
    unsafe { ns_browser_contextmenu_full(b, x, y, ptr::null_mut()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_media_at(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    out_is_video: *mut c_int,
    out_stream: *mut c_int,
) -> *mut c_char {
    unsafe {
        if let Some(out) = out_is_video.as_mut() {
            *out = 0;
        }
        if let Some(out) = out_stream.as_mut() {
            *out = 0;
        }
    }
    let Some(found) = (unsafe { browser(b) }).and_then(|b| hit::media_at(b, x, y)) else {
        return ptr::null_mut();
    };
    unsafe {
        if let Some(out) = out_is_video.as_mut() {
            *out = bool_int(found.is_video);
        }
        if let Some(out) = out_stream.as_mut() {
            *out = bool_int(found.stream);
        }
    }
    take_or_null(Some(found.url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_find(
    b: *mut NsBrowser,
    query: *const c_char,
    case_sensitive: c_int,
    direction: c_int,
    from_y: c_int,
    out_total: *mut c_int,
    out_current: *mut c_int,
    out_y: *mut c_int,
) -> c_int {
    let outs = [out_total, out_current, out_y];
    for out in outs {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = 0;
        }
    }
    let Some(found) = (unsafe { browser(b) })
        .and_then(|b| hit::find(b, c_str(query), case_sensitive != 0, direction, from_y))
    else {
        return -1;
    };
    for (out, value) in outs.into_iter().zip([found.total, found.current, found.y]) {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = value;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_press(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    mods: c_int,
) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| {
        malloc_or_null(input::press(b, x, y, mods))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_release_click(
    b: *mut NsBrowser,
    out_changed: *mut c_int,
) -> *mut c_char {
    let Some(b) = (unsafe { browser(b) }) else {
        if let Some(out) = unsafe { out_changed.as_mut() } {
            *out = -1;
        }
        return ptr::null_mut();
    };
    if let Some(out) = unsafe { out_changed.as_mut() } {
        *out = 0;
    }
    let (nav, changed) = input::release_click(b);
    if changed && let Some(out) = unsafe { out_changed.as_mut() } {
        *out = 1;
    }
    malloc_or_null(nav)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_release(b: *mut NsBrowser) -> c_int {
    match unsafe { browser(b) } {
        Some(b) => bool_int(input::release_click(b).1),
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_click(
    b: *mut NsBrowser,
    x: c_int,
    y: c_int,
    mods: c_int,
) -> *mut c_char {
    let nav = unsafe { ns_browser_press(b, x, y, mods) };
    if !nav.is_null() && unsafe { *nav } != 0 {
        if let Some(b) = unsafe { browser(b) } {
            b.press_node.set(None);
            b.press_active.set(false);
            css_set_active_node(None);
        }
        return nav;
    }
    unsafe { southstar_glib::g_free(nav.cast()) };
    let out = unsafe { ns_browser_release_click(b, ptr::null_mut()) };
    if (out.is_null() || unsafe { *out } == 0)
        && let Some(b) = unsafe { browser(b) }
    {
        hit::video_click_toggle(b, x, y);
    }
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_key_full(
    b: *mut NsBrowser,
    kind: c_int,
    key: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
    out_prevented: *mut c_int,
) -> *mut c_char {
    if let Some(out) = unsafe { out_prevented.as_mut() } {
        *out = 0;
    }
    let Some(b) = (unsafe { browser(b) }) else {
        return ptr::null_mut();
    };
    let ev = input::KeyEvent {
        kind,
        key: c_str(key),
        code: c_str(code),
        keycode,
        mods,
    };
    let (nav, prevented) = input::key(b, &ev);
    if prevented && let Some(out) = unsafe { out_prevented.as_mut() } {
        *out = 1;
    }
    malloc_or_null(nav)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_key(
    b: *mut NsBrowser,
    kind: c_int,
    key: *const c_char,
    code: *const c_char,
    keycode: c_int,
    mods: c_int,
) -> *mut c_char {
    unsafe { ns_browser_key_full(b, kind, key, code, keycode, mods, ptr::null_mut()) }
}
