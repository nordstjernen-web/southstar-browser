//! Southstar — struct ns_browser with its typed fields and the C ABI of src/libsouthstar.h and src/layers.h.
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

use core::cell::{Cell, UnsafeCell};
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, GPtrArray, GStr};
use southstar_layout::{BoxRef, NsBox};

pub use dom::*;
pub use engine::*;
pub use events::*;
pub use glib::*;
pub use js::{Js, NavigationTiming};
pub use net::*;
pub use paint::*;

use crate::{build, hit, input, open, page, query, render, settle};

#[repr(transparent)]
pub struct Flag(Cell<GBoolean>);

impl Flag {
    pub fn get(&self) -> bool {
        self.0.get() != 0
    }

    pub fn set(&self, value: bool) {
        self.0.set(southstar_glib::boolean(value));
    }
}

#[repr(transparent)]
pub struct Text(Cell<*mut c_char>);

impl Text {
    pub fn get(&self) -> Option<&CStr> {
        c_str(self.0.get())
    }

    pub fn ptr(&self) -> *const c_char {
        self.0.get()
    }

    pub fn is_set(&self) -> bool {
        !self.0.get().is_null()
    }

    pub fn adopt(&self, value: Option<GStr>) {
        let new = value.map_or(ptr::null_mut(), |s| {
            let raw = s.as_ptr().cast_mut();
            core::mem::forget(s);
            raw
        });
        let old = self.0.replace(new);
        unsafe { southstar_glib::g_free(old.cast()) };
    }

    pub fn set(&self, value: Option<&CStr>) {
        let new = value.map_or(ptr::null_mut(), |v| unsafe {
            southstar_glib::g_strdup(v.as_ptr())
        });
        let old = self.0.replace(new);
        unsafe { southstar_glib::g_free(old.cast()) };
    }

    pub fn take(&self) -> Option<GStr> {
        unsafe { GStr::take(self.0.replace(ptr::null_mut())) }
    }

    fn take_raw(&self) -> *mut c_char {
        self.0.replace(ptr::null_mut())
    }

    pub fn clear(&self) {
        self.adopt(None);
    }
}

#[repr(transparent)]
pub struct NodeSlot(Cell<*const NsNode>);

impl NodeSlot {
    pub fn get(&self) -> Option<Node<'_>> {
        unsafe { Node::from_ptr(self.0.get()) }
    }

    pub fn set(&self, node: Option<Node<'_>>) {
        self.0.set(Node::ptr_or_null(node));
    }

    pub fn is(&self, node: Option<Node<'_>>) -> bool {
        ptr::eq(self.0.get(), Node::ptr_or_null(node))
    }
}

#[repr(transparent)]
pub struct BoxSlot(Cell<*const NsBox>);

impl BoxSlot {
    pub fn set(&self, b: Option<BoxRef<'_>>) {
        self.0.set(b.map_or(ptr::null(), |b| b.as_ptr()));
    }
}

#[repr(C)]
struct RawSelection {
    anchor_box: *const NsBox,
    anchor_byte: usize,
    focus_box: *const NsBox,
    focus_byte: usize,
    active: GBoolean,
}

