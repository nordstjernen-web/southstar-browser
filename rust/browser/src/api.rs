//! Southstar — the embedding API for Rust callers: an owned page and the process-wide calls, over the same internals as the C ABI of src/libsouthstar.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use std::ffi::CString;

use crate::ffi::{self, NsBrowser, PixelBuf};
use crate::{build, callbacks, hit, input, open, page, query, render, scroll, settle};

pub use crate::ffi::{LayerPlan, PrintPages, PrintSetup, StickyY, VpLayerInfo};
pub use crate::hit::Found;

pub struct Post {
    pub body: Vec<u8>,
    pub content_type: Option<CString>,
}

pub struct Media {
    pub url: Vec<u8>,
    pub is_video: bool,
    pub stream: bool,
}

pub struct TileTarget<'a, 'b> {
    pub scroll_x: c_int,
    pub tile_y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub scale: f64,
    pub bufs: &'a mut [&'b mut [u8]],
    pub stride: c_int,
    pub upper_used: &'a mut [c_int],
}

pub struct LayerTarget<'a> {
    pub scroll_x: c_int,
    pub scroll_y: c_int,
    pub origin_y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub scale: f64,
    pub out: &'a mut [u8],
    pub stride: c_int,
}

pub fn init() {
    build::init();
}

pub fn shutdown() {
    ffi::shutdown();
}

pub fn sandbox(self_exe: Option<&CStr>) {
    ffi::sandbox(self_exe);
}

pub fn set_color_scheme(dark: bool) {
    ffi::css_set_color_scheme(dark);
}

pub fn set_next_referrer(url: Option<&CStr>) {
    open::set_next_referrer(url);
}

pub fn set_next_user_activated(user_activated: bool) {
    open::set_next_user_activated(user_activated);
}

pub fn set_device_pixel_ratio(page: Option<&Page>, dppx: f64) -> bool {
    page::set_device_pixel_ratio(page.map(Page::browser), dppx) > 0
}

pub fn print_setup_default() -> PrintSetup {
    ffi::print_setup_default()
}

fn text(text: Option<&CStr>) -> Option<Vec<u8>> {
    text.map(|text| text.to_bytes().to_vec())
}

pub struct Page(NonNull<NsBrowser>);

impl Page {
    pub(crate) fn adopt(browser: Box<NsBrowser>) -> Page {
        Page(NonNull::from(Box::leak(browser)))
    }

    /// # Safety
    ///
    /// `raw` is null or a pointer `into_raw` returned that has not been turned back into a page; the page takes ownership of it.
    pub(crate) unsafe fn from_raw(raw: *mut NsBrowser) -> Option<Page> {
        NonNull::new(raw).map(Page)
    }

    pub(crate) fn into_raw(self) -> *mut NsBrowser {
        ManuallyDrop::new(self).0.as_ptr()
    }

    pub(crate) fn browser(&self) -> &NsBrowser {
        // SAFETY: the page owns the allocation, which stays at this address until the page drops.
        unsafe { self.0.as_ref() }
    }

    pub fn open(
        url: &CStr,
        width: c_int,
        height: f64,
        settle_ms: c_int,
        post: Option<&Post>,
    ) -> Option<Page> {
        let view = open::Viewport {
            width,
            height,
            settle_ms,
        };
        let post = post.map(|post| ffi::Post {
            body: post.body.as_ptr().cast(),
            len: post.body.len(),
            content_type: post.content_type.as_deref(),
        });
        open::open(url, &view, post.as_ref())
    }

    pub fn busy(&self) -> bool {
        settle::busy(self.browser())
    }

    pub fn take_post(&self) -> Option<Post> {
        let (body, content_type) = self.browser().take_post()?;
        Some(Post {
            body,
            content_type: content_type.map(|ct| CString::from(&*ct)),
        })
    }

    pub fn url(&self) -> Option<Vec<u8>> {
        text(self.browser().base_url.get().as_deref())
    }

    pub fn title(&self) -> Option<Vec<u8>> {
        query::title(self.browser())
    }

