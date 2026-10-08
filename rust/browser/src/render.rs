//! Southstar — painting a page for the embedder: whole-viewport frames, scroll snapping, layer plans, document tiles, fixed and sticky layers and scroller rectangles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, Ordering};

use southstar_layout::{BoxRef, children};

use crate::ffi::{self, Cairo, NsBrowser, PixelBuf, Plan, Surface, TileTarget, VpLayerInfo};
use crate::{images, page};

const VP_LAYER_PAD: f64 = 24.0;
const FRAME_DUMP_EVERY: u32 = 30;

static FRAME_NO: AtomicU32 = AtomicU32::new(0);

fn non_negative(v: f64) -> f64 {
    if v < 0.0 { 0.0 } else { v }
}

fn valid_scale(scale: f64) -> f64 {
    if scale > 0.0 { scale } else { 1.0 }
}

fn target_ok(buf: &PixelBuf) -> bool {
    buf.width > 0 && buf.height > 0 && buf.stride >= buf.width * 4
}

fn note_videos(b: &NsBrowser) {
    if let (Some(videos), Some(layout)) = (b.videos(), b.layout()) {
        let now = ffi::monotonic_us();
        videos.discover(layout, b.doc(), now);
        if let Some(layout) = b.layout() {
            videos.note_layout(
                layout,
                b.cur_scroll_x.get(),
                b.cur_scroll_y.get(),
                b.cur_scale.get(),
            );
        }
    }
}

fn set_view(b: &NsBrowser, scroll_x: c_int, scroll_y: c_int, height: c_int, scale: f64) {
    b.cur_scroll_x.set(f64::from(scroll_x));
    b.cur_scroll_y.set(f64::from(scroll_y));
    b.cur_scale.set(scale);
    b.cur_viewport_h.set(f64::from(height) / scale);
}

pub fn note_viewport(b: &NsBrowser, scroll_x: c_int, scroll_y: c_int, height: c_int, scale: f64) {
    set_view(b, scroll_x, scroll_y, height, scale);
    ffi::set_hit_viewport(b.cur_scroll_x.get(), b.cur_scroll_y.get());
    images::ensure_images(b);
    note_videos(b);
}

fn paint_begin(b: &NsBrowser) {
    ffi::paint_set_js(b.js());
    ffi::paint_set_anim(b.anim());
    b.paint_search();
    ffi::set_caret_visible(b.caret_paint_visible.get());
}

fn paint_end() {
    ffi::set_caret_visible(true);
    ffi::set_search(false, core::ptr::null());
    ffi::paint_set_anim(None);
    ffi::paint_set_js(None);
}

pub fn render_rgba(
    b: &NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    out: &PixelBuf,
) -> c_int {
    if b.layout().is_none() || !target_ok(out) {
        return -1;
    }
    let scale = valid_scale(scale);
    set_view(b, scroll_x, scroll_y, out.height, scale);
    images::ensure_images(b);
    note_videos(b);
    let Some(surface) = Surface::image(out.width, out.height) else {
        return -1;
    };
    let cr = Cairo::on_surface(&surface, out.width, out.height, scale);
    cr.scale(scale);
    cr.translate(-f64::from(scroll_x), -f64::from(scroll_y));
    ffi::paint_set_js(b.js());
    ffi::paint_set_anim(b.anim());
    b.paint_search();
    if let Some(layout) = b.layout() {
        cr.paint(b, layout);
    }
    b.flush_video_composites(ffi::monotonic_us());
    ffi::set_search(false, core::ptr::null());
    ffi::paint_set_anim(None);
    ffi::paint_set_js(None);
    cr.destroy();
    surface.flush();
    surface.copy_to_rgba(out);
    0
}

fn dump_frame(surface: &Surface) {
    let Some(dir) = ffi::env_value(c"NS_FRAME_DUMP") else {
        return;
    };
    let frame = FRAME_NO.fetch_add(1, Ordering::Relaxed);
    if frame % FRAME_DUMP_EVERY == 0 {
        let mut path = dir.to_bytes().to_vec();
        path.extend_from_slice(format!("/frame-{frame:05}.png").as_bytes());
        if let Some(path) = ffi::gstr_from(&path) {
            surface.write_png(&path);
        }
    }
}

