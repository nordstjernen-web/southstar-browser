//! Southstar — the script engine calls of the in-process run and the callbacks it and the video cache make back into the driver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable, GStr};
use southstar_layout::NsBox;

use super::engine::{Anim, ImageCache, VideoCache};
use crate::inproc::{Ctx, NavCapture};

#[repr(C)]
#[derive(Default)]
pub struct NavigationTiming {
    pub origin_us: i64,
    pub origin_real_ms: f64,
    pub domain_lookup_start_ms: f64,
    pub domain_lookup_end_ms: f64,
    pub connect_start_ms: f64,
    pub connect_end_ms: f64,
    pub secure_connection_start_ms: f64,
    pub request_start_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
    pub dom_loading_ms: f64,
    pub dom_interactive_ms: f64,
    pub dom_content_loaded_event_start_ms: f64,
    pub dom_content_loaded_event_end_ms: f64,
    pub dom_complete_ms: f64,
    pub load_event_start_ms: f64,
    pub load_event_end_ms: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<NavigationTiming>() == 136);

type LogCb = unsafe extern "C" fn(line: *const c_char, ud: *mut c_void);
type MutatedCb = unsafe extern "C" fn(ud: *mut c_void);
type NavigateCb = unsafe extern "C" fn(url: *const c_char, reload: GBoolean, ud: *mut c_void);
type FormSubmitCb =
    unsafe extern "C" fn(form: *const NsNode, submitter: *const NsNode, ud: *mut c_void);
type FlushCb = unsafe extern "C" fn(ud: *mut c_void);
type MseCb = unsafe extern "C" fn(
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
    eos: GBoolean,
    ud: *mut c_void,
) -> GBoolean;
type MseBufferedCb =
    unsafe extern "C" fn(stream_id: c_uint, kind: c_char, start: *mut f64, ud: *mut c_void) -> f64;
type MseRemoveCb = unsafe extern "C" fn(
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    ud: *mut c_void,
) -> GBoolean;
type MseBytesCb = unsafe extern "C" fn(stream_id: c_uint, kind: c_char, ud: *mut c_void) -> usize;
type VideoJsCb =
    unsafe extern "C" fn(node: *const c_void, kind: *const c_char, value: f64, ud: *mut c_void);

