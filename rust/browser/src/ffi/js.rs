//! Southstar — the script engine handle of a page and the script engine calls the lifecycle makes on it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;
use std::rc::Rc;

use southstar_css::StyleMap;

use southstar_dom::Node;
use southstar_js_runtime::api::{self, NsJs, Page};
use southstar_layout::BoxRef;
use std::ffi::CString;

use super::engine::{Anim, Images};
use super::glib::StrBufOwned;

#[derive(Clone, Copy)]
pub struct Js(Page);

impl Js {
    pub fn new(page: Page) -> Js {
        Js(page)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast_mut().cast()
    }

    pub fn page(self) -> *const NsJs {
        self.0.as_ptr()
    }

    pub fn handle(self) -> Page {
        self.0
    }

    pub fn consume_mutated(self) -> bool {
        api::consume_mutated(self.page())
    }

    pub fn sync_window_metrics(self) {
        southstar_js_window::sync_window_metrics(self.handle());
    }

    pub fn set_layout_root(self, root: Option<BoxRef<'_>>) {
        api::set_layout_root(self.page(), root);
    }

    pub fn set_style_table(self, styles: Option<Rc<StyleMap>>) {
        api::set_style_table(self.page(), styles);
    }

    pub fn focused_node<'a>(self) -> Option<Node<'a>> {
        southstar_js_focus::focused_node(self.page())
    }

    pub fn fire_media_load_events(self, layout: BoxRef<'_>) {
        southstar_js_lifecycle::fire_media_load_events(self.handle(), Some(layout));
    }

    pub fn has_pending_work(self) -> bool {
        southstar_js_lifecycle::has_pending_work(self.handle())
    }

    pub fn has_pending_animation_frame(self) -> bool {
        southstar_js_lifecycle::has_pending_animation_frame(self.handle())
    }

    pub fn dispatch_anim_events(self, anim: Anim) {
        southstar_js_lifecycle::dispatch_anim_events(self.handle(), anim);
    }

    pub fn run_animation_frame(self) -> bool {
        southstar_js_lifecycle::run_animation_frame(self.handle())
    }

    pub fn set_selection(self, text: Option<&CStr>, has_range: bool, r: [f64; 4]) {
        api::set_selection(self.page(), text, has_range, r);
    }

    pub fn request_repaint(self) {
        api::request_repaint(self.page());
    }

    pub fn add_csp_header(self, header: Option<&CStr>) {
        api::add_csp_header(self.page(), header);
    }

    pub fn run_scripts_in_doc(self, doc: Option<Node<'_>>, base: Option<&CStr>) {
        if let Some(doc) = doc {
            southstar_js_page::run_scripts_in_doc(self.page(), doc, base);
        }
    }

    pub fn note_viewport_scroll(self, x: f64, y: f64) {
        southstar_js_window::note_viewport_scroll(self.handle(), x, y);
    }

    pub fn needs_tick(self) -> bool {
        southstar_js_lifecycle::needs_tick(self.handle())
    }

    pub fn dispatch_resize(self) {
        southstar_js_window::dispatch_resize(self.handle());
    }

    pub fn reeval_media_queries(self) {
        southstar_js_window::reeval_media_queries(self.handle());
    }

    pub fn window_action_applied(self) {
        southstar_js_focus::window_action_applied(self.handle());
    }

    pub fn dump_stats(self, out: &StrBufOwned) {
        out.append(southstar_js_page::dump_stats(self.page()).as_bytes());
    }

    pub fn eval(self, src: &CStr, origin: &CStr) -> Option<CString> {
        southstar_js_loader::eval_source(self.page(), src, Some(origin))
            .and_then(|out| super::glib::owned_text(&out))
    }

    pub fn fire_page_transition(self, kind: &CStr, persisted: bool) {
        southstar_js_window::fire_page_transition(self.handle(), kind, persisted);
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.page())
    }

    pub fn free(self) {
        api::free(self.page());
    }

    pub fn submit_form(self, form: Node<'_>, submitter: Node<'_>) {
        api::submit_form(self.page(), form, Some(submitter));
    }

    pub fn csp_form_action_allowed(self, url: &CStr) -> bool {
        api::csp_form_action_allowed(self.page(), Some(url))
    }

    pub fn dispatch_event(self, target: Node<'_>, kind: &CStr) {
        southstar_js_dispatch::dispatch_event(self.page(), target, &kind.to_string_lossy());
    }

    pub fn dispatch_submit_event(self, form: Node<'_>, submitter: Node<'_>) -> bool {
        let (_, prevented) =
            southstar_js_forms::dispatch_submit_event(self.handle(), form, Some(submitter));
        prevented
    }

    pub fn set_image_cache(self, images: Option<Images>) {
        api::set_image_cache(self.page(), images.map_or(ptr::null_mut(), Images::raw));
    }

    pub fn set_anim(self, anim: Option<Anim>) {
        api::set_anim(self.page(), anim.map_or(ptr::null_mut(), Anim::raw));
    }
}