pub fn render_argb32(
    b: &NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    scale: f64,
    out: &PixelBuf,
) -> c_int {
    if b.layout().is_none() || !target_ok(out) {
        return -1;
    }
    let scale = valid_scale(scale);
    b.set_video_page_coords(false);
    note_viewport(b, scroll_x, scroll_y, out.height, scale);
    let Some(cr) = Cairo::on_buffer(out, scale) else {
        return -1;
    };
    cr.scale(scale);
    cr.translate(-f64::from(scroll_x), -f64::from(scroll_y));
    paint_begin(b);
    let t0 = ffi::monotonic_us();
    if let Some(layout) = b.layout() {
        cr.paint(b, layout);
    }
    b.flush_video_composites(ffi::monotonic_us());
    if ffi::env_set(c"NS_PROFILE") {
        ffi::printerr_paint_profile(
            (ffi::monotonic_us() - t0) as f64 / 1000.0,
            out.width,
            out.height,
        );
    }
    paint_end();
    let surface = cr.finish();
    dump_frame(&surface);
    0
}

pub fn snap_document(
    b: &NsBrowser,
    viewport: (f64, f64),
    prev: (c_int, c_int),
    pos: (c_int, c_int),
) -> Option<(c_int, c_int)> {
    let layout = b.layout()?;
    let doc = b.doc()?;
    let style = [c"html", c"body"].into_iter().find_map(|tag| {
        let style = ffi::find_first_element(doc, tag).map_or(core::ptr::null(), |n| b.style_for(n));
        (!style.is_null() && ffi::snap_type_is_set(style)).then_some(style)
    })?;
    let (page_w, page_h) = page::page_size(b)?;
    let max_x = non_negative(f64::from(page_w) - viewport.0);
    let max_y = non_negative(f64::from(page_h) - viewport.1);
    let mut snapped = (f64::from(pos.0), f64::from(pos.1));
    let prev = (f64::from(prev.0), f64::from(prev.1));
    if !ffi::scroll_snap_viewport(layout, style, viewport, (max_x, max_y), prev, &mut snapped) {
        return None;
    }
    let next = ((snapped.0 + 0.5) as c_int, (snapped.1 + 0.5) as c_int);
    (next != pos).then_some(next)
}

pub fn layers_prepare(
    b: &NsBrowser,
    scroll_x: c_int,
    scroll_y: c_int,
    width: c_int,
    height: c_int,
    scale: f64,
    plan: &Plan<'_>,
) -> c_int {
    let scale = valid_scale(scale);
    b.set_video_page_coords(true);
    note_viewport(b, scroll_x, scroll_y, height, scale);
    let mut row = vec![0u8; width as usize * 4];
    let buf = unsafe { PixelBuf::new(row.as_mut_ptr(), width, 1, width * 4) };
    let Some(cr) = Cairo::on_buffer(&buf, scale) else {
        return -1;
    };
    cr.scale(scale);
    cr.translate(-f64::from(scroll_x), -f64::from(scroll_y));
    paint_begin(b);
    if let Some(layout) = b.layout() {
        cr.plan_layers(layout, plan);
    }
    paint_end();
    drop(cr.finish());
    drop(row);
    if plan.dynamic() { -1 } else { 0 }
}

pub fn render_doc_tile(b: &NsBrowser, plan: &Plan<'_>, target: &TileTarget) -> c_int {
    let Some(layout) = b.layout() else {
        return -1;
    };
    ffi::render_doc_tile(b, layout, plan, target, || paint_begin(b), paint_end)
}

fn viewport_css_h(b: &NsBrowser) -> f64 {
    if b.cur_viewport_h.get() > 0.0 {
        b.cur_viewport_h.get()
    } else {
        b.vh.get()
    }
}

fn fixed_layer_info(layer: BoxRef<'_>, vh: f64, out: &mut VpLayerInfo) -> c_int {
    let exact = ffi::subtree_extent_y(layer, out);
    out.top = if exact {
        let t = out.top - VP_LAYER_PAD;
        if t > 0.0 { t } else { 0.0 }
    } else {
        0.0
    };
    out.bottom = if exact {
        let bottom = out.bottom + VP_LAYER_PAD;
        if bottom < vh { bottom } else { vh }
    } else {
        vh
    };
    if out.bottom <= out.top {
        out.top = 0.0;
        out.bottom = 0.0;
    }
    0
}

fn sticky_layer_info(b: &NsBrowser, layer: BoxRef<'_>, vh: f64, out: &mut VpLayerInfo) -> c_int {
    let exact = ffi::subtree_extent_y(layer, out);
    if !ffi::sticky_y_model(layer, vh, out) {
        return -1;
    }
    let (sx, sy) = (b.cur_scroll_x.get(), b.cur_scroll_y.get());
    let (dx, _) = ffi::sticky_offset(layer, sx, sy, sx + f64::from(b.vw.get()), sy + vh);
    out.x_offset = dx;
    let pad = if exact { VP_LAYER_PAD } else { vh / 2.0 };
    out.top -= pad;
    out.bottom += pad;
    if out.bottom - out.top <= vh * 4.0 {
        0
    } else {
        -1
    }
}