unsafe extern "C" {
    fn ns_js_new(
        log_cb: LogCb,
        log_ud: *mut c_void,
        mut_cb: MutatedCb,
        mut_ud: *mut c_void,
        nav_cb: NavigateCb,
        nav_ud: *mut c_void,
        timing: *const NavigationTiming,
    ) -> *mut c_void;
    fn ns_js_free(js: *mut c_void);
    fn ns_js_set_form_submit_cb(js: *mut c_void, cb: Option<FormSubmitCb>, ud: *mut c_void);
    fn ns_js_set_style_table(js: *mut c_void, styles: *mut GHashTable);
    fn ns_js_set_image_cache(js: *mut c_void, cache: *mut c_void);
    fn ns_js_set_anim(js: *mut c_void, anim: *mut c_void);
    fn ns_js_set_layout_flush_cb(js: *mut c_void, cb: Option<FlushCb>, ud: *mut c_void);
    fn ns_js_set_layout_root(js: *mut c_void, root: *const NsBox);
    fn ns_js_set_mse_cb(js: *mut c_void, cb: MseCb, ud: *mut c_void);
    fn ns_js_set_mse_buffered_cb(js: *mut c_void, cb: MseBufferedCb, ud: *mut c_void);
    fn ns_js_set_mse_remove_cb(js: *mut c_void, cb: MseRemoveCb, ud: *mut c_void);
    fn ns_js_set_mse_bytes_cb(js: *mut c_void, cb: MseBytesCb, ud: *mut c_void);
    fn ns_js_set_early_inject_src(js: *mut c_void, src: *const c_char);
    fn ns_js_run_scripts_in_doc(js: *mut c_void, doc: *mut NsNode, base_url: *const c_char);
    fn ns_js_consume_mutated(js: *mut c_void) -> GBoolean;
    fn ns_js_eval_source(js: *mut c_void, src: *const c_char, origin: *const c_char)
    -> *mut c_char;
    fn ns_js_focused_node(js: *const c_void) -> *const NsNode;
    fn ns_js_set_focused_node(js: *mut c_void, el: *const NsNode);
    fn ns_js_dispatch_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_submit_event(
        js: *mut c_void,
        form: *const NsNode,
        submitter: *const NsNode,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_key_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        key: *const c_char,
        code: *const c_char,
        key_code: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_mouse_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        client_x: f64,
        client_y: f64,
        page_x: f64,
        page_y: f64,
        button: c_int,
        buttons: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        related: *const NsNode,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_drag_event(
        js: *mut c_void,
        session: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        client_x: f64,
        client_y: f64,
        page_x: f64,
        page_y: f64,
        button: c_int,
        buttons: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        related: *const NsNode,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_drag_session_new(js: *mut c_void) -> *mut c_void;
    fn ns_js_drag_session_free(session: *mut c_void);
    fn ns_js_drag_session_set_data(session: *mut c_void, kind: *const c_char, data: *const c_char);
    fn ns_js_click_activate(js: *mut c_void, node: *const NsNode) -> GBoolean;
    fn ns_js_details_toggle_open(js: *mut c_void, details: *mut NsNode, open: GBoolean);
    fn ns_js_note_pointer_input(js: *mut c_void, pointer: GBoolean);
    fn ns_js_note_viewport_scroll(js: *mut c_void, x: f64, y: f64);
    fn ns_js_sequential_focus_target(js: *mut c_void, backward: GBoolean) -> *const NsNode;
    fn ns_js_dispatch_anim_events(js: *mut c_void, anim: *mut c_void);
    fn ns_js_run_animation_frame(js: *mut c_void) -> GBoolean;
    fn ns_js_fire_media_load_events(js: *mut c_void, layout: *const NsBox);
    fn ns_js_video_event(js: *mut c_void, node: *const c_void, kind: *const c_char, value: f64);
    fn ns_video_cache_set_js_cb(cache: *mut c_void, cb: VideoJsCb, ud: *mut c_void);
    fn ns_video_cache_mse_append(
        cache: *mut c_void,
        stream_id: c_uint,
        kind: c_char,
        data: *const u8,
        len: usize,
    ) -> GBoolean;
    fn ns_video_cache_mse_eos(cache: *mut c_void, stream_id: c_uint);
    fn ns_video_cache_mse_buffered(
        cache: *mut c_void,
        stream_id: c_uint,
        kind: c_char,
        start: *mut f64,
    ) -> f64;
    fn ns_video_cache_mse_remove(
        cache: *mut c_void,
        stream_id: c_uint,
        kind: c_char,
        start: f64,
        end: f64,
    ) -> GBoolean;
    fn ns_video_cache_mse_bytes(cache: *mut c_void, stream_id: c_uint, kind: c_char) -> usize;
}

unsafe extern "C" fn on_log(line: *const c_char, _ud: *mut c_void) {
    let line = (!line.is_null()).then(|| unsafe { CStr::from_ptr(line) }.to_bytes());
    crate::inproc::js_log(line.unwrap_or(b"(null)"));
}

unsafe extern "C" fn on_mutated(_ud: *mut c_void) {
    crate::inproc::note_mutated();
}

unsafe extern "C" fn on_navigate(url: *const c_char, _reload: GBoolean, ud: *mut c_void) {
    let (Some(nav), false) = (unsafe { ud.cast::<NavCapture>().as_ref() }, url.is_null()) else {
        return;
    };
    nav.navigate(unsafe { CStr::from_ptr(url) });
}

unsafe extern "C" fn on_form_submit(
    form: *const NsNode,
    submitter: *const NsNode,
    ud: *mut c_void,
) {
    let (Some(nav), Some(form)) = (unsafe { ud.cast::<NavCapture>().as_ref() }, unsafe {
        Node::from_ptr(form)
    }) else {
        return;
    };
    crate::inproc::form_submit(nav, form, unsafe { Node::from_ptr(submitter) });
}

unsafe extern "C" fn on_flush(ud: *mut c_void) {
    if let Some(ctx) = unsafe { ud.cast::<Ctx>().as_ref() } {
        crate::inproc::flush_layout(ctx);
    }
}

unsafe extern "C" fn on_video_event(
    node: *const c_void,
    kind: *const c_char,
    value: f64,
    ud: *mut c_void,
) {
    if !ud.is_null() {
        unsafe { ns_js_video_event(ud, node, kind, value) };
    }
}

unsafe extern "C" fn on_mse_data(
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
    eos: GBoolean,
    ud: *mut c_void,
) -> GBoolean {
    if ud.is_null() {
        return glib::FALSE;
    }
    if eos != 0 {
        unsafe { ns_video_cache_mse_eos(ud, stream_id) };
        return glib::TRUE;
    }
    unsafe { ns_video_cache_mse_append(ud, stream_id, kind, data, len) }
}

unsafe extern "C" fn on_mse_buffered(
    stream_id: c_uint,
    kind: c_char,
    start: *mut f64,
    ud: *mut c_void,
) -> f64 {
    if ud.is_null() {
        if !start.is_null() {
            unsafe { *start = 0.0 };
        }
        return 0.0;
    }
    unsafe { ns_video_cache_mse_buffered(ud, stream_id, kind, start) }
}

unsafe extern "C" fn on_mse_remove(
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    ud: *mut c_void,
) -> GBoolean {
    glib::boolean(
        !ud.is_null() && unsafe { ns_video_cache_mse_remove(ud, stream_id, kind, start, end) } != 0,
    )
}

unsafe extern "C" fn on_mse_bytes(stream_id: c_uint, kind: c_char, ud: *mut c_void) -> usize {
    if ud.is_null() {
        return 0;
    }
    unsafe { ns_video_cache_mse_bytes(ud, stream_id, kind) }
}

#[derive(Clone, Copy)]
pub struct Js(NonNull<c_void>);

pub fn js_raw(js: Option<Js>) -> *mut c_void {
    js.map_or(ptr::null_mut(), |js| js.0.as_ptr())
}

fn node_ptr(node: Option<Node>) -> *const NsNode {
    Node::ptr_or_null(node)
}

pub fn new_js(nav: &NavCapture, timing: &NavigationTiming) -> Option<Js> {
    let js = unsafe {
        ns_js_new(
            on_log,
            ptr::null_mut(),
            on_mutated,
            ptr::null_mut(),
            on_navigate,
            (nav as *const NavCapture).cast_mut().cast(),
            timing,
        )
    };
    NonNull::new(js).map(Js)
}

pub fn consume_mutated(js: Option<Js>) -> bool {
    unsafe { ns_js_consume_mutated(js_raw(js)) != 0 }
}

pub fn eval_source(js: Option<Js>, src: &CStr, origin: &CStr) -> Option<GStr> {
    unsafe { GStr::take(ns_js_eval_source(js_raw(js), src.as_ptr(), origin.as_ptr())) }
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    unsafe { ns_js_note_pointer_input(js_raw(js), glib::boolean(pointer)) };
}

pub fn note_viewport_scroll(js: Option<Js>, x: f64, y: f64) {
    unsafe { ns_js_note_viewport_scroll(js_raw(js), x, y) };
}

impl Js {
    fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_js_free(self.raw()) };
    }

    pub fn bind_form_submit(self, nav: &NavCapture) {
        let ud = (nav as *const NavCapture).cast_mut().cast();
        unsafe { ns_js_set_form_submit_cb(self.raw(), Some(on_form_submit), ud) };
    }

    pub fn bind_layout_flush(self, ctx: &Ctx) {
        let ud = (ctx as *const Ctx).cast_mut().cast();
        unsafe { ns_js_set_layout_flush_cb(self.raw(), Some(on_flush), ud) };
    }

    pub fn unbind_layout_flush(self) {
        unsafe { ns_js_set_layout_flush_cb(self.raw(), None, ptr::null_mut()) };
    }

    pub fn bind_media(self, video_cache: Option<VideoCache>) {
        let ud = video_cache.map_or(ptr::null_mut(), VideoCache::raw);
        unsafe {
            ns_js_set_mse_cb(self.raw(), on_mse_data, ud);
            ns_js_set_mse_buffered_cb(self.raw(), on_mse_buffered, ud);
            ns_js_set_mse_remove_cb(self.raw(), on_mse_remove, ud);
            ns_js_set_mse_bytes_cb(self.raw(), on_mse_bytes, ud);
        }
    }

    pub fn set_style_table(self, styles: *mut GHashTable) {
        unsafe { ns_js_set_style_table(self.raw(), styles) };
    }

    pub fn set_image_cache(self, cache: Option<ImageCache>) {
        unsafe { ns_js_set_image_cache(self.raw(), super::engine::image_cache_raw(cache)) };
    }

    pub fn set_anim(self, anim: Option<Anim>) {
        unsafe { ns_js_set_anim(self.raw(), super::engine::anim_raw(anim)) };
    }

    pub fn set_layout_root(self, root: *const NsBox) {
        unsafe { ns_js_set_layout_root(self.raw(), root) };
    }

    pub fn set_early_inject_src(self, src: &CStr) {
        unsafe { ns_js_set_early_inject_src(self.raw(), src.as_ptr()) };
    }

    pub fn run_scripts_in_doc(self, doc: *mut NsNode, base_url: Option<&CStr>) {
        let base = base_url.map_or(ptr::null(), CStr::as_ptr);
        unsafe { ns_js_run_scripts_in_doc(self.raw(), doc, base) };
    }

    pub fn focused_node<'a>(self) -> Option<Node<'a>> {
        unsafe { Node::from_ptr(ns_js_focused_node(self.raw())) }
    }

    pub fn set_focused_node(self, el: Node) {
        unsafe { ns_js_set_focused_node(self.raw(), el.as_ptr()) };
    }

    pub fn dispatch(self, target: Node, kind: &CStr) {
        unsafe {
            ns_js_dispatch_event(self.raw(), target.as_ptr(), kind.as_ptr(), ptr::null_mut())
        };
    }

    pub fn dispatch_checked(self, target: Node, kind: &CStr) -> bool {
        let mut prevented: GBoolean = glib::FALSE;
        unsafe { ns_js_dispatch_event(self.raw(), target.as_ptr(), kind.as_ptr(), &mut prevented) };
        prevented != 0
    }

    pub fn dispatch_submit(self, form: Node, submitter: Node) -> bool {
        let mut prevented: GBoolean = glib::FALSE;
        unsafe {
            ns_js_dispatch_submit_event(
                self.raw(),
                form.as_ptr(),
                submitter.as_ptr(),
                &mut prevented,
            )
        };
        prevented != 0
    }

    pub fn dispatch_key(
        self,
        target: Node,
        kind: &CStr,
        key: &CStr,
        key_code: c_int,
        check: bool,
    ) -> bool {
        let mut prevented: GBoolean = glib::FALSE;
        let out = if check {
            &mut prevented as *mut GBoolean
        } else {
            ptr::null_mut()
        };
        unsafe {
            ns_js_dispatch_key_event(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                key.as_ptr(),
                key.as_ptr(),
                key_code,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                out,
            )
        };
        prevented != 0
    }

    pub fn dispatch_mouse(
        self,
        target: Node,
        kind: &CStr,
        (x, y): (f64, f64),
        (button, buttons): (c_int, c_int),
        prevented: &mut GBoolean,
    ) {
        unsafe {
            ns_js_dispatch_mouse_event(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                x,
                y,
                x,
                y,
                button,
                buttons,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                ptr::null(),
                prevented,
            )
        };
    }

    pub fn click_activate(self, node: Node) -> bool {
        unsafe { ns_js_click_activate(self.raw(), node.as_ptr()) != 0 }
    }

    pub fn details_toggle_open(self, details: Node, open: bool) {
        unsafe { ns_js_details_toggle_open(self.raw(), details.as_mut_ptr(), glib::boolean(open)) };
    }

    pub fn sequential_focus_target<'a>(self) -> Option<Node<'a>> {
        unsafe { Node::from_ptr(ns_js_sequential_focus_target(self.raw(), glib::FALSE)) }
    }

    pub fn dispatch_anim_events(self, anim: Anim) {
        unsafe { ns_js_dispatch_anim_events(self.raw(), anim.raw()) };
    }

    pub fn run_animation_frame(self) {
        unsafe { ns_js_run_animation_frame(self.raw()) };
    }

    pub fn fire_media_load_events(self, layout: *const NsBox) {
        unsafe { ns_js_fire_media_load_events(self.raw(), layout) };
    }

    pub fn drag_session(self) -> Option<DragSession> {
        NonNull::new(unsafe { ns_js_drag_session_new(self.raw()) }).map(DragSession)
    }
}

