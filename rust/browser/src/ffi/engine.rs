//! Southstar — the pipeline calls of the page lifecycle: cascade and relayout, captures, CSS state, layout, paint, selection and the image, video and animation caches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, GStr};
use southstar_layout::{BoxRef, NsBox, Style};

use super::glib::StrBufOwned;
use super::{Js, NsBrowser};

pub use southstar_anim::AnimHandle as Anim;
pub use southstar_engine::{ImageSession as Session, PrintSetup};
pub use southstar_image::ImageCache as Images;
pub use southstar_video::VideoCache as Videos;

unsafe extern "C" {
    fn ns_render_page_rule() -> *const c_void;
    fn ns_print_setup_default(setup: *mut PrintSetup);
    fn ns_print_setup_apply_page_rule(setup: *mut PrintSetup, rule: *const c_void);
    fn ns_layout_set_open_select(select: *const NsNode);
    fn ns_layout_set_datalist_open(open: GBoolean);
    pub fn ns_box_free(b: *mut NsBox);
    fn ns_config_init();
    fn ns_config_shutdown();
    fn ns_security_harden_allocator();
    fn ns_security_win32_mitigations_init(allow_child_processes: GBoolean);
    fn ns_security_sandbox_init(self_exe: *const c_char);
    fn ns_security_seccomp_init();
    fn ns_net_init();
    fn ns_net_shutdown();
    fn ns_net_set_allow_file_urls(allow: GBoolean);
    fn ns_cache_init();
    fn ns_cache_shutdown();
    fn ns_history_init();
    fn ns_history_shutdown();
    fn ns_font_init();
    fn ns_font_shutdown();
    fn ns_spell_init();
    fn ns_webgl_take_pending_origin() -> *mut c_char;
    fn ns_webgl_set_decision(origin: *const c_char, allow: c_int);
    fn ns_camera_take_pending_origin() -> *mut c_char;
    fn ns_camera_set_decision(origin: *const c_char, allow: c_int);
}

fn opt(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

impl NsBrowser {
    fn engine_page<'a>(&'a self, base: Option<&'a CStr>) -> southstar_engine::Page<'a> {
        southstar_engine::Page {
            doc: self.doc(),
            base,
            css_cache: self.css_cache.get(),
        }
    }

    pub fn engine_relayout(&self, focused: Option<Node<'_>>) {
        let base = self.base_url.get();
        let relayout = southstar_engine::Relayout {
            page: self.engine_page(base.as_deref()),
            viewport_width: self.vw.get(),
            viewport_height: self.vh.get(),
            images: self.images.get(),
            anim: self.anim.get(),
            js: self.js.get(),
            focused,
            hover: self.hover_node.get(),
            caret_byte: self.caret_byte.get(),
            sel_anchor_byte: self.sel_anchor_byte.get(),
        };
        // SAFETY: the page's script engine slot is null or holds its live engine.
        let styles = unsafe { southstar_engine::relayout(&relayout, &self.layout) };
        self.styles.set(styles);
    }

    pub fn compute_cascade(&self) {
        let base = self.base_url.get();
        let styles = southstar_engine::compute_cascade(&self.engine_page(base.as_deref()));
        self.styles.set(styles);
    }

    pub fn load_keyframes(&self, anim: Anim) {
        let base = self.base_url.get();
        southstar_engine::load_keyframes(anim, &self.engine_page(base.as_deref()));
        // SAFETY: the page's style table is null or the live table of its last cascade.
        unsafe { southstar_engine::anim_observe(anim, self.styles.get(), super::monotonic_us()) };
    }

    pub fn speculative_preload(&self) {
        let base = self.base_url.get();
        if let (Some(doc), Some(base)) = (self.doc(), base.as_deref()) {
            southstar_engine::speculative_preload(doc, base, false);
        }
    }

    pub fn styles_raw(&self) -> *mut GHashTable {
        self.styles.get()
    }

    pub fn set_open_select_for_layout(&self) {
        unsafe { ns_layout_set_open_select(Node::ptr_or_null(self.open_select.get())) };
    }
}

pub unsafe fn box_free(b: *mut NsBox) {
    unsafe { ns_box_free(b) };
}

pub use southstar_engine::in_blocking_fetch as engine_in_blocking_fetch;

pub fn write_capture(layout: BoxRef<'_>, path: &CStr, pdf: bool) -> c_int {
    if pdf {
        southstar_engine::write_pdf(layout, path)
    } else {
        southstar_engine::write_png(layout, path)
    }
}

pub fn dump_text(layout: BoxRef<'_>, out: &StrBufOwned) {
    out.append(&southstar_engine::dump_text(layout));
}

pub fn dump_layout(layout: BoxRef<'_>, out: &StrBufOwned) {
    out.append(&southstar_engine::dump_layout(layout));
}

pub fn print_setup_default() -> PrintSetup {
    let mut setup = PrintSetup::default();
    unsafe { ns_print_setup_default(&mut setup) };
    setup
}

pub fn apply_page_rule(setup: &mut PrintSetup) -> bool {
    let rule = unsafe { ns_render_page_rule() };
    if rule.is_null() {
        return false;
    }
    unsafe { ns_print_setup_apply_page_rule(setup, rule) };
    true
}

pub fn css_set_viewport(vw: f64, vh: f64) {
    southstar_css_media::set_base_viewport(vw, vh);
}

pub fn css_set_doc_language(lang: Option<&CStr>) {
    southstar_css::set_doc_language(lang.map(CStr::to_bytes));
}

pub fn css_set_target_fragment(fragment: Option<&CStr>) {
    southstar_css::set_target_fragment(fragment.map(CStr::to_bytes));
}

pub fn css_device_pixel_ratio() -> f64 {
    southstar_css_media::device_pixel_ratio()
}

pub fn css_set_device_pixel_ratio(dppx: f64) {
    southstar_css_media::set_device_pixel_ratio(dppx);
}

pub fn css_set_print_media(printing: bool) {
    southstar_css_media::set_print_media(printing);
}

pub fn css_set_active_node(node: Option<Node<'_>>) {
    super::set_active_node(node);
}

pub fn css_prop_id(name: &CStr) -> c_int {
    southstar_css::Prop::from_name(name.to_bytes()).map_or(-1, |prop| prop.id() as c_int)
}

fn keyword_in<'a>(style: Option<southstar_style::StyleRef<'a>>, prop: c_int) -> Option<&'a CStr> {
    let prop = usize::try_from(prop)
        .ok()
        .and_then(southstar_css::Prop::from_id)?;
    style?.keyword_of(prop)
}

