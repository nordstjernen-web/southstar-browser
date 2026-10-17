//! Southstar — the script engine handle of a page and the script engine calls the lifecycle makes on it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::Node;
use southstar_glib::{GHashTable, GStr};
use southstar_js_runtime::api::{self, NsJs};
use southstar_layout::BoxRef;

use super::engine::{Anim, Images};
use super::glib::StrBufOwned;

#[derive(Clone, Copy)]
pub struct Js(NonNull<c_void>);

impl Js {
    pub unsafe fn from_ptr(p: *mut c_void) -> Option<Js> {
        NonNull::new(p).map(Js)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn page(self) -> *const NsJs {
        self.0.as_ptr().cast_const().cast()
    }

    pub fn consume_mutated(self) -> bool {
        api::consume_mutated(self.page())
    }

    pub fn sync_window_metrics(self) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_window::sync_window_metrics(self.raw()) };
    }

    pub fn set_layout_root(self, root: Option<BoxRef<'_>>) {
        api::set_layout_root(self.page(), root.map_or(ptr::null(), |r| r.as_ptr().cast()));
    }

    pub fn set_style_table(self, styles: Option<*mut GHashTable>) {
        api::set_style_table(self.page(), styles.unwrap_or(ptr::null_mut()));
    }

    pub fn focused_node<'a>(self) -> Option<Node<'a>> {
        southstar_js_focus::focused_node(self.page())
    }

    pub fn fire_media_load_events(self, layout: BoxRef<'_>) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::fire_media_load_events(self.raw(), Some(layout)) };
    }

    pub fn has_pending_work(self) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::has_pending_work(self.raw()) }
    }

    pub fn has_pending_animation_frame(self) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::has_pending_animation_frame(self.raw()) }
    }

    pub fn dispatch_anim_events(self, anim: Anim) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::dispatch_anim_events(self.raw(), anim) };
    }

    pub fn run_animation_frame(self) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::run_animation_frame(self.raw()) }
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
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_window::note_viewport_scroll(self.raw(), x, y) };
    }

    pub fn needs_tick(self) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_lifecycle::needs_tick(self.raw()) }
    }

    pub fn dispatch_resize(self) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_window::dispatch_resize(self.raw()) };
    }

    pub fn reeval_media_queries(self) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_window::reeval_media_queries(self.raw()) };
    }

    pub fn window_action_applied(self) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_focus::window_action_applied(self.raw()) };
    }

    pub fn dump_stats(self, out: &StrBufOwned) {
        out.append(southstar_js_page::dump_stats(self.page()).as_bytes());
    }

    pub fn eval(self, src: &CStr, origin: &CStr) -> Option<GStr> {
        southstar_js_loader::eval_source(self.page(), src, Some(origin))
            .and_then(|out| super::glib::gstr_from(&out))
    }

    pub fn fire_page_transition(self, kind: &CStr, persisted: bool) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_window::fire_page_transition(self.raw(), kind, persisted) };
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
        // SAFETY: a Js handle names the page's live script engine.
        let (_, prevented) =
            unsafe { southstar_js_forms::dispatch_submit_event(self.raw(), form, Some(submitter)) };
        prevented
    }

    pub fn set_image_cache(self, images: Option<Images>) {
        api::set_image_cache(self.page(), images.map_or(ptr::null_mut(), Images::raw));
    }

    pub fn set_anim(self, anim: Option<Anim>) {
        api::set_anim(self.page(), anim.map_or(ptr::null_mut(), Anim::raw));
    }
}