pub fn bind_video_events(cache: Option<VideoCache>, js: Option<Js>) {
    if let Some(cache) = cache {
        unsafe { ns_video_cache_set_js_cb(cache.raw(), on_video_event, js_raw(js)) };
    }
}

pub struct DragSession(NonNull<c_void>);

impl DragSession {
    pub fn set_data(&self, kind: &CStr, data: &CStr) {
        unsafe { ns_js_drag_session_set_data(self.0.as_ptr(), kind.as_ptr(), data.as_ptr()) };
    }

    pub fn dispatch(
        &self,
        js: Js,
        target: Node,
        kind: &CStr,
        (x, y): (f64, f64),
        buttons: c_int,
        related: Option<Node>,
    ) -> bool {
        let mut prevented: GBoolean = glib::FALSE;
        unsafe {
            ns_js_dispatch_drag_event(
                js.raw(),
                self.0.as_ptr(),
                target.as_ptr(),
                kind.as_ptr(),
                x,
                y,
                x,
                y,
                0,
                buttons,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                glib::FALSE,
                node_ptr(related),
                &mut prevented,
            )
        };
        prevented != 0
    }
}

impl Drop for DragSession {
    fn drop(&mut self) {
        unsafe { ns_js_drag_session_free(self.0.as_ptr()) };
    }
}
