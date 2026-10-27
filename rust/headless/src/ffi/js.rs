//! Southstar — the script engine calls of the in-process run and the callbacks it and the video cache make back into the driver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_runtime::api::{self, Page};
use southstar_layout::BoxRef;

use super::engine::{Anim, ImageCache, VideoCache};
use crate::inproc::{Ctx, NavCapture};

pub use southstar_js_runtime::api::NavigationTiming;

#[derive(Clone, Copy)]
pub struct Js(Page);

pub fn js_page(js: Option<Js>) -> Option<Page> {
    js.map(Js::handle)
}

pub fn new_js(nav: &Rc<NavCapture>, timing: &NavigationTiming) -> Option<Js> {
    southstar_js_page::install_runtime_hooks();
    let nav = Rc::clone(nav);
    let callbacks = api::PageCallbacks {
        log: Some(Rc::new(|line: &CStr| {
            crate::inproc::js_log(line.to_bytes())
        })),
        mutated: Some(Rc::new(crate::inproc::note_mutated)),
        navigate: Some(Rc::new(move |url: &CStr, _reload| nav.navigate(url))),
    };
    api::new_page(callbacks, Some(*timing)).map(Js)
}

pub fn consume_mutated(js: Option<Js>) -> bool {
    api::consume_mutated(js_page(js))
}

pub fn eval_source(js: Option<Js>, src: &CStr, origin: &CStr) -> Option<CString> {
    let mut out = southstar_js_loader::eval_source(js_page(js), src, Some(origin))?;
    out.truncate(out.iter().position(|&b| b == 0).unwrap_or(out.len()));
    CString::new(out).ok()
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    if let Some(js) = js {
        southstar_js_focus::note_pointer_input(js.0, pointer);
    }
}

pub fn note_viewport_scroll(js: Option<Js>, x: f64, y: f64) {
    if let Some(js) = js {
        southstar_js_window::note_viewport_scroll(js.0, x, y);
    }
}

impl Js {
    pub fn handle(self) -> Page {
        self.0
    }

    fn page(self) -> Option<Page> {
        Some(self.0)
    }

    pub fn free(self) {
        api::free(self.0);
    }

    pub fn bind_form_submit(self, nav: &Rc<NavCapture>) {
        let nav = Rc::clone(nav);
        api::set_form_submit_hook(
            self.page(),
            Some(Rc::new(
                move |form: Node<'_>, submitter: Option<Node<'_>>| {
                    crate::inproc::form_submit(&nav, form, submitter);
                },
            )),
        );
    }

    pub fn bind_layout_flush(self, ctx: &Ctx) {
        let ctx = ptr::from_ref(ctx);
        api::set_layout_flush_hook(
            self.page(),
            Some(Rc::new(move || {
                // SAFETY: the run unbinds the layout flush before its context goes away.
                crate::inproc::flush_layout(unsafe { &*ctx });
            })),
        );
    }

    pub fn unbind_layout_flush(self) {
        api::set_layout_flush_hook(self.page(), None);
    }

    pub fn bind_media(self, video_cache: Option<VideoCache>) {
        let page = self.page();
        southstar_js_media::set_mse_hook(
            page,
            Some(Rc::new(move |stream_id, kind, data: Option<&[u8]>| {
                let Some(cache) = video_cache else {
                    return false;
                };
                match data {
                    Some(data) => cache.mse_append(stream_id, kind, data),
                    None => {
                        cache.mse_eos(stream_id);
                        true
                    }
                }
            })),
        );
        southstar_js_media::set_mse_buffered_hook(
            page,
            Some(Rc::new(move |stream_id, kind| {
                video_cache.map_or((0.0, 0.0), |cache| cache.mse_buffered(stream_id, kind))
            })),
        );
        southstar_js_media::set_mse_remove_hook(
            page,
            Some(Rc::new(move |stream_id, kind, start, end| {
                video_cache.is_some_and(|cache| cache.mse_remove(stream_id, kind, start, end))
            })),
        );
        southstar_js_media::set_mse_bytes_hook(
            page,
            Some(Rc::new(move |stream_id, kind| {
                video_cache.map_or(0, |cache| cache.mse_bytes(stream_id, kind))
            })),
        );
    }

    pub fn set_style_table(self, styles: Option<Rc<StyleMap>>) {
        api::set_style_table(self.page(), styles);
    }

    pub fn set_image_cache(self, cache: Option<ImageCache>) {
        api::set_image_cache(self.page(), super::engine::image_cache_raw(cache));
    }

    pub fn set_anim(self, anim: Option<Anim>) {
        api::set_anim(self.page(), super::engine::anim_raw(anim));
    }

    pub fn clear_layout_root(self) {
        api::set_layout_root(self.page(), None);
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
        southstar_js_focus::set_focused_node(self.handle(), Some(el));
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
        let (_, prevented) =
            southstar_js_forms::dispatch_submit_event(self.handle(), form, Some(submitter));
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
        let (_, was_prevented) =
            southstar_js_input::dispatch_mouse_event(self.handle(), target, kind, &input, None);
        *prevented = glib::boolean(was_prevented);
    }

    pub fn click_activate(self, node: Node) -> bool {
        southstar_js_forms::click_activate(self.handle(), node)
    }

    pub fn details_toggle_open(self, details: Node, open: bool) {
        southstar_js_top_layer::details_toggle_open(self.page(), details, open);
    }

    pub fn sequential_focus_target<'a>(self) -> Option<Node<'a>> {
        southstar_js_focus::sequential_focus_target(self.handle(), false)
    }

    pub fn dispatch_anim_events(self, anim: Anim) {
        southstar_js_lifecycle::dispatch_anim_events(self.handle(), anim);
    }

    pub fn run_animation_frame(self) {
        southstar_js_lifecycle::run_animation_frame(self.handle());
    }

    pub fn fire_media_load_events(self, layout: Option<BoxRef<'_>>) {
        southstar_js_lifecycle::fire_media_load_events(self.handle(), layout);
    }

    pub fn drag_session(self) -> Option<DragSession> {
        southstar_js_input::DragData::new(self.handle()).map(DragSession)
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
        let (_, prevented) = southstar_js_input::dispatch_drag_event(
            js.handle(),
            Some(&self.0),
            target,
            kind,
            &input,
            related,
        );
        prevented
    }
}