#[repr(C)]
pub struct NsBrowser {
    doc: Cell<*mut NsNode>,
    layout: Cell<*mut NsBox>,
    styles: Cell<*mut GHashTable>,
    js: Cell<*mut c_void>,
    anim: Cell<*mut c_void>,
    images: Cell<*mut c_void>,
    videos: Cell<*mut c_void>,
    css_cache: Cell<*mut GHashTable>,
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
    img_sessions: Cell<*mut GPtrArray>,
    pub image_arrivals_since_layout: Cell<c_uint>,
    pub media_events_source: Cell<c_uint>,
    pub images_arrived_since_layout: Flag,
    pub load_delay_deadline_us: Cell<i64>,
    img_requested: Cell<*mut GHashTable>,
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
    pending_post_body: Text,
    pending_post_len: Cell<usize>,
    pending_post_ct: Text,
    pub caret_byte: Cell<usize>,
    pub sel_anchor_byte: Cell<usize>,
    pub caret_blink_node: NodeSlot,
    pub caret_blink_byte: Cell<usize>,
    pub caret_blink_anchor: Cell<usize>,
    pub caret_blink_epoch_us: Cell<i64>,
    pub caret_blink_active: Flag,
    pub caret_paint_visible: Flag,
    selection: UnsafeCell<RawSelection>,
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

#[cfg(target_pointer_width = "64")]
const _: () = {
    use core::mem::offset_of;
    assert!(core::mem::size_of::<NsBrowser>() == 656);
    assert!(offset_of!(NsBrowser, base_url) == 64 && offset_of!(NsBrowser, vh) == 96);
    assert!(
        offset_of!(NsBrowser, bfcache_ok) == 112 && offset_of!(NsBrowser, cur_viewport_h) == 160
    );
    assert!(
        offset_of!(NsBrowser, pending_scroll) == 176
            && offset_of!(NsBrowser, scroll_anchor_y) == 192
    );
    assert!(offset_of!(NsBrowser, media_events_source) == 212);
    assert!(
        offset_of!(NsBrowser, load_delay_deadline_us) == 224 && offset_of!(NsBrowser, dirty) == 240
    );
    assert!(
        offset_of!(NsBrowser, relaying) == 260 && offset_of!(NsBrowser, soft_nav_pushed) == 272
    );
    assert!(
        offset_of!(NsBrowser, pending_audio) == 304 && offset_of!(NsBrowser, refresh_due_us) == 320
    );
    assert!(
        offset_of!(NsBrowser, pending_post_ct) == 344
            && offset_of!(NsBrowser, caret_blink_node) == 368
    );
    assert!(
        offset_of!(NsBrowser, caret_paint_visible) == 404
            && offset_of!(NsBrowser, selection) == 408
    );
    assert!(
        offset_of!(NsBrowser, selection_dragged) == 448
            && offset_of!(NsBrowser, datalist_suppressed) == 472
    );
    assert!(
        offset_of!(NsBrowser, press_mods) == 496 && offset_of!(NsBrowser, keydown_prevented) == 504
    );
    assert!(offset_of!(NsBrowser, search_case) == 520 && offset_of!(NsBrowser, layout_sig) == 544);
    assert!(offset_of!(NsBrowser, layout_osc) == 560 && offset_of!(NsBrowser, damp_logged) == 584);
    assert!(
        offset_of!(NsBrowser, hover_restyle_pending) == 608
            && offset_of!(NsBrowser, sb_grab) == 632
    );
    assert!(offset_of!(NsBrowser, sb_dragging) == 640 && offset_of!(NsBrowser, security) == 644);
    assert!(offset_of!(NsBrowser, remote_ip) == 648);
};

impl NsBrowser {
    fn allocate() -> &'static NsBrowser {
        let zeroed: NsBrowser = unsafe { core::mem::zeroed() };
        unsafe { &*Box::into_raw(Box::new(zeroed)) }
    }

    pub fn as_ptr(&self) -> *mut NsBrowser {
        ptr::from_ref(self).cast_mut()
    }