    pub fn links(&self) -> Option<Vec<u8>> {
        query::links(self.browser())
    }

    pub fn favicon_url(&self) -> Option<Vec<u8>> {
        query::favicon_url(self.browser())
    }

    pub fn render_text(&self) -> Option<Vec<u8>> {
        query::dump_text(self.browser()).map(|text| text.bytes().to_vec())
    }

    pub fn dump_dom(&self) -> Option<Vec<u8>> {
        query::dump_dom(self.browser())
    }

    pub fn dump_layout(&self) -> Option<Vec<u8>> {
        query::dump_layout(self.browser()).map(|text| text.bytes().to_vec())
    }

    pub fn dump_performance(&self) -> Vec<u8> {
        query::dump_performance(self.browser()).bytes().to_vec()
    }

    pub fn console_drain(&self) -> Option<Vec<u8>> {
        self.browser().console_buf.take_bytes()
    }

    pub fn take_pending_nav(&self) -> Option<Vec<u8>> {
        text(self.browser().pending_nav.take().as_deref())
    }

    pub fn take_pending_webgl(&self) -> Option<Vec<u8>> {
        text(ffi::webgl_take_pending_origin().as_deref())
    }

    pub fn take_pending_camera(&self) -> Option<Vec<u8>> {
        text(ffi::camera_take_pending_origin().as_deref())
    }

    pub fn take_pending_download(&self) -> Option<Vec<u8>> {
        text(self.browser().pending_download.take().as_deref())
    }

    pub fn take_pending_audio(&self) -> Option<Vec<u8>> {
        self.browser().pending_audio.take_bytes()
    }

    pub fn take_pending_window_action(&self) -> Option<Vec<u8>> {
        text(self.browser().pending_window_action.take().as_deref())
    }

    pub fn has_pending_clipboard(&self) -> bool {
        self.browser().pending_clipboard.is_set()
    }

    pub fn take_pending_clipboard(&self) -> Option<Vec<u8>> {
        text(self.browser().pending_clipboard.take().as_deref())
    }

    pub fn take_soft_nav_pushed(&self) -> bool {
        self.browser().soft_nav_pushed.take()
    }

    pub fn take_pending_scroll(&self) -> Option<(c_int, c_int)> {
        scroll::take_pending(self.browser())
    }

    pub fn render_image(&self, path: &CStr) -> c_int {
        query::render_image(self.browser(), path)
    }

    pub fn page_size(&self) -> Option<(c_int, c_int)> {
        page::page_size(self.browser())
    }

    pub fn snap_document(
        &self,
        viewport: (f64, f64),
        previous: (c_int, c_int),
        scroll: (c_int, c_int),
    ) -> Option<(c_int, c_int)> {
        render::snap_document(self.browser(), viewport, previous, scroll)
    }

    pub fn set_viewport(&self, width: c_int, height: f64) -> bool {
        page::set_viewport(self.browser(), width, height) == 0
    }

    pub fn window_action_applied(&self) {
        if let Some(js) = self.browser().js() {
            js.window_action_applied();
        }
    }

    pub fn render_argb32(
        &self,
        scroll_x: c_int,
        scroll_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        out: &mut [u8],
    ) -> c_int {
        match PixelBuf::from_slice(out, width, height, width.wrapping_mul(4)) {
            Some(buf) => render::render_argb32(self.browser(), scroll_x, scroll_y, scale, &buf),
            None => -1,
        }
    }

    pub fn link_at(&self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        text(hit::link_near(self.browser(), x, y, 9).as_deref())
    }

    pub fn link_under(&self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        text(hit::link_near(self.browser(), x, y, 1).as_deref())
    }

    pub fn cursor_at(&self, x: c_int, y: c_int) -> Option<Vec<u8>> {
        hit::cursor_at(self.browser(), x, y).map(<[u8]>::to_vec)
    }

    pub fn press(&self, x: c_int, y: c_int, mods: c_int) -> Option<Vec<u8>> {
        text(input::press(self.browser(), x, y, mods).as_deref())
    }

