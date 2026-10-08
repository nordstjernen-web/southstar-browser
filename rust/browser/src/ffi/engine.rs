//! Southstar — the pipeline calls of the page lifecycle: cascade and relayout, captures, CSS state, layout, paint, selection and the image, video and animation caches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, GPtrArray};
use southstar_layout::{BoxRef, NsBox, Style};

use super::glib::{GString, StrBufOwned};
use super::{Js, NsBrowser};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PrintSetup {
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

pub mod selection {
    use core::ffi::c_char;

    use southstar_glib::GBoolean;
    use southstar_layout::NsBox;

    unsafe extern "C" {
        pub fn ns_selection_clear(sel: *mut core::ffi::c_void);
        pub fn ns_selection_has_range(sel: *const core::ffi::c_void) -> GBoolean;
        pub fn ns_selection_select_all(sel: *mut core::ffi::c_void, root: *const NsBox)
        -> GBoolean;
        pub fn ns_selection_collect_text(
            root: *const NsBox,
            sel: *const core::ffi::c_void,
        ) -> *mut c_char;
        pub fn ns_selection_bounds(
            root: *const NsBox,
            sel: *const core::ffi::c_void,
            x: *mut f64,
            y: *mut f64,
            w: *mut f64,
            h: *mut f64,
        ) -> GBoolean;
    }
}

unsafe extern "C" {
    fn ns_engine_relayout(
        doc: *mut NsNode,
        base_url: *const c_char,
        viewport_width: c_int,
        viewport_height: f64,
        images: *mut c_void,
        anim: *mut c_void,
        js: *mut c_void,
        css_cache: *mut GHashTable,
        focused: *const NsNode,
        hover: *const NsNode,
        caret_byte: usize,
        sel_anchor_byte: usize,
        out_layout: *mut *mut NsBox,
    ) -> *mut GHashTable;
    fn ns_engine_compute_cascade(
        doc: *mut NsNode,
        base_url: *const c_char,
        css_cache: *mut GHashTable,
        anim: *mut c_void,
    ) -> *mut GHashTable;
    fn ns_engine_load_keyframes(
        anim: *mut c_void,
        doc: *mut NsNode,
        base_url: *const c_char,
        css_cache: *mut GHashTable,
    );
    fn ns_engine_anim_observe(anim: *mut c_void, styles: *mut GHashTable, now_us: i64);
    fn ns_engine_speculative_preload(
        doc: *mut NsNode,
        base_url: *const c_char,
        include_images: GBoolean,
    );
    fn ns_engine_img_session_outstanding(s: *const c_void) -> c_int;
    fn ns_engine_img_session_close(s: *mut c_void);
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_engine_write_png(root: *const NsBox, path: *const c_char) -> c_int;
    fn ns_engine_write_pdf(root: *const NsBox, path: *const c_char) -> c_int;
    fn ns_engine_dump_text(root: *const NsBox, out: *mut GString);
    fn ns_engine_dump_layout(root: *const NsBox, indent: c_int, out: *mut GString);
    fn ns_engine_print_recordings(root: *const NsBox, setup: *const PrintSetup) -> *mut GPtrArray;
    fn ns_render_page_rule() -> *const c_void;
    fn ns_print_setup_default(setup: *mut PrintSetup);
    fn ns_print_setup_apply_page_rule(setup: *mut PrintSetup, rule: *const c_void);
    fn ns_css_set_viewport(vw: f64, vh: f64);
    fn ns_css_set_doc_language(lang: *const c_char);
    fn ns_css_set_target_fragment(fragment: *const c_char);
    fn ns_css_device_pixel_ratio() -> f64;
    fn ns_css_set_device_pixel_ratio(dppx: f64);
    fn ns_css_set_print_media(printing: GBoolean);
    fn ns_css_set_active_node(node: *const NsNode) -> *const NsNode;
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
    fn ns_layout_set_open_select(select: *const NsNode);
    fn ns_layout_set_datalist_open(open: GBoolean);
    pub fn ns_box_free(b: *mut NsBox);
    fn ns_box_inline_rect_for_dom(
        root: *const NsBox,
        target: *const NsNode,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    ) -> GBoolean;
    fn ns_paint_3d_invalidate();
    fn ns_paint_set_js(js: *mut c_void);
    fn ns_paint_set_anim(anim: *mut c_void);
    fn ns_anim_new() -> *mut c_void;
    fn ns_anim_free(a: *mut c_void);
    fn ns_anim_tick(a: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_anim_has_active(a: *const c_void) -> GBoolean;
    fn ns_anim_needs_layout(a: *const c_void) -> GBoolean;
    fn ns_image_cache_new() -> *mut c_void;
    fn ns_image_cache_free(c: *mut c_void);
    fn ns_image_cache_tick(c: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_image_cache_has_pending(c: *const c_void) -> GBoolean;
    fn ns_image_cache_animating(c: *const c_void) -> GBoolean;
    fn ns_video_cache_new() -> *mut c_void;
    fn ns_video_cache_free(c: *mut c_void);
    fn ns_video_cache_set_base(c: *mut c_void, base_url: *const c_char);
    fn ns_video_cache_discover(c: *mut c_void, root: *const NsBox, doc: *const NsNode, now_us: i64);
    fn ns_video_cache_note_layout(
        c: *mut c_void,
        root: *const NsBox,
        scroll_x: f64,
        scroll_y: f64,
        scale: f64,
    );
    fn ns_video_cache_tick(c: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_video_cache_has_pending(c: *const c_void) -> GBoolean;
    fn ns_video_cache_animating(c: *const c_void) -> GBoolean;
    fn ns_video_cache_waiting_growth(c: *const c_void) -> GBoolean;
    fn ns_video_cache_seek_node(
        c: *mut c_void,
        node: *const c_void,
        seconds: f64,
        now_us: i64,
    ) -> GBoolean;
    fn ns_video_cache_set_node_playing(
        c: *mut c_void,
        node: *const c_void,
        play: GBoolean,
        now_us: i64,
    ) -> GBoolean;
    fn ns_video_cache_set_node_muted(
        c: *mut c_void,
        node: *const c_void,
        muted: GBoolean,
    ) -> GBoolean;
    fn ns_video_cache_set_node_volume(c: *mut c_void, node: *const c_void, volume: f64)
    -> GBoolean;
    pub fn ns_video_cache_mse_append(
        c: *mut c_void,
        id: c_uint,
        kind: c_char,
        data: *const u8,
        len: usize,
    ) -> GBoolean;
    pub fn ns_video_cache_mse_eos(c: *mut c_void, id: c_uint);
    pub fn ns_video_cache_mse_buffered(
        c: *mut c_void,
        id: c_uint,
        kind: c_char,
        start: *mut f64,
    ) -> f64;
    pub fn ns_video_cache_mse_remove(
        c: *mut c_void,
        id: c_uint,
        kind: c_char,
        start: f64,
        end: f64,
    ) -> GBoolean;
    pub fn ns_video_cache_mse_bytes(c: *mut c_void, id: c_uint, kind: c_char) -> usize;
    fn ns_video_cache_helper_event(
        c: *mut c_void,
        token: *const c_char,
        kind: *const c_char,
    ) -> GBoolean;
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
    fn ns_bytecode_cache_init();
    fn ns_bytecode_cache_shutdown();
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
    pub fn engine_relayout(&self, focused: Option<Node<'_>>) {
        let styles = unsafe {
            ns_engine_relayout(
                self.doc.get(),
                self.base_url.ptr(),
                self.vw.get(),
                self.vh.get(),
                self.images.get(),
                self.anim.get(),
                self.js.get(),
                self.css_cache.get(),
                Node::ptr_or_null(focused),
                Node::ptr_or_null(self.hover_node.get()),
                self.caret_byte.get(),
                self.sel_anchor_byte.get(),
                self.layout.as_ptr(),
            )
        };
        self.styles.set(styles);
    }

    pub fn compute_cascade(&self) {
        let styles = unsafe {
            ns_engine_compute_cascade(
                self.doc.get(),
                self.base_url.ptr(),
                self.css_cache.get(),
                ptr::null_mut(),
            )
        };
        self.styles.set(styles);
    }

    pub fn load_keyframes(&self, anim: Anim) {
        unsafe {
            ns_engine_load_keyframes(
                anim.raw(),
                self.doc.get(),
                self.base_url.ptr(),
                self.css_cache.get(),
            )
        };
        unsafe { ns_engine_anim_observe(anim.raw(), self.styles.get(), super::monotonic_us()) };
    }

    pub fn speculative_preload(&self) {
        unsafe { ns_engine_speculative_preload(self.doc.get(), self.base_url.ptr(), 0) };
    }

    pub fn styles_raw(&self) -> *mut GHashTable {
        self.styles.get()
    }

    pub fn set_open_select_for_layout(&self) {
        unsafe { ns_layout_set_open_select(Node::ptr_or_null(self.open_select.get())) };
    }

    pub fn print_recordings(&self, setup: &PrintSetup) -> *mut GPtrArray {
        unsafe { ns_engine_print_recordings(self.layout.get(), setup) }
    }
}

#[derive(Clone, Copy)]
pub struct Anim(NonNull<c_void>);

impl Anim {
    pub fn from_raw(p: *mut c_void) -> Option<Anim> {
        NonNull::new(p).map(Anim)
    }

    pub fn create() -> Option<Anim> {
        Anim::from_raw(unsafe { ns_anim_new() })
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_anim_free(self.raw()) };
    }

    pub fn tick(self, now_us: i64) -> bool {
        unsafe { ns_anim_tick(self.raw(), now_us) != 0 }
    }

    pub fn has_active(self) -> bool {
        unsafe { ns_anim_has_active(self.raw()) != 0 }
    }

    pub fn needs_layout(self) -> bool {
        unsafe { ns_anim_needs_layout(self.raw()) != 0 }
    }
}

#[derive(Clone, Copy)]
pub struct Images(NonNull<c_void>);

impl Images {
    pub fn from_raw(p: *mut c_void) -> Option<Images> {
        NonNull::new(p).map(Images)
    }

    pub fn create() -> *mut c_void {
        unsafe { ns_image_cache_new() }
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_image_cache_free(self.raw()) };
    }

    pub fn tick(self, now_us: i64) -> bool {
        unsafe { ns_image_cache_tick(self.raw(), now_us) != 0 }
    }

    pub fn has_pending(self) -> bool {
        unsafe { ns_image_cache_has_pending(self.raw()) != 0 }
    }

    pub fn animating(self) -> bool {
        unsafe { ns_image_cache_animating(self.raw()) != 0 }
    }
}

#[derive(Clone, Copy)]
pub struct Videos(NonNull<c_void>);

impl Videos {
    pub fn from_raw(p: *mut c_void) -> Option<Videos> {
        NonNull::new(p).map(Videos)
    }

    pub fn create() -> *mut c_void {
        unsafe { ns_video_cache_new() }
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_video_cache_free(self.raw()) };
    }

    pub fn set_base(self, base: Option<&CStr>) {
        unsafe { ns_video_cache_set_base(self.raw(), opt(base)) };
    }

    pub fn discover(self, layout: BoxRef<'_>, doc: Option<Node<'_>>, now_us: i64) {
        unsafe {
            ns_video_cache_discover(self.raw(), layout.as_ptr(), Node::ptr_or_null(doc), now_us)
        };
    }

    pub fn note_layout(self, layout: BoxRef<'_>, scroll_x: f64, scroll_y: f64, scale: f64) {
        unsafe {
            ns_video_cache_note_layout(self.raw(), layout.as_ptr(), scroll_x, scroll_y, scale)
        };
    }

    pub fn tick(self, now_us: i64) -> bool {
        unsafe { ns_video_cache_tick(self.raw(), now_us) != 0 }
    }

    pub fn has_pending(self) -> bool {
        unsafe { ns_video_cache_has_pending(self.raw()) != 0 }
    }

    pub fn animating(self) -> bool {
        unsafe { ns_video_cache_animating(self.raw()) != 0 }
    }

    pub fn waiting_growth(self) -> bool {
        unsafe { ns_video_cache_waiting_growth(self.raw()) != 0 }
    }

    pub fn seek_node(self, node: *const c_void, seconds: f64, now_us: i64) -> bool {
        unsafe { ns_video_cache_seek_node(self.raw(), node, seconds, now_us) != 0 }
    }

    pub fn set_node_playing(self, node: *const c_void, play: bool, now_us: i64) {
        unsafe {
            ns_video_cache_set_node_playing(self.raw(), node, southstar_glib::boolean(play), now_us)
        };
    }

    pub fn set_node_muted(self, node: *const c_void, muted: bool) {
        unsafe { ns_video_cache_set_node_muted(self.raw(), node, southstar_glib::boolean(muted)) };
    }

    pub fn set_node_volume(self, node: *const c_void, volume: f64) {
        unsafe { ns_video_cache_set_node_volume(self.raw(), node, volume) };
    }

    pub unsafe fn helper_event(self, token: *const c_char, kind: *const c_char) -> bool {
        unsafe { ns_video_cache_helper_event(self.raw(), token, kind) != 0 }
    }
}

#[derive(Clone, Copy)]
pub struct Session(pub(super) *mut c_void);

impl Session {
    pub fn from_raw(p: *mut c_void) -> Option<Session> {
        (!p.is_null()).then_some(Session(p))
    }

    pub fn outstanding(self) -> c_int {
        unsafe { ns_engine_img_session_outstanding(self.0) }
    }

    pub fn close(self) {
        unsafe { ns_engine_img_session_close(self.0) };
    }
}

pub unsafe fn box_free(b: *mut NsBox) {
    unsafe { ns_box_free(b) };
}

pub fn engine_in_blocking_fetch() -> bool {
    unsafe { ns_engine_in_blocking_fetch() != 0 }
}

pub fn write_capture(layout: BoxRef<'_>, path: &CStr, pdf: bool) -> c_int {
    if pdf {
        unsafe { ns_engine_write_pdf(layout.as_ptr(), path.as_ptr()) }
    } else {
        unsafe { ns_engine_write_png(layout.as_ptr(), path.as_ptr()) }
    }
}

pub fn dump_text(layout: BoxRef<'_>, out: &StrBufOwned) {
    unsafe { ns_engine_dump_text(layout.as_ptr(), out.raw()) };
}

pub fn dump_layout(layout: BoxRef<'_>, out: &StrBufOwned) {
    unsafe { ns_engine_dump_layout(layout.as_ptr(), 0, out.raw()) };
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
    unsafe { ns_css_set_viewport(vw, vh) };
}

pub fn css_set_doc_language(lang: Option<&CStr>) {
    unsafe { ns_css_set_doc_language(opt(lang)) };
}

pub fn css_set_target_fragment(fragment: Option<&CStr>) {
    unsafe { ns_css_set_target_fragment(opt(fragment)) };
}

pub fn css_device_pixel_ratio() -> f64 {
    unsafe { ns_css_device_pixel_ratio() }
}

pub fn css_set_device_pixel_ratio(dppx: f64) {
    unsafe { ns_css_set_device_pixel_ratio(dppx) };
}

pub fn css_set_print_media(printing: bool) {
    unsafe { ns_css_set_print_media(southstar_glib::boolean(printing)) };
}

pub fn css_set_active_node(node: Option<Node<'_>>) {
    unsafe { ns_css_set_active_node(Node::ptr_or_null(node)) };
}

pub fn css_prop_id(name: &CStr) -> c_int {
    unsafe { ns_css_prop_id(name.as_ptr()) }
}

pub fn style_keyword(b: BoxRef<'_>, prop: c_int) -> Option<&CStr> {
    let kw = unsafe { ns_style_keyword(b.style(), prop) };
    (!kw.is_null()).then(|| unsafe { CStr::from_ptr(kw) })
}

pub fn layout_set_datalist_open(open: bool) {
    unsafe { ns_layout_set_datalist_open(southstar_glib::boolean(open)) };
}

pub fn inline_rect_y(layout: BoxRef<'_>, target: Node<'_>) -> Option<f64> {
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    let ok = unsafe {
        ns_box_inline_rect_for_dom(
            layout.as_ptr(),
            target.as_ptr(),
            &mut x,
            &mut y,
            &mut w,
            &mut h,
        )
    };
    (ok != 0).then_some(y)
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn paint_3d_invalidate() {
    unsafe { ns_paint_3d_invalidate() };
}

pub fn paint_set_js(js: Option<Js>) {
    unsafe { ns_paint_set_js(js.map_or(ptr::null_mut(), Js::raw)) };
}

pub fn paint_set_anim(anim: Option<Anim>) {
    unsafe { ns_paint_set_anim(anim.map_or(ptr::null_mut(), Anim::raw)) };
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
        ns_bytecode_cache_init();
        ns_history_init();
        ns_font_init();
        ns_spell_init();
    }
}

pub unsafe fn sandbox(self_exe: *const c_char) {
    unsafe {
        ns_security_win32_mitigations_init(0);
        ns_security_sandbox_init(self_exe);
        ns_security_seccomp_init();
    }
}

pub fn shutdown() {
    unsafe {
        ns_font_shutdown();
        ns_bytecode_cache_shutdown();
        ns_history_shutdown();
        ns_cache_shutdown();
        ns_net_shutdown();
        ns_config_shutdown();
    }
}

pub fn webgl_take_pending_origin() -> *mut c_char {
    unsafe { ns_webgl_take_pending_origin() }
}

pub unsafe fn webgl_set_decision(origin: *const c_char, allow: c_int) {
    unsafe { ns_webgl_set_decision(origin, allow) };
}

pub fn camera_take_pending_origin() -> *mut c_char {
    unsafe { ns_camera_take_pending_origin() }
}

pub unsafe fn camera_set_decision(origin: *const c_char, allow: c_int) {
    unsafe { ns_camera_set_decision(origin, allow) };
}
