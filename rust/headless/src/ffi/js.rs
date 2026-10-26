//! Southstar — the script engine calls of the in-process run and the callbacks it and the video cache make back into the driver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use super::engine::{Anim, ImageCache, VideoCache};
use crate::inproc::{Ctx, NavCapture};

pub use southstar_js_runtime::api::NavigationTiming;

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

unsafe extern "C" {
    fn ns_js_set_mse_cb(js: *mut c_void, cb: MseCb, ud: *mut c_void);
    fn ns_js_set_mse_buffered_cb(js: *mut c_void, cb: MseBufferedCb, ud: *mut c_void);
    fn ns_js_set_mse_remove_cb(js: *mut c_void, cb: MseRemoveCb, ud: *mut c_void);
    fn ns_js_set_mse_bytes_cb(js: *mut c_void, cb: MseBytesCb, ud: *mut c_void);
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

unsafe extern "C" fn on_style_flush(ud: *mut c_void) {
    if let Some(ctx) = unsafe { ud.cast::<Ctx>().as_ref() } {
        crate::inproc::flush_style(ctx);
    }
}

fn video_cache(ud: *mut c_void) -> Option<VideoCache> {
    // SAFETY: bind_media registers null or the run's cache handle, which outlives the script engine's callbacks.
    unsafe { VideoCache::from_raw(ud) }
}

unsafe extern "C" fn on_mse_data(
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
    eos: GBoolean,
    ud: *mut c_void,
) -> GBoolean {
    let Some(cache) = video_cache(ud) else {
        return glib::FALSE;
    };
    if eos != 0 {
        cache.mse_eos(stream_id);
        return glib::TRUE;
    }
    if data.is_null() {
        return glib::FALSE;
    }
    // SAFETY: the script engine passes `len` readable bytes at `data` for the duration of the call.
    let data = unsafe { glib::slice(data, len) };
    glib::boolean(cache.mse_append(stream_id, kind as u8, data))
}

unsafe extern "C" fn on_mse_buffered(
    stream_id: c_uint,
    kind: c_char,
    start: *mut f64,
    ud: *mut c_void,
) -> f64 {
    let (first, end) = video_cache(ud).map_or((0.0, 0.0), |cache| {
        cache.mse_buffered(stream_id, kind as u8)
    });
    // SAFETY: `start` is null or the script engine's out-parameter, writable for the call.
    if let Some(start) = unsafe { start.as_mut() } {
        *start = first;
    }
    end
}

unsafe extern "C" fn on_mse_remove(
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    ud: *mut c_void,
) -> GBoolean {
    glib::boolean(
        video_cache(ud).is_some_and(|cache| cache.mse_remove(stream_id, kind as u8, start, end)),
    )
}

unsafe extern "C" fn on_mse_bytes(stream_id: c_uint, kind: c_char, ud: *mut c_void) -> usize {
    video_cache(ud).map_or(0, |cache| cache.mse_bytes(stream_id, kind as u8))
}

#[derive(Clone, Copy)]
pub struct Js(NonNull<c_void>);

pub fn js_raw(js: Option<Js>) -> *mut c_void {
    js.map_or(ptr::null_mut(), |js| js.0.as_ptr())
}

pub fn new_js(nav: &NavCapture, timing: &NavigationTiming) -> Option<Js> {
    southstar_js_page::install_runtime_hooks();
    let callbacks = api::PageCallbacks {
        log: Some(on_log),
        log_data: ptr::null_mut(),
        mutated: Some(on_mutated),
        mutated_data: ptr::null_mut(),
        navigate: Some(on_navigate),
        navigate_data: (nav as *const NavCapture).cast_mut().cast(),
    };
    NonNull::new(api::new_page(callbacks, Some(*timing)).cast()).map(Js)
}

pub fn consume_mutated(js: Option<Js>) -> bool {
    api::consume_mutated(js_raw(js).cast())
}

pub fn eval_source(js: Option<Js>, src: &CStr, origin: &CStr) -> Option<CString> {
    let mut out = southstar_js_loader::eval_source(js_raw(js).cast(), src, Some(origin))?;
    out.truncate(out.iter().position(|&b| b == 0).unwrap_or(out.len()));
    CString::new(out).ok()
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    // SAFETY: js_raw gives null or the run's live script engine.
    unsafe { southstar_js_focus::note_pointer_input(js_raw(js), pointer) };
}

pub fn note_viewport_scroll(js: Option<Js>, x: f64, y: f64) {
    // SAFETY: js_raw gives null or the run's live script engine.
    unsafe { southstar_js_window::note_viewport_scroll(js_raw(js), x, y) };
}

impl Js {
    fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    fn page(self) -> *const api::NsJs {
        self.0.as_ptr().cast()
    }

    pub fn free(self) {
        api::free(self.page());
    }

    pub fn bind_form_submit(self, nav: &NavCapture) {
        let ud = (nav as *const NavCapture).cast_mut().cast();
        api::set_form_submit_hook(self.page(), Some(on_form_submit), ud);
    }

    pub fn bind_layout_flush(self, ctx: &Ctx) {
        let ud = (ctx as *const Ctx).cast_mut().cast();
        api::set_layout_flush_hook(self.page(), Some(on_flush), ud);
        api::set_style_flush_hook(self.page(), Some(on_style_flush), ud);
    }

    pub fn unbind_layout_flush(self) {
        api::set_layout_flush_hook(self.page(), None, ptr::null_mut());
        api::set_style_flush_hook(self.page(), None, ptr::null_mut());
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
        api::set_style_table(self.page(), styles);
    }

    pub fn set_image_cache(self, cache: Option<ImageCache>) {
        api::set_image_cache(self.page(), super::engine::image_cache_raw(cache));
    }

    pub fn set_anim(self, anim: Option<Anim>) {
        api::set_anim(self.page(), super::engine::anim_raw(anim));
    }

    pub fn set_layout_root(self, root: *const NsBox) {
        api::set_layout_root(self.page(), root.cast());
    }

    pub fn set_early_inject_src(self, src: &CStr) {
        api::set_early_inject_src(self.page(), Some(src));
    }

    pub fn run_scripts_in_doc(self, doc: Node, base_url: Option<&CStr>) {
        southstar_js_page::run_scripts_in_doc(self.page(), doc, base_url);
    }

    pub fn focused_node<'a>(self) -> Option<Node<'a>> {
        southstar_js_focus::focused_node(self.page())
    }

    pub fn set_focused_node(self, el: Node) {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_focus::set_focused_node(self.raw(), Some(el)) };
    }

    pub fn dispatch(self, target: Node, kind: &CStr) {
        self.dispatch_checked(target, kind);
    }

    pub fn dispatch_checked(self, target: Node, kind: &CStr) -> bool {
        southstar_js_dispatch::dispatch_event(self.page(), target, &kind.to_string_lossy()).1
    }

    pub fn submit_form(self, form: Node, submitter: Node) {
        api::submit_form(self.page(), form, Some(submitter));
    }

    pub fn dispatch_submit(self, form: Node, submitter: Node) -> bool {
        // SAFETY: a Js handle names the run's live script engine.
        let (_, prevented) =
            unsafe { southstar_js_forms::dispatch_submit_event(self.raw(), form, Some(submitter)) };
        prevented
    }

    pub fn dispatch_key(
        self,
        target: Node,
        kind: &CStr,
        key: &CStr,
        key_code: c_int,
        check: bool,
    ) -> bool {
        let key = key.to_string_lossy();
        let press = southstar_js_dispatch::KeyPress {
            key: Some(&key),
            code: Some(&key),
            key_code,
            char_code: 0,
            modifiers: southstar_js_dispatch::Modifiers::default(),
        };
        let (_, prevented) = southstar_js_dispatch::dispatch_key_event(
            self.page(),
            target,
            &kind.to_string_lossy(),
            &press,
        );
        check && prevented
    }

    pub fn dispatch_mouse(
        self,
        target: Node,
        kind: &CStr,
        (x, y): (f64, f64),
        (button, buttons): (c_int, c_int),
        prevented: &mut GBoolean,
    ) {
        let input = pointer((x, y), button, buttons);
        // SAFETY: a Js handle names the run's live script engine.
        let (_, was_prevented) = unsafe {
            southstar_js_input::dispatch_mouse_event(self.raw(), target, kind, &input, None)
        };
        *prevented = glib::boolean(was_prevented);
    }

    pub fn click_activate(self, node: Node) -> bool {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_forms::click_activate(self.raw(), node) }
    }

    pub fn details_toggle_open(self, details: Node, open: bool) {
        southstar_js_top_layer::details_toggle_open(self.page(), details, open);
    }

    pub fn sequential_focus_target<'a>(self) -> Option<Node<'a>> {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_focus::sequential_focus_target(self.raw(), false) }
    }

    pub fn dispatch_anim_events(self, anim: Anim) {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_lifecycle::dispatch_anim_events(self.raw(), anim) };
    }

    pub fn run_animation_frame(self) {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_lifecycle::run_animation_frame(self.raw()) };
    }

    pub fn fire_media_load_events(self, layout: Option<BoxRef<'_>>) {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_lifecycle::fire_media_load_events(self.raw(), layout) };
    }

    pub fn drag_session(self) -> Option<DragSession> {
        // SAFETY: a Js handle names the run's live script engine.
        unsafe { southstar_js_input::DragData::new(self.raw()) }.map(DragSession)
    }
}