    pub fn release_click(&self) -> (Option<Vec<u8>>, bool) {
        let (nav, changed) = input::release_click(self.browser());
        (text(nav.as_deref()), changed)
    }

    pub fn select(&self, kind: c_int, x: c_int, y: c_int) -> Option<Vec<u8>> {
        match input::select(self.browser(), kind, x, y) {
            Some(input::Selected::Text(text)) => Some(text),
            _ => None,
        }
    }

    pub fn key(
        &self,
        kind: c_int,
        key: &CStr,
        code: &CStr,
        keycode: c_int,
        mods: c_int,
    ) -> (Option<Vec<u8>>, bool) {
        let event = input::KeyEvent {
            kind,
            key: Some(key),
            code: Some(code),
            keycode,
            mods,
        };
        let (nav, prevented) = input::key(self.browser(), &event);
        (text(nav.as_deref()), prevented)
    }

    pub fn focused_editable(&self) -> bool {
        query::focused_editable(self.browser())
    }

    pub fn focused_editable_value(&self) -> Option<(Vec<u8>, usize, usize)> {
        query::focused_editable_value(self.browser())
    }

    pub fn set_focused_editable_selection(&self, caret: usize, anchor: usize) -> bool {
        query::set_focused_editable_selection(self.browser(), caret, anchor)
    }

    pub fn hover(&self, x: c_int, y: c_int) -> bool {
        input::hover(self.browser(), x, y) > 0
    }

    pub fn scroll_at(&self, x: c_int, y: c_int, dx: c_int, dy: c_int) -> (bool, bool) {
        let (consumed, snapped) = input::scroll_at(self.browser(), x, y, dx, dy);
        (consumed != 0, consumed != 0 && snapped)
    }

    pub fn scrollbar_press(&self, x: c_int, y: c_int) -> bool {
        input::scrollbar_press(self.browser(), x, y) != 0
    }

    pub fn scrollbar_drag(&self, y: c_int) -> bool {
        input::scrollbar_drag(self.browser(), y) != 0
    }

    pub fn scrollbar_release(&self) {
        input::scrollbar_release(self.browser());
    }

    pub fn drop_files(&self, x: c_int, y: c_int, paths: &[&CStr]) -> bool {
        !paths.is_empty() && input::drop_files(self.browser(), x, y, paths) > 0
    }

    pub fn video_helper_event(&self, token: Option<&CStr>, kind: &CStr) -> bool {
        self.browser()
            .videos()
            .zip(token)
            .is_some_and(|(videos, token)| videos.helper_event(token.to_bytes(), kind.to_bytes()))
    }

    pub fn eval(&self, src: &CStr) -> Option<Vec<u8>> {
        text(input::eval(self.browser(), src).as_deref())
    }

    pub fn media_at(&self, x: c_int, y: c_int) -> Option<Media> {
        let found = hit::media_at(self.browser(), x, y)?;
        Some(Media {
            url: found.url.to_bytes().to_vec(),
            is_video: found.is_video,
            stream: found.stream,
        })
    }

    pub fn contextmenu(&self, x: c_int, y: c_int) -> (bool, c_int) {
        let (prevented, edit_state) = input::contextmenu(self.browser(), x, y);
        (prevented != 0, edit_state)
    }

    pub fn find(
        &self,
        query: &CStr,
        case_sensitive: bool,
        direction: c_int,
        from_y: c_int,
    ) -> Option<Found> {
        hit::find(
            self.browser(),
            Some(query),
            case_sensitive,
            direction,
            from_y,
        )
    }

    pub fn tick(&self, budget_ms: c_int) -> bool {
        settle::tick(self.browser(), budget_ms)
    }

    pub fn animating(&self) -> bool {
        settle::animating(self.browser())
    }

    pub fn set_caret_blink_active(&self, active: bool) -> bool {
        query::set_caret_blink_active(self.browser(), active)
    }

    pub fn caret_blinking(&self) -> bool {
        self.browser().caret_blink_active.get()
    }