    pub fn doc(&self) -> Option<Node<'_>> {
        unsafe { Node::from_ptr(self.doc.get()) }
    }

    pub fn layout(&self) -> Option<BoxRef<'_>> {
        unsafe { BoxRef::from_ptr(self.layout.get()) }
    }

    pub fn js(&self) -> Option<Js> {
        unsafe { Js::from_ptr(self.js.get()) }
    }

    pub fn anim(&self) -> Option<Anim> {
        Anim::from_raw(self.anim.get())
    }

    pub fn images(&self) -> Option<Images> {
        Images::from_raw(self.images.get())
    }

    pub fn videos(&self) -> Option<Videos> {
        Videos::from_raw(self.videos.get())
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

    pub fn selection_clear(&self) {
        unsafe { selection::ns_selection_clear(self.selection.get().cast()) };
    }

    pub fn selection_has_range(&self) -> bool {
        unsafe { selection::ns_selection_has_range(self.selection.get().cast()) != 0 }
    }

    pub fn selection_select_all(&self, layout: BoxRef<'_>) -> bool {
        unsafe {
            selection::ns_selection_select_all(self.selection.get().cast(), layout.as_ptr()) != 0
        }
    }

    pub fn selection_text(&self) -> Option<GStr> {
        unsafe {
            GStr::take(selection::ns_selection_collect_text(
                self.layout.get(),
                self.selection.get().cast(),
            ))
        }
    }

    pub fn selection_bounds(&self) -> (bool, [f64; 4]) {
        let mut r = [0.0f64; 4];
        let [x, y, w, h] = &mut r;
        let ok = unsafe {
            selection::ns_selection_bounds(
                self.layout.get(),
                self.selection.get().cast(),
                x,
                y,
                w,
                h,
            )
        };
        (ok != 0, r)
    }

    pub fn sessions(&self) -> Sessions<'_> {
        Sessions(&self.img_sessions)
    }

    pub fn requested_images(&self) -> *mut GHashTable {
        if self.img_requested.get().is_null() {
            self.img_requested.set(new_string_set());
        }
        self.img_requested.get()
    }

    pub fn set_post(&self, body: StrBufOwned, content_type: &CStr) {
        self.pending_post_body.clear();
        self.pending_post_ct.clear();
        let (raw, len) = body.into_raw();
        self.pending_post_len.set(len);
        self.pending_post_body.0.set(raw);
        self.pending_post_ct.set(Some(content_type));
    }

    fn release(&self) {
        let source = self.media_events_source.get();
        if source != 0 {
            source_remove(source);
        }
        self.sessions().close_all();
        let requested = self.img_requested.replace(ptr::null_mut());
        if !requested.is_null() {
            unsafe { southstar_glib::g_hash_table_destroy(requested) };
        }
        css_set_active_node(None);
        paint_set_anim(None);
        if let Some(js) = self.js() {
            js.set_layout_root(None);
            js.set_style_table(None);
        }
        if let Some(anim) = self.anim() {
            anim.free();
        }
        let layout = self.layout.replace(ptr::null_mut());
        if !layout.is_null() {
            paint_3d_invalidate();
            unsafe { box_free(layout) };
        }
        self.drop_styles();
        let cache = self.css_cache.replace(ptr::null_mut());
        if !cache.is_null() {
            unsafe { southstar_glib::g_hash_table_destroy(cache) };
        }
        if let Some(js) = self.js() {
            js.free();
        }
        let doc = self.doc.replace(ptr::null_mut());
        if !doc.is_null() {
            unsafe { node_free(doc) };
        }
        if let Some(videos) = self.videos() {
            videos.free();
        }
        if let Some(images) = self.images() {
            images.free();
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
            &self.pending_post_body,
            &self.pending_post_ct,
            &self.search_query,
            &self.remote_ip,
        ] {
            text.clear();
        }
        self.console_buf.free();
    }
}

pub struct Sessions<'a>(&'a Cell<*mut GPtrArray>);

impl Sessions<'_> {
    pub fn ensure(&self) {
        if self.0.get().is_null() {
            self.0.set(unsafe { southstar_glib::g_ptr_array_new() });
        }
    }

    pub fn exists(&self) -> bool {
        !self.0.get().is_null()
    }

    pub fn push(&self, session: Session) {
        unsafe { southstar_glib::g_ptr_array_add(self.0.get(), session.0) };
    }

    pub fn len(&self) -> usize {
        unsafe { self.0.get().as_ref() }.map_or(0, |a| a.len as usize)
    }

    pub fn get(&self, i: usize) -> Session {
        Session(unsafe { *(*self.0.get()).pdata.add(i) })
    }

    pub fn remove_fast(&self, i: usize) {
        unsafe { glib::g_ptr_array_remove_index_fast(self.0.get(), i as c_uint) };
    }

    fn close_all(&self) {
        let array = self.0.replace(ptr::null_mut());
        if array.is_null() {
            return;
        }
        let sessions = Sessions(&Cell::new(array)).len();
        for i in 0..sessions {
            Session(unsafe { *(*array).pdata.add(i) }).close();
        }
        unsafe { southstar_glib::g_ptr_array_free(array, southstar_glib::TRUE) };
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
    pub timing: Option<&'a NavigationTimingData>,
}

pub fn create_browser(c: Created<'_>) -> &'static NsBrowser {
    let b = NsBrowser::allocate();
    b.doc.set(c.doc.0);
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
        navigation_timing: c.timing.map_or(ptr::null(), |t| ptr::from_ref(t).cast()),
    };
    build::build(b, setup);
    b
}

pub fn attach_js(b: &NsBrowser, navigation_timing: *const NavigationTiming) -> Option<Js> {
    let js = trampolines::new_js(b, navigation_timing);
    b.js.set(js.map_or(ptr::null_mut(), Js::raw));
    js
}

pub fn attach_caches(b: &NsBrowser) {
    b.css_cache.set(new_css_cache());
    b.images.set(Images::create());
    b.videos.set(Videos::create());
}