pub fn bind_video_events(cache: Option<VideoCache>, js: Option<Js>) {
    if let Some(cache) = cache {
        cache.set_js_sink(js.map(|js| -> southstar_video::JsSink {
            Rc::new(move |node, kind, value| {
                // SAFETY: the video cache reports events with the pointer of the live media element that owns the video.
                if let Some(node) = unsafe { Node::from_ptr(node.cast()) } {
                    southstar_js_media::video_event(js.page(), node, kind, value);
                }
            })
        }));
    }
}

fn pointer((x, y): (f64, f64), button: c_int, buttons: c_int) -> southstar_js_input::Pointer {
    southstar_js_input::Pointer {
        client: (x, y),
        page: (x, y),
        button,
        buttons,
        modifiers: southstar_js_input::Modifiers::default(),
    }
}

pub struct DragSession(southstar_js_input::DragData);

impl DragSession {
    pub fn set_data(&self, kind: &CStr, data: &CStr) {
        self.0.set_data(kind, Some(data));
    }

    pub fn dispatch(
        &self,
        js: Js,
        target: Node,
        kind: &CStr,
        at: (f64, f64),
        buttons: c_int,
        related: Option<Node>,
    ) -> bool {
        let input = pointer(at, 0, buttons);
        // SAFETY: a Js handle names the run's live script engine.
        let (_, prevented) = unsafe {
            southstar_js_input::dispatch_drag_event(
                js.raw(),
                Some(&self.0),
                target,
                kind,
                &input,
                related,
            )
        };
        prevented
    }
}