pub fn vp_layer_info(
    b: &NsBrowser,
    plan: Option<&Plan<'_>>,
    index: c_int,
    out: &mut VpLayerInfo,
) -> c_int {
    let Some(cap) = plan.and_then(|p| p.capture(index)) else {
        return -1;
    };
    if b.layout().is_none() {
        return -1;
    }
    out.kind = cap.kind();
    let vh = viewport_css_h(b);
    let Some(layer) = cap.box_ref() else {
        return -1;
    };
    if cap.kind() == ffi::VP_STICKY {
        sticky_layer_info(b, layer, vh, out)
    } else {
        fixed_layer_info(layer, vh, out)
    }
}

pub struct VpRender {
    pub index: c_int,
    pub scroll_x: c_int,
    pub scroll_y: c_int,
    pub origin_y: c_int,
    pub scale: f64,
}

pub fn render_vp_layer(b: &NsBrowser, plan: &Plan<'_>, r: &VpRender, out: &PixelBuf) -> c_int {
    let Some(cap) = plan.capture(r.index) else {
        return -1;
    };
    let Some(layout) = b.layout() else {
        return -1;
    };
    if !target_ok(out) {
        return -1;
    }
    let scale = valid_scale(r.scale);
    out.clear();
    let Some(cr) = Cairo::on_buffer(out, scale) else {
        return -1;
    };
    cr.translate(0.0, -f64::from(r.origin_y));
    cr.scale(scale);
    if cap.kind() == ffi::VP_STICKY {
        cr.translate(-f64::from(r.scroll_x), 0.0);
    } else {
        cr.translate(-f64::from(r.scroll_x), -f64::from(r.scroll_y));
    }
    paint_begin(b);
    cr.vp_layer(b, layout, cap, f64::from(r.scroll_x), f64::from(r.scroll_y));
    paint_end();
    drop(cr.finish());
    0
}

fn under_fixed(b: BoxRef<'_>) -> bool {
    let mut cur = Some(b);
    while let Some(a) = cur {
        if ffi::box_is_fixed(a) {
            return true;
        }
        cur = a.parent();
    }
    false
}

struct ScrollerWalk {
    vx: f64,
    vy: f64,
    out: Vec<u8>,
    left: c_int,
}

fn scroller_rect_emit(b: BoxRef<'_>, ox: f64, oy: f64, w: &mut ScrollerWalk) {
    let fixed = under_fixed(b);
    let (margin, border, padding) = (b.margin(), b.border(), b.padding());
    let mut x = b.x() + margin.left + border.left + ox;
    let mut y = b.y() + margin.top + border.top + oy;
    let bw = b.content_width() + padding.left + padding.right;
    let bh = b.content_height() + padding.top + padding.bottom;
    if fixed {
        x -= w.vx;
        y -= w.vy;
    }
    if !(bw > 0.0 && bh > 0.0 && x.is_finite() && y.is_finite()) {
        return;
    }
    let axes = c_int::from(b.scroll_max_x() > 0.0) | (c_int::from(b.scroll_max_y() > 0.0) << 1);
    w.out.extend_from_slice(
        format!(
            "sr {} {} {} {} {} {}\n",
            x.floor() as c_int,
            y.floor() as c_int,
            bw.ceil() as c_int,
            bh.ceil() as c_int,
            axes,
            c_int::from(fixed)
        )
        .as_bytes(),
    );
    w.left -= 1;
}

fn scroller_rects_walk(b: BoxRef<'_>, ox: f64, oy: f64, w: &mut ScrollerWalk) {
    if w.left <= 0 {
        return;
    }
    let (hx, hy) = ffi::box_hit_offset(b);
    let (ox, oy) = (ox + hx, oy + hy);
    if b.scrolls() && (b.scroll_max_x() > 0.0 || b.scroll_max_y() > 0.0) {
        scroller_rect_emit(b, ox, oy, w);
    }
    let (cx, cy) = (ox - b.scroll_x(), oy - b.scroll_y());
    for c in children(b) {
        scroller_rects_walk(c, cx, cy, w);
    }
}

pub fn scroller_rects(b: &NsBrowser, max_rects: c_int) -> Option<Vec<u8>> {
    let layout = b.layout()?;
    let mut w = ScrollerWalk {
        vx: b.cur_scroll_x.get(),
        vy: b.cur_scroll_y.get(),
        out: Vec::new(),
        left: max_rects,
    };
    scroller_rects_walk(layout, 0.0, 0.0, &mut w);
    if w.left <= 0 {
        w.out.extend_from_slice(b"sr-all\n");
    }
    Some(w.out)
}