pub fn style_keyword(b: BoxRef<'_>, prop: c_int) -> Option<&CStr> {
    keyword_in(southstar_style::box_style(b), prop)
}

pub fn style_keyword_raw<'a>(style: *const Style, prop: c_int) -> Option<&'a CStr> {
    keyword_in(unsafe { southstar_style::StyleRef::from_ptr(style) }, prop)
}

pub fn layout_set_datalist_open(open: bool) {
    unsafe { ns_layout_set_datalist_open(southstar_glib::boolean(open)) };
}

pub fn inline_rect_y(layout: BoxRef<'_>, target: Node<'_>) -> Option<f64> {
    southstar_layout_query::inline_rect_for_dom(layout, target).map(|(_, y, _, _)| y)
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn paint_3d_invalidate() {
    southstar_paint::invalidate_3d();
}

pub fn paint_set_js(js: Option<Js>) {
    // SAFETY: the page's script engine outlives the paint passes it is set for, and the caller resets it to null when they end.
    unsafe { southstar_paint::set_js(js.map_or(ptr::null_mut(), Js::raw)) };
}

pub fn paint_set_anim(anim: Option<Anim>) {
    southstar_paint::set_anim(anim);
}

pub fn init_subsystems() {
    unsafe {
        ns_config_init();
        if southstar_config::get().is_some_and(|c| c.harden_allocator != 0) {
            ns_security_harden_allocator();
        }
        ns_net_init();
        ns_net_set_allow_file_urls(1);
        ns_cache_init();
        ns_history_init();
        ns_font_init();
        ns_spell_init();
    }
    southstar_bytecode_cache::init();
}

pub fn sandbox(self_exe: Option<&CStr>) {
    unsafe {
        ns_security_win32_mitigations_init(0);
        ns_security_sandbox_init(opt(self_exe));
        ns_security_seccomp_init();
    }
}

pub fn shutdown() {
    southstar_bytecode_cache::shutdown();
    unsafe {
        ns_font_shutdown();
        ns_history_shutdown();
        ns_cache_shutdown();
        ns_net_shutdown();
        ns_config_shutdown();
    }
}

pub fn webgl_take_pending_origin() -> Option<GStr> {
    // SAFETY: the call has no preconditions and returns a newly allocated string or null.
    let origin = unsafe { ns_webgl_take_pending_origin() };
    // SAFETY: the string is g_malloc'd and owned by nothing else.
    unsafe { GStr::take(origin) }
}

pub fn webgl_set_decision(origin: &CStr, allow: bool) {
    // SAFETY: `origin` is NUL-terminated for the call.
    unsafe { ns_webgl_set_decision(origin.as_ptr(), c_int::from(allow)) };
}

pub fn camera_take_pending_origin() -> Option<GStr> {
    // SAFETY: the call has no preconditions and returns a newly allocated string or null.
    let origin = unsafe { ns_camera_take_pending_origin() };
    // SAFETY: the string is g_malloc'd and owned by nothing else.
    unsafe { GStr::take(origin) }
}

pub fn camera_set_decision(origin: &CStr, allow: bool) {
    // SAFETY: `origin` is NUL-terminated for the call.
    unsafe { ns_camera_set_decision(origin.as_ptr(), c_int::from(allow)) };
}
