//! Southstar — the C ABI of the renderer host, as declared in src/renderer_tiles.h, over the engine's layer and paint calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::tiles::{self, LayerPlan, LayerTarget, Page, Sticky, TileTarget, Tiles, View, VpLayer};

#[repr(C)]
struct GArray {
    data: *mut c_char,
    len: c_uint,
}

#[repr(C)]
pub struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

#[repr(C)]
pub struct PaintLayerPlan {
    dynamic: GBoolean,
    kinds: *mut c_void,
    vp: *mut GArray,
}

#[repr(C)]
#[derive(Default)]
struct StickyY {
    has_top: GBoolean,
    has_bottom: GBoolean,
    top_start: f64,
    top_cap: f64,
    bottom_start: f64,
    bottom_cap: f64,
}

#[repr(C)]
#[derive(Default)]
struct VpLayerInfo {
    kind: c_int,
    top: f64,
    bottom: f64,
    x_offset: f64,
    sticky: StickyY,
}

#[repr(C)]
pub struct Browser {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn ns_paint_layer_plan_init(plan: *mut PaintLayerPlan);
    fn ns_paint_layer_plan_clear(plan: *mut PaintLayerPlan);
    fn ns_browser_note_viewport(
        browser: *mut Browser,
        scroll_x: c_int,
        scroll_y: c_int,
        height: c_int,
        scale: f64,
    );
    fn ns_browser_flush_video_rects(browser: *mut Browser);
    fn ns_browser_layers_prepare(
        browser: *mut Browser,
        scroll_x: c_int,
        scroll_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        plan: *mut PaintLayerPlan,
    ) -> c_int;
    fn ns_browser_render_doc_tile(
        browser: *mut Browser,
        plan: *const PaintLayerPlan,
        scroll_x: c_int,
        tile_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        bufs: *const *mut u8,
        stride: c_int,
        upper_used: *mut GBoolean,
    ) -> c_int;
    fn ns_browser_vp_layer_info(
        browser: *mut Browser,
        plan: *const PaintLayerPlan,
        index: c_int,
        out: *mut VpLayerInfo,
    ) -> c_int;
    fn ns_browser_render_vp_layer(
        browser: *mut Browser,
        plan: *const PaintLayerPlan,
        index: c_int,
        scroll_x: c_int,
        scroll_y: c_int,
        origin_y: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
        out: *mut u8,
        stride: c_int,
    ) -> c_int;
    fn ns_browser_canvas_color(browser: *mut Browser, rgba_out: *mut f64) -> GBoolean;
    fn ns_browser_scroller_rects(browser: *mut Browser, out: *mut GString, max_rects: c_int);
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(string: *mut GString, free_segment: GBoolean) -> *mut c_char;
    fn g_string_append_len(string: *mut GString, val: *const c_char, len: isize) -> *mut GString;
}

pub struct Plan(Box<PaintLayerPlan>);

impl Plan {
    fn new() -> Plan {
        let mut plan = Box::new(PaintLayerPlan {
            dynamic: 0,
            kinds: ptr::null_mut(),
            vp: ptr::null_mut(),
        });
        unsafe { ns_paint_layer_plan_init(&mut *plan) };
        Plan(plan)
    }
}

impl Drop for Plan {
    fn drop(&mut self) {
        unsafe { ns_paint_layer_plan_clear(&mut *self.0) };
    }
}

impl LayerPlan for Plan {
    fn vp_count(&self) -> usize {
        unsafe { self.0.vp.as_ref() }.map_or(0, |vp| vp.len as usize)
    }
}

struct Engine(*mut Browser);

impl Page for Engine {
    type Plan = Plan;

    fn note_viewport(&mut self, sx: c_int, sy: c_int, height: c_int, scale: f64) {
        unsafe { ns_browser_note_viewport(self.0, sx, sy, height, scale) };
    }

    fn prepare_layers(
        &mut self,
        plan: &mut Plan,
        sx: c_int,
        sy: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
    ) -> c_int {
        unsafe { ns_browser_layers_prepare(self.0, sx, sy, width, height, scale, &mut *plan.0) }
    }