pub fn set_anim(b: &NsBrowser, anim: Anim) {
    b.anim.set(anim.raw());
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
    if kind.is_null() {
        return 0;
    }
    bool_int(unsafe { videos.helper_event(token, kind) })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_init() -> c_int {
    build::init();
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_sandbox(self_exe: *const c_char) {
    unsafe { sandbox(self_exe) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_shutdown() {
    shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_render_text(b: *mut NsBrowser) -> *mut c_char {
    let Some(layout) = (unsafe { browser(b) }).and_then(NsBrowser::layout) else {
        return ptr::null_mut();
    };
    let text = StrBufOwned::new();
    dump_text(layout, &text);
    malloc_dup(text.bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_dump_dom(b: *mut NsBrowser) -> *mut c_char {
    match (unsafe { browser(b) }).and_then(NsBrowser::doc) {
        Some(doc) => node_dump(doc),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_dump_layout(b: *mut NsBrowser) -> *mut c_char {
    let Some(layout) = (unsafe { browser(b) }).and_then(NsBrowser::layout) else {
        return ptr::null_mut();
    };
    let out = StrBufOwned::new();
    dump_layout(layout, &out);
    out.into_text()
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
        (Some(b), Some(path)) if b.layout().is_some() => query::render_image(b, path),
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
    if b.doc().is_none() {
        return ptr::null_mut();
    }
    let (pages, setup) = query::print_pages(b);
    *out = setup;
    pages
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
    let Some(b) = (unsafe { browser(b) }).filter(|b| b.pending_post_body.is_set()) else {
        return ptr::null_mut();
    };
    let body = b.pending_post_body.take_raw();
    if let Some(out) = unsafe { out_len.as_mut() } {
        *out = b.pending_post_len.get();
    }
    b.pending_post_len.set(0);
    match unsafe { out_ct.as_mut() } {
        Some(out) => *out = b.pending_post_ct.take_raw(),
        None => b.pending_post_ct.clear(),
    }
    body
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
    match unsafe { browser(b) } {
        Some(b) if b.soft_nav_pushed.get() => {
            b.soft_nav_pushed.set(false);
            1
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_scroll(
    b: *mut NsBrowser,
    out_scroll_x: *mut c_int,
    out_scroll_y: *mut c_int,
) -> c_int {
    let Some(b) = (unsafe { browser(b) }).filter(|b| b.pending_scroll.get()) else {
        return 0;
    };
    unsafe {
        if let Some(out) = out_scroll_x.as_mut() {
            *out = b.pending_scroll_x.get();
        }
        if let Some(out) = out_scroll_y.as_mut() {
            *out = b.pending_scroll_y.get();
        }
    }
    b.pending_scroll_x.set(-1);
    b.pending_scroll.set(false);
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
    webgl_take_pending_origin()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_has_pending_clipboard(b: *mut NsBrowser) -> c_int {
    unsafe { browser(b) }.map_or(0, |b| bool_int(b.pending_clipboard.is_set()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_clipboard(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_clipboard.take_raw())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_download(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_download.take_raw())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_take_pending_window_action(b: *mut NsBrowser) -> *mut c_char {
    unsafe { browser(b) }.map_or(ptr::null_mut(), |b| b.pending_window_action.take_raw())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_webgl(
    _b: *mut NsBrowser,
    origin: *const c_char,
    allow: c_int,
) {
    unsafe { webgl_set_decision(origin, allow) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_take_pending_camera(_b: *mut NsBrowser) -> *mut c_char {
    camera_take_pending_origin()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_resolve_camera(
    b: *mut NsBrowser,
    origin: *const c_char,
    allow: c_int,
) {
    unsafe { camera_set_decision(origin, allow) };
    if let Some(js) = (unsafe { browser(b) }).and_then(NsBrowser::js) {
        let src = if allow != 0 {
            c"__nd_camera_resolve_pending(true)"
        } else {
            c"__nd_camera_resolve_pending(false)"
        };
        drop(js.eval(src, c"camera-resolve"));
    }
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
    if let Some(js) = (unsafe { browser(b) }).and_then(NsBrowser::js) {
        js.fire_page_transition(c"pagehide", true);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_bfcache_restore(
    b: *mut NsBrowser,
    viewport_width: c_int,
    viewport_height: f64,
) {
    let Some(browser_ref) = (unsafe { browser(b) }) else {
        return;
    };
    if viewport_width > 0 && viewport_height > 0.0 {
        page::set_viewport(browser_ref, viewport_width, viewport_height);
    }
    if let Some(js) = browser_ref.js() {
        js.fire_page_transition(c"pageshow", true);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_busy(b: *const NsBrowser) -> c_int {
    let Some(b) = (unsafe { b.as_ref() }) else {
        return 0;
    };
    if engine_in_blocking_fetch() {
        return 1;
    }
    bool_int(b.js().is_some_and(Js::in_pump))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_close(b: *mut NsBrowser) {
    let Some(browser_ref) = (unsafe { b.as_ref() }) else {
        return;
    };
    browser_ref.release();
    drop(unsafe { Box::from_raw(b) });
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

unsafe fn open_with(
    url: *const c_char,
    view: open::Viewport,
    post: Option<&Post<'_>>,
) -> *mut NsBrowser {
    c_str(url)
        .and_then(|url| open::open(url, &view, post))
        .map_or(ptr::null_mut(), NsBrowser::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_set_next_referrer(url: *const c_char) {
    open::set_next_referrer(c_str(url));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_next_user_activated(user_activated: c_int) {
    open::set_next_user_activated(user_activated != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_browser_set_color_scheme(dark: c_int) {
    css_set_color_scheme(dark != 0);
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
    unsafe { open_with(url, view(viewport_width, 0.0, settle_ms), None) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_open_viewport(
    url: *const c_char,
    viewport_width: c_int,
    viewport_height: f64,
    settle_ms: c_int,
) -> *mut NsBrowser {
    unsafe { open_with(url, view(viewport_width, viewport_height, settle_ms), None) }
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
    let post = (!body.is_null()).then(|| Post {
        body,
        len: body_len,
        content_type: c_str(content_type),
    });
    unsafe {
        open_with(
            url,
            view(viewport_width, viewport_height, settle_ms),
            post.as_ref(),
        )
    }
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
    let Some(b) = (unsafe { browser(b) }).filter(|b| b.layout().is_some()) else {
        return;
    };
    b.set_video_page_coords(true);
    render::note_viewport(
        b,
        scroll_x,
        scroll_y,
        height,
        if scale > 0.0 { scale } else { 1.0 },
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_flush_video_rects(b: *mut NsBrowser) {
    if let Some(b) = (unsafe { browser(b) }).filter(|b| b.videos().is_some()) {
        b.flush_video_composites(monotonic_us());
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
    match (unsafe { browser(b) }, unsafe { Plan::from_ptr(plan) }) {
        (Some(b), Some(plan)) if b.layout().is_some() && width > 0 && height > 0 => {
            render::layers_prepare(b, scroll_x, scroll_y, width, height, scale, &plan)
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
    let (Some(b), Some(plan)) = (unsafe { browser(b) }, unsafe { Plan::from_ptr(plan) }) else {
        return -1;
    };
    if bufs.is_null() || b.layout().is_none() || width <= 0 || height <= 0 || stride < width * 4 {
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
        scale: if scale > 0.0 { scale } else { 1.0 },
    };
    render::render_doc_tile(b, &plan, &target)
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
        Some(b) => render::vp_layer_info(b, unsafe { Plan::from_ptr(plan) }.as_ref(), index, out),
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
    let (Some(b), Some(plan)) = (unsafe { browser(b) }, unsafe { Plan::from_ptr(plan) }) else {
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
    render::render_vp_layer(b, &plan, &r, &unsafe {
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
    match unsafe { browser(b) } {
        Some(b) if b.layout().is_some() => input::hover(b, x, y),
        _ => -1,
    }
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
    if consumed != 0 {
        if let Some(out) = unsafe { out_snapped.as_mut() } {
            *out = bool_int(snapped);
        }
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
        (Some(b), Some(src)) => input::eval(b, src),
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
    let Some(b) = (unsafe { browser(b) }).filter(|b| b.layout().is_some()) else {
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
    match unsafe { browser(b) } {
        Some(b) if b.layout().is_some() => take_or_null(input::press(b, x, y, mods)),
        _ => ptr::null_mut(),
    }
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
    if changed {
        if let Some(out) = unsafe { out_changed.as_mut() } {
            *out = 1;
        }
    }
    take_or_null(nav)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_browser_release(b: *mut NsBrowser) -> c_int {
    let mut changed = 0;
    let nav = unsafe { ns_browser_release_click(b, &mut changed) };
    unsafe { southstar_glib::g_free(nav.cast()) };
    changed
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
    if out.is_null() || unsafe { *out } == 0 {
        if let Some(b) = unsafe { browser(b) } {
            hit::video_click_toggle(b, x, y);
        }
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
    if prevented {
        if let Some(out) = unsafe { out_prevented.as_mut() } {
            *out = 1;
        }
    }
    take_or_null(nav)
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