    pub fn security(&self) -> (c_int, Option<Vec<u8>>) {
        let b = self.browser();
        (b.security.get(), text(b.remote_ip.get().as_deref()))
    }

    pub fn resolve_webgl(&self, origin: &CStr, allow: bool) {
        ffi::webgl_set_decision(origin, allow);
    }

    pub fn resolve_camera(&self, origin: &CStr, allow: bool) {
        callbacks::resolve_camera(Some(self.browser()), Some(origin), allow);
    }

    pub fn bfcache_eligible(&self) -> bool {
        self.browser().bfcache_ok.get()
    }

    pub fn bfcache_park(&self) {
        page::bfcache_park(self.browser());
    }

    pub fn bfcache_restore(&self, width: c_int, height: f64) {
        page::bfcache_restore(self.browser(), width, height);
    }

    pub fn print_pages(&self) -> Option<(PrintPages, PrintSetup)> {
        query::print_pages(self.browser())
    }

    pub fn note_viewport(&self, scroll_x: c_int, scroll_y: c_int, height: c_int, scale: f64) {
        render::note_composited_viewport(self.browser(), scroll_x, scroll_y, height, scale);
    }

    pub fn flush_video_rects(&self) {
        render::flush_video_rects(self.browser());
    }

    pub fn prepare_layers(
        &self,
        plan: &mut LayerPlan,
        scroll_x: c_int,
        scroll_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
    ) -> bool {
        render::layers_prepare(
            self.browser(),
            scroll_x,
            scroll_y,
            width,
            height,
            scale,
            plan,
        ) == 0
    }

    pub fn vp_layer(&self, plan: &LayerPlan, index: usize) -> Option<VpLayerInfo> {
        let index = c_int::try_from(index).ok()?;
        let mut info = VpLayerInfo::default();
        (render::vp_layer_info(self.browser(), Some(plan), index, &mut info) == 0).then_some(info)
    }

    pub fn render_doc_tile(&self, plan: &LayerPlan, target: TileTarget<'_, '_>) -> c_int {
        let layers = plan.viewport_layers();
        let needed = usize::try_from(target.stride)
            .ok()
            .zip(usize::try_from(target.height).ok())
            .and_then(|(stride, height)| stride.checked_mul(height));
        let fits = needed.is_some_and(|needed| target.bufs.iter().all(|buf| buf.len() >= needed));
        if !fits || target.bufs.len() <= layers || target.upper_used.len() < layers {
            return -1;
        }
        let bufs: Vec<*mut u8> = target.bufs.iter_mut().map(|buf| buf.as_mut_ptr()).collect();
        let tile = ffi::TileTarget {
            bufs: bufs.as_ptr(),
            used: target.upper_used.as_mut_ptr(),
            width: target.width,
            height: target.height,
            stride: target.stride,
            scroll_x: target.scroll_x,
            tile_y: target.tile_y,
            scale: target.scale,
        };
        render::render_doc_tile(self.browser(), plan, tile)
    }

    pub fn render_vp_layer(
        &self,
        plan: &LayerPlan,
        index: usize,
        target: LayerTarget<'_>,
    ) -> c_int {
        let (Ok(index), Some(out)) = (
            c_int::try_from(index),
            PixelBuf::from_slice(target.out, target.width, target.height, target.stride),
        ) else {
            return -1;
        };
        let layer = render::VpRender {
            index,
            scroll_x: target.scroll_x,
            scroll_y: target.scroll_y,
            origin_y: target.origin_y,
            scale: target.scale,
        };
        render::render_vp_layer(self.browser(), plan, &layer, &out)
    }

    pub fn canvas_color(&self) -> Option<[f64; 4]> {
        let layout = self.browser().layout()?;
        let mut rgba = [0.0; 4];
        ffi::canvas_color(layout, &mut rgba);
        Some(rgba)
    }

    pub fn scroller_rects(&self, max_rects: c_int) -> Option<Vec<u8>> {
        render::scroller_rects(self.browser(), max_rects)
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.browser().release();
        // SAFETY: the allocation came from Box::leak in adopt, and release dropped what still pointed into it.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}