    fn vp_layer(&mut self, plan: &Plan, index: usize) -> Option<VpLayer> {
        let mut info = VpLayerInfo::default();
        if unsafe { ns_browser_vp_layer_info(self.0, &*plan.0, index as c_int, &mut info) } != 0 {
            return None;
        }
        Some(VpLayer {
            kind: info.kind,
            top: info.top,
            bottom: info.bottom,
            x_offset: info.x_offset,
            sticky: Sticky {
                has_top: info.sticky.has_top != 0,
                has_bottom: info.sticky.has_bottom != 0,
                top_start: info.sticky.top_start,
                top_cap: info.sticky.top_cap,
                bottom_start: info.sticky.bottom_start,
                bottom_cap: info.sticky.bottom_cap,
            },
        })
    }

    fn render_doc_tile(&mut self, plan: &Plan, target: TileTarget<'_, '_>) -> c_int {
        let bufs: Vec<*mut u8> = target.bufs.iter_mut().map(|buf| buf.as_mut_ptr()).collect();
        unsafe {
            ns_browser_render_doc_tile(
                self.0,
                &*plan.0,
                target.sx,
                target.tile_y,
                target.width,
                target.height,
                target.scale,
                bufs.as_ptr(),
                target.stride,
                target.upper_used.as_mut_ptr(),
            )
        }
    }

    fn render_vp_layer(&mut self, plan: &Plan, index: usize, target: LayerTarget<'_>) -> c_int {
        unsafe {
            ns_browser_render_vp_layer(
                self.0,
                &*plan.0,
                index as c_int,
                target.sx,
                target.sy,
                target.origin_y,
                target.width,
                target.height,
                target.scale,
                target.out.as_mut_ptr(),
                target.stride,
            )
        }
    }

    fn canvas_color(&mut self, rgba: &mut [f64; 4]) {
        unsafe { ns_browser_canvas_color(self.0, rgba.as_mut_ptr()) };
    }

    fn scroller_rects(&mut self, out: &mut Vec<u8>, max_rects: c_int) {
        unsafe {
            let rects = g_string_new(ptr::null());
            ns_browser_scroller_rects(self.0, rects, max_rects);
            if !(*rects).str.is_null() {
                out.extend_from_slice(core::slice::from_raw_parts(
                    (*rects).str.cast(),
                    (*rects).len,
                ));
            }
            g_string_free(rects, 1);
        }
    }

    fn flush_video_rects(&mut self) {
        unsafe { ns_browser_flush_video_rects(self.0) };
    }
}

unsafe fn body<'a>(body: *const c_char) -> &'a [u8] {
    unsafe { glib::bytes(body) }.unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_tiles_new() -> *mut Tiles<Plan> {
    Box::into_raw(Box::new(Tiles::new(Plan::new())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tiles_free(t: *mut Tiles<Plan>) {
    if !t.is_null() {
        drop(unsafe { Box::from_raw(t) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tiles_requested(body_text: *const c_char) -> GBoolean {
    glib::boolean(tiles::tiles_requested(unsafe { body(body_text) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tiles_render(
    t: *mut Tiles<Plan>,
    b: *mut Browser,
    body_text: *const c_char,
    view: *const View,
    invalid: GBoolean,
    fb: *mut u8,
    fb_size: usize,
    desc: *mut GString,
) -> c_int {
    let (Some(t), Some(view)) = (unsafe { t.as_mut() }, unsafe { view.as_ref() }) else {
        return -1;
    };
    if b.is_null() {
        return -1;
    }
    let fb: &mut [u8] = if fb.is_null() {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(fb, fb_size) }
    };
    let mut text = Vec::new();
    let ok = t.render(
        &mut Engine(b),
        unsafe { body(body_text) },
        view,
        invalid != 0,
        fb,
        &mut text,
    );
    if !desc.is_null() && !text.is_empty() {
        unsafe { g_string_append_len(desc, text.as_ptr().cast(), text.len() as isize) };
    }
    if ok { 0 } else { -1 }
}
