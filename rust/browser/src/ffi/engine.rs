//! Southstar — the pipeline calls of the page lifecycle: cascade and relayout, captures, CSS state, layout, paint, selection and the image, video and animation caches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::rc::Rc;

use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_layout::{BoxPool, BoxRef, NsBox, Style};

use super::glib::StrBufOwned;
use super::{Js, NsBrowser};

pub use southstar_anim::AnimHandle as Anim;
pub use southstar_engine::{ImageSession as Session, PrintSetup};
pub use southstar_image::ImageCache as Images;
pub use southstar_video::VideoCache as Videos;

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
            js: self.js.get().map(Js::handle),
            focused,
            hover: self.hover_node.get(),
            caret_byte: self.caret_byte.get(),
            sel_anchor_byte: self.sel_anchor_byte.get(),
        };
        // SAFETY: the layout slot holds null or a box of this thread's layout store.
        let styles = unsafe { southstar_engine::relayout(&relayout, &self.layout.0) };
        self.styles.replace(styles);
    }

    pub fn compute_cascade(&self) {
        let base = self.base_url.get();
        let styles = southstar_engine::compute_cascade(&self.engine_page(base.as_deref()));
        self.styles.replace(Some(styles));
    }

    pub fn load_keyframes(&self, anim: Anim) {
        let base = self.base_url.get();
        southstar_engine::load_keyframes(anim, &self.engine_page(base.as_deref()));
        let styles = self.styles();
        southstar_engine::anim_observe(anim, styles.as_deref(), super::monotonic_us());
    }

    pub fn speculative_preload(&self) {
        let base = self.base_url.get();
        if let (Some(doc), Some(base)) = (self.doc(), base.as_deref()) {
            southstar_engine::speculative_preload(doc, base, false);
        }
    }

    pub fn styles(&self) -> Option<Rc<StyleMap>> {
        self.styles.borrow().clone()
    }

    pub fn has_styles(&self) -> bool {
        self.styles.borrow().is_some()
    }

    pub fn set_open_select_for_layout(&self) {
        southstar_layout_build::set_open_select(self.open_select.get());
    }
}

pub unsafe fn box_free(b: *mut NsBox) {
    // SAFETY: the caller hands over null or a layout tree it owns and no longer reads.
    BoxPool::free_tree(unsafe { BoxRef::from_ptr(b) });
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
    PrintSetup::a4()
}

pub fn apply_page_rule(setup: &mut PrintSetup) -> bool {
    let Some(rule) = southstar_engine::page_rule() else {
        return false;
    };
    setup.apply_page_rule(&rule);
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
    southstar_layout_build::set_datalist_open(open);
}

pub fn inline_rect_y(layout: BoxRef<'_>, target: Node<'_>) -> Option<f64> {
    southstar_layout_query::inline_rect_for_dom(layout, target).map(|(_, y, _, _)| y)
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn paint_3d_invalidate() {
    southstar_paint::invalidate_3d();
}

pub fn paint_set_js(js: Option<Js>) {
    southstar_paint::set_js(js.map(Js::handle));
}

pub fn paint_set_anim(anim: Option<Anim>) {
    southstar_paint::set_anim(anim);
}

pub fn init_subsystems() {
    southstar_config::init();
    if southstar_config::get().is_some_and(|c| c.harden_allocator != 0) {
        southstar_sandbox::harden_allocator();
    }
    southstar_net::init();
    southstar_net::set_allow_file_urls(true);
    southstar_http_cache::init();
    southstar_history::init();
    southstar_font::init();
    southstar_paint::register_font_oracle();
    southstar_spellcheck::init();
    southstar_bytecode_cache::init();
}

fn webgpu_allowed() -> bool {
    std::env::var_os("NS_WEBGPU_ALLOW").is_some_and(|v| v.as_encoded_bytes().first() == Some(&b'1'))
}

pub fn sandbox(self_exe: Option<&CStr>) {
    southstar_sandbox::win32_mitigations_init(false);
    southstar_sandbox::confine_as_renderer();
    if southstar_config::webgl_enabled() || webgpu_allowed() {
        southstar_sandbox::allow_gpu_devices();
    }
    southstar_sandbox::sandbox_init(self_exe);
    southstar_sandbox::seccomp_init();
}

pub fn shutdown() {
    southstar_bytecode_cache::shutdown();
    southstar_font::shutdown();
    southstar_history::shutdown();
    southstar_http_cache::shutdown();
    southstar_net::shutdown();
    southstar_config::shutdown();
}

pub use southstar_webgl::take_pending_origin as webgl_take_pending_origin;

pub fn webgl_set_decision(origin: &CStr, allow: bool) {
    southstar_webgl::set_decision(origin.to_bytes(), allow);
}
