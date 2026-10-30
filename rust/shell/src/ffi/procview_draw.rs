//! Southstar — the tab view's drawing area, page frame and tile snapshot, cursors, scroll adjustments, wheel and fling scrolling, and the render and tick requests that keep the page painted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_cairo::{Antialias, Context, Format, LineCap, LineJoin, Pattern, Surface};
use southstar_glib::{self as glib, GBoolean};
use southstar_gtk::{self as gtk, Cursor, EventControllerScroll, SnapshotRef, Texture, Widget};
use southstar_gtk_ffi::{GdkRgba, GdkTexture, GrapheneRect, GtkSnapshot};

use super::pagelayers;
use super::procview_input::{
    area, cur_scale, hadj, maybe_update_viewport, new_req, page_dpr, push_req, push_req_front,
    raster_scale, request_hover, vadj, view, viewport_h, viewport_w,
};
use super::procview_media::{ns_pv_media_on_page, ns_pv_media_snapshot, ns_pv_media_video_playing};
use super::procview_result::{do_load, ns_proc_view_zoom_in, ns_proc_view_zoom_out};
use super::procview_state::{NsProcView, Res};
use crate::pagelayers::{LayerUpdate, PageLayers};
use crate::procview::draw::{self, Cursor as PageCursor, Glide, PathOp};

const REQ_RENDER: c_int = 1;
const REQ_TICK: c_int = 21;

fn flag(value: bool) -> GBoolean {
    GBoolean::from(value)
}

pub(super) fn layers<'a>(v: *mut NsProcView) -> Option<&'a mut PageLayers> {
    // SAFETY: layers is null or the view's PageLayers, owned by the view and used only on the main loop.
    unsafe { view(v).layers.cast::<PageLayers>().as_mut() }
}

fn frame(v: *mut NsProcView) -> Option<Texture> {
    // SAFETY: frame is null or the live texture the view holds a reference to.
    unsafe { gtk::from_borrowed(view(v).frame) }
}

fn frame_covers(frame: Option<&Texture>, fs: f64, width: f32, height: f32) -> bool {
    frame.is_some_and(|frame| {
        f64::from(frame.width()) / fs >= f64::from(width) - 0.5
            && f64::from(frame.height()) / fs >= f64::from(height) - 0.5
    })
}

unsafe extern "C" fn snapshot_page_video(
    snapshot: *mut GtkSnapshot,
    offset_x: f64,
    offset_y: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    if !ns_pv_media_on_page(v) {
        return;
    }
    // SAFETY: the page layers pass the snapshot they are recording into, live for this call.
    let Some(snapshot) = (unsafe { SnapshotRef::from_raw(snapshot) }) else {
        return;
    };
    let fs = raster_scale(v);
    snapshot.save();
    snapshot.translate((-offset_x / fs) as f32, (-offset_y / fs) as f32);
    ns_pv_media_snapshot(v, fs, snapshot.as_ptr());
    snapshot.restore();
}

fn area_snapshot(v: *mut NsProcView, widget: &Widget, snapshot: SnapshotRef<'_>) {
    let width = widget.width();
    let height = widget.height();
    let state = view(v);
    if state.tiles_mode != 0 {
        pagelayers::ns_page_layers_snapshot(
            layers(v).as_deref(),
            snapshot.as_ptr(),
            f64::from(width),
            f64::from(height),
            raster_scale(v),
            f64::from(state.scroll_x),
            f64::from(state.scroll_y),
            Some(snapshot_page_video),
            v.cast(),
        );
        return;
    }
    let fs = if state.frame_scale > 0.0 {
        state.frame_scale
    } else {
        1.0
    };
    let area = GrapheneRect::new(0.0, 0.0, width as f32, height as f32);
    let frame = frame(v);
    if !frame_covers(frame.as_ref(), fs, area.size.width, area.size.height) {
        let white = GdkRgba {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        };
        snapshot.append_color(&white, &area);
    }
    if !ns_pv_media_on_page(v) {
        snapshot.push_clip(&area);
        ns_pv_media_snapshot(v, fs, snapshot.as_ptr());
        snapshot.pop();
    }
    let Some(frame) = frame else {
        return;
    };
    let bounds = GrapheneRect::new(
        0.0,
        0.0,
        (f64::from(frame.width()) / fs) as f32,
        (f64::from(frame.height()) / fs) as f32,
    );
    snapshot.push_clip(&area);
    snapshot.append_texture(&frame, &bounds);
    snapshot.pop();
}

fn path(cr: &Context, ops: &[PathOp]) {
    let cr = cr.as_ref();
    for op in ops {
        match *op {
            PathOp::Move(x, y) => cr.move_to(x, y),
            PathOp::Line(x, y) => cr.line_to(x, y),
            PathOp::Curve(x1, y1, x2, y2, x3, y3) => cr.curve_to(x1, y1, x2, y2, x3, y3),
            PathOp::Close => cr.close_path(),
        }
    }
}

fn stroke(cr: &Context, width: f64, ops: &[PathOp]) {
    cr.as_ref().set_line_width(width);
    path(cr, ops);
    cr.as_ref().stroke();
}

fn hourglass_texture() -> Option<Texture> {
    const SIZE: c_int = 32;
    let surface = Surface::image(Format::Argb32, SIZE, SIZE)?;
    let cr = Context::new(surface.as_ref());
    let c = cr.as_ref();
    c.set_antialias(Antialias::Best);
    c.set_line_cap(LineCap::Round);
    c.set_line_join(LineJoin::Round);

    c.set_source_rgba(0.0, 0.0, 0.0, 0.22);
    stroke(&cr, 3.0, &draw::hourglass_caps(0.7, 0.9));
    stroke(&cr, 2.6, &draw::hourglass_bulbs(0.7, 0.9));

    let glass = Pattern::linear(16.0, 7.0, 16.0, 26.0);
    glass.add_color_stop_rgba(0.0, 0.97, 1.0, 1.0, 0.55);
    glass.add_color_stop_rgba(0.5, 0.62, 0.80, 0.95, 0.30);
    glass.add_color_stop_rgba(1.0, 0.93, 0.98, 1.0, 0.52);
    c.set_source(&glass);
    path(&cr, &draw::hourglass_bulbs(0.0, 0.0));
    c.fill();
    drop(glass);

    let sand = Pattern::linear(16.0, 7.0, 16.0, 25.0);
    sand.add_color_stop_rgba(0.0, 1.0, 0.85, 0.40, 0.98);
    sand.add_color_stop_rgba(1.0, 0.90, 0.52, 0.14, 0.98);
    c.set_source(&sand);
    let grains = draw::hourglass_sand();
    path(&cr, &grains[..5]);
    c.fill();
    path(&cr, &grains[5..]);
    c.fill();
    stroke(
        &cr,
        1.1,
        &[PathOp::Move(16.0, 15.3), PathOp::Line(16.0, 20.9)],
    );
    drop(sand);

    c.set_source_rgba(1.0, 1.0, 1.0, 0.80);
    stroke(
        &cr,
        0.9,
        &[
            PathOp::Move(12.6, 8.2),
            PathOp::Curve(13.7, 10.4, 14.6, 12.2, 15.3, 13.8),
        ],
    );
    c.move_to(16.7, 18.4);
    c.curve_to(17.6, 20.2, 18.7, 22.2, 19.6, 24.0);
    c.stroke();

    c.set_source_rgba(0.10, 0.12, 0.14, 0.98);
    stroke(&cr, 2.0, &draw::hourglass_caps(0.0, 0.0));
    stroke(&cr, 1.5, &draw::hourglass_bulbs(0.0, 0.0));

    c.set_source_rgba(0.74, 0.82, 0.90, 0.85);
    stroke(&cr, 0.6, &draw::hourglass_bulbs(0.0, 0.0));

    drop(cr);
    surface.as_ref().flush();
    surface
        .as_ref()
        .with_data(|data, stride| Texture::from_bytes(SIZE, SIZE, data, stride))
        .flatten()
}

fn busy_cursor(v: *mut NsProcView) -> Option<Cursor> {
    let state = view(v);
    // SAFETY: hourglass_cursor is null or the cursor the view holds a reference to until it is freed.
    if let Some(cursor) = unsafe { gtk::from_borrowed::<Cursor>(state.hourglass_cursor) } {
        return Some(cursor);
    }
    let cursor = hourglass_texture()
        .and_then(|texture| {
            let fallback = Cursor::from_name(c"wait", None);
            Cursor::from_texture(&texture, 16, 16, fallback.as_ref())
        })
        .or_else(|| Cursor::from_name(c"wait", None))?;
    state.hourglass_cursor = cursor.clone().into_raw();
    Some(cursor)
}

pub(super) fn set_busy_cursor(v: *mut NsProcView) {
    view(v).busy_cursor = 1;
    let Some(area) = area(v) else {
        return;
    };
    match busy_cursor(v) {
        Some(cursor) => area.set_cursor(Some(&cursor)),
        None => area.set_cursor_from_name(Some(c"wait")),
    }
}

pub(super) fn clear_busy_cursor(v: *mut NsProcView) {
    let state = view(v);
    if state.busy_cursor == 0 {
        return;
    }
    state.busy_cursor = 0;
    if let Some(area) = area(v) {
        area.set_cursor_from_name(None);
    }
}

pub(super) fn set_named_cursor(v: *mut NsProcView, name: &[u8]) {
    let Some(widget) = area(v) else {
        return;
    };
    let PageCursor::Named(fallback) = draw::cursor_for(name) else {
        widget.set_cursor(None);
        return;
    };
    let name = CString::new(name).unwrap_or_default();
    let fallback = fallback.and_then(|fallback| {
        let fallback = CString::new(fallback).unwrap_or_default();
        Cursor::from_name(&fallback, None)
    });
    let cursor = Cursor::from_name(&name, fallback.as_ref());
    widget.set_cursor(cursor.as_ref().or(fallback.as_ref()));
}

pub(super) fn stage_fill(pixels: &[u8], w: c_int, h: c_int, stride: c_int) -> *mut GdkTexture {
    let row = w.max(0) as usize * 4;
    if w <= 0 || h <= 0 || (stride as usize) < row {
        return ptr::null_mut();
    }
    let (rows, stride) = (h as usize, stride as usize);
    let needed = (rows - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(row));
    if needed.is_none_or(|n| pixels.len() < n) {
        return ptr::null_mut();
    }
    let texture = if stride == row {
        Texture::from_bytes(w, h, &pixels[..row * rows], row)
    } else {
        let packed: Vec<u8> = pixels
            .chunks(stride)
            .take(rows)
            .flat_map(|line| &line[..row])
            .copied()
            .collect();
        Texture::from_vec(w, h, packed, row)
    };
    texture.map_or(ptr::null_mut(), Texture::into_raw)
}

pub(super) fn configure_adjustments(v: *mut NsProcView) {
    let s = cur_scale(v);
    let cw = f64::from(viewport_w(v)) / s;
    let ch = f64::from(viewport_h(v)) / s;
    let state = view(v);
    let can_scroll_x = f64::from(state.page_w) > cw + 0.5;
    let can_scroll_y = f64::from(state.page_h) > ch + 0.5;
    if !can_scroll_x {
        state.scroll_x = 0;
    }
    if !can_scroll_y {
        state.scroll_y = 0;
    }
    // SAFETY: hscroll is null or the view's live horizontal scrollbar.
    if let Some(hscroll) = unsafe { gtk::from_borrowed::<Widget>(state.hscroll.cast()) } {
        hscroll.set_visible(can_scroll_x);
    }
    // SAFETY: vscroll is null or the view's live vertical scrollbar.
    if let Some(vscroll) = unsafe { gtk::from_borrowed::<Widget>(state.vscroll.cast()) } {
        vscroll.set_visible(can_scroll_y);
    }
    let upper_w = if f64::from(state.page_w) > cw {
        f64::from(state.page_w)
    } else {
        cw
    };
    let upper_h = if f64::from(state.page_h) > ch {
        f64::from(state.page_h)
    } else {
        ch
    };
    let (Some(hadj), Some(vadj)) = (hadj(v), vadj(v)) else {
        return;
    };
    hadj.configure(f64::from(state.scroll_x), 0.0, upper_w, 60.0, cw, cw);
    vadj.configure(f64::from(state.scroll_y), 0.0, upper_h, 60.0, ch, ch);
    let state = view(v);
    state.scroll_x = hadj.value() as c_int;
    state.scroll_y = vadj.value() as c_int;
}

fn on_adj_changed(v: *mut NsProcView) {
    let state = view(v);
    if state.closed != 0 {
        return;
    }
    if let (Some(hadj), Some(vadj)) = (hadj(v), vadj(v)) {
        state.scroll_x = hadj.value() as c_int;
        state.scroll_y = vadj.value() as c_int;
    }
    if state.tiles_mode != 0
        && let Some(area) = area(v)
    {
        area.queue_draw();
    }
    if state.opened != 0 && state.adopting_scroll == 0 {
        request_render(v);
    }
}

fn tiles_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| glib::env_bytes("NS_TILES").is_none_or(|v| v != b"0"))
}

fn tiles_range(v: *mut NsProcView) -> (f64, f64) {
    let vh = f64::from(viewport_h(v)) / cur_scale(v);
    let state = view(v);
    draw::tiles_range(f64::from(state.scroll_y), vh, state.scroll_dir)
}

fn tiles_request(v: *mut NsProcView, req: &mut super::procview_state::Req) {
    if !tiles_enabled() {
        return;
    }
    let (y0, y1) = tiles_range(v);
    let state = view(v);
    let pl = layers(v).map(|pl| &*pl);
    req.tiles.tile_h = draw::TILE_H;
    req.tiles.want_y0 = y0.floor() as c_int;
    req.tiles.want_y1 = y1.ceil() as c_int;
    req.tiles.gen_ = pagelayers::ns_page_layers_gen(pl);
    req.tiles.vp_held = pagelayers::ns_page_layers_vp_held(pl);
    req.tiles.fill = state.tiles_fill;
    state.tiles_fill = 0;
    req.have = pagelayers::ns_page_layers_have(pl, y0, y1, false);
    req.hold = pagelayers::ns_page_layers_have(pl, y0, y1, true);
}

fn adjustments_stale(v: *mut NsProcView) -> bool {
    let s = cur_scale(v);
    let cw = f64::from(viewport_w(v)) / s;
    let ch = f64::from(viewport_h(v)) / s;
    let state = view(v);
    let upper = |adj: Option<gtk::Adjustment>| adj.map_or(0.0, |adj| adj.upper());
    (upper(hadj(v)) - f64::from(state.page_w).max(cw)).abs() > 0.5
        || (upper(vadj(v)) - f64::from(state.page_h).max(ch)).abs() > 0.5
}

pub(super) fn tiles_apply(v: *mut NsProcView, res: &Res) {
    // SAFETY: a layers reply carries null or the LayerUpdate the worker built for this view, handed to the main loop with the reply.
    let update = unsafe { res.layers.cast::<LayerUpdate>().as_mut() };
    let changed = pagelayers::ns_page_layers_apply(layers(v), update);
    if !pagelayers::ns_page_layers_active(layers(v).as_deref()) {
        return;
    }
    let state = view(v);
    if state.tiles_mode == 0 {
        state.tiles_mode = 1;
        if !state.frame.is_null() {
            // SAFETY: frame holds the view's own reference to its texture, released once here and cleared.
            unsafe { gtk::sys::g_object_unref(state.frame) };
            state.frame = ptr::null_mut();
        }
    }
    state.render_restarts = 0;
    clear_busy_cursor(v);
    let vh = f64::from(viewport_h(v)) / cur_scale(v);
    let state = view(v);
    let scroll_y = f64::from(state.scroll_y);
    pagelayers::ns_page_layers_evict(layers(v), scroll_y - vh * 1.5, scroll_y + vh * 2.5);
    if res.ph > 0 {
        state.page_h = res.ph;
        if res.pw > 0 {
            state.page_w = res.pw;
        }
    }
    if adjustments_stale(v) {
        state.adopting_scroll = 1;
        configure_adjustments(v);
        view(v).adopting_scroll = 0;
    }
    let (y0, y1) = tiles_range(v);
    if changed {
        if let Some(area) = area(v) {
            area.queue_draw();
        }
        let state = view(v);
        let scroll_y = f64::from(state.scroll_y);
        let missing = pagelayers::ns_page_layers_missing(
            layers(v).as_deref(),
            y0,
            y1,
            scroll_y,
            scroll_y + vh,
            f64::from(state.page_h),
        );
        if missing && state.render_pending == 0 {
            state.render_pending = 1;
            state.tiles_fill = 1;
        }
    }
}

pub(super) fn tiles_leave(v: *mut NsProcView) {
    let state = view(v);
    if state.tiles_mode == 0 {
        return;
    }
    state.tiles_mode = 0;
    pagelayers::ns_page_layers_reset(layers(v));
}

pub(super) fn request_tick(v: *mut NsProcView) {
    let state = view(v);
    if state.opened == 0 {
        return;
    }
    if state.tick_inflight != 0 {
        state.tick_pending = 1;
        return;
    }
    state.tick_inflight = 1;
    let seq = state.load_seq;
    push_req_front(v, new_req(REQ_TICK, |r| r.seq = seq));
}

pub(super) fn request_render(v: *mut NsProcView) {
    let state = view(v);
    if state.opened == 0 {
        return;
    }
    state.tiles_fill = 0;
    if state.render_inflight != 0 {
        state.render_pending = 1;
        return;
    }
    start_render(v);
}

pub(super) fn start_render(v: *mut NsProcView) {
    let state = view(v);
    if state.opened == 0 {
        return;
    }
    state.render_inflight = 1;
    state.render_seq += 1;
    let raster = raster_scale(v);
    let req = new_req(REQ_RENDER, |r| {
        let state = view(v);
        r.seq = state.render_seq;
        r.raster = raster;
        r.w = (f64::from(viewport_w(v)) * raster).ceil() as c_int;
        r.h = (f64::from(viewport_h(v)) * raster).ceil() as c_int;
        r.sx = state.scroll_x;
        r.sy = state.scroll_y;
        r.scale = cur_scale(v) * raster;
        r.dpr = page_dpr(v);
        r.caret_active = flag(area(v).is_some_and(|area| area.has_focus()));
        tiles_request(v, r);
        let state = view(v);
        r.wheel.dx = state.wheel_pend_x as c_int;
        r.wheel.dy = state.wheel_pend_y as c_int;
        if r.wheel.dx != 0 || r.wheel.dy != 0 {
            let s = cur_scale(v);
            state.wheel_pend_x -= f64::from(r.wheel.dx);
            state.wheel_pend_y -= f64::from(r.wheel.dy);
            r.wheel.x = state.scroll_x + (state.pointer_x / s) as c_int;
            r.wheel.y = state.scroll_y + (state.pointer_y / s) as c_int;
            r.wheel.viewport = state.wheel_viewport != 0;
        }
    });
    push_req(v, req);
}

fn hover_after_scroll(v: *mut NsProcView) {
    let state = view(v);
    state.hover_after_scroll_id = 0;
    if state.closed != 0 || state.opened == 0 {
        return;
    }
    if area(v).is_some_and(|area| area.contains(state.pointer_x, state.pointer_y)) {
        let s = cur_scale(v);
        request_hover(
            v,
            state.scroll_x + (state.pointer_x / s) as c_int,
            state.scroll_y + (state.pointer_y / s) as c_int,
        );
    }
}

fn route_wheel(v: *mut NsProcView, dx: f64, dy: f64) {
    let s = cur_scale(v);
    let state = view(v);
    state.wheel_remote = flag(
        state.wheel_viewport == 0
            && pagelayers::ns_page_layers_scroller_at(
                layers(v).as_deref(),
                state.pointer_x / s,
                state.pointer_y / s,
                f64::from(state.scroll_x),
                f64::from(state.scroll_y),
                dy.abs() >= dx.abs(),
            ),
    );
}

fn scroll_local(v: *mut NsProcView) {
    let state = view(v);
    let dx = draw::take_whole(&mut state.wheel_pend_x);
    let dy = draw::take_whole(&mut state.wheel_pend_y);
    if dx == 0 && dy == 0 {
        return;
    }
    if dy != 0 {
        state.scroll_dir = if dy > 0 { 1 } else { -1 };
    }
    let want_x = state.scroll_x + dx;
    let want_y = state.scroll_y + dy;
    if let Some(hadj) = hadj(v) {
        hadj.set_value(f64::from(want_x));
    }
    if let Some(vadj) = vadj(v) {
        vadj.set_value(f64::from(want_y));
    }
    let state = view(v);
    if state.scroll_x != want_x {
        state.wheel_left_x = 0.0;
        state.fling_vx = 0.0;
        state.wheel_pend_x = 0.0;
    }
    if state.scroll_y != want_y {
        state.wheel_left_y = 0.0;
        state.fling_vy = 0.0;
        state.wheel_pend_y = 0.0;
    }
}

fn queue_wheel_scroll(v: *mut NsProcView, dx: f64, dy: f64) {
    let state = view(v);
    if state.opened == 0 {
        return;
    }
    if state.hover_after_scroll_id != 0 {
        // SAFETY: the id is the view's pending hover timeout, still attached; GLib only warns on a stale one.
        unsafe { gtk::sys::g_source_remove(state.hover_after_scroll_id) };
    } else {
        route_wheel(v, dx, dy);
    }
    let state = view(v);
    state.hover_after_scroll_id =
        glib::timeout_add_local_once(draw::HOVER_AFTER_SCROLL_MS, move || hover_after_scroll(v))
            .raw();
    state.wheel_pend_x += dx;
    state.wheel_pend_y += dy;
    if state.tiles_mode != 0 && state.wheel_remote == 0 {
        scroll_local(v);
    } else if state.wheel_pend_x.abs() >= 1.0 || state.wheel_pend_y.abs() >= 1.0 {
        request_render(v);
    }
}

fn glide(state: &NsProcView) -> Glide {
    Glide {
        left_x: state.wheel_left_x,
        left_y: state.wheel_left_y,
        fling_x: state.fling_vx,
        fling_y: state.fling_vy,
    }
}

fn wheel_scrolling(v: *mut NsProcView) -> bool {
    let state = view(v);
    state.hover_after_scroll_id != 0
        || !glide(state).idle()
        || state.wheel_pend_x != 0.0
        || state.wheel_pend_y != 0.0
}

fn wheel_tick(v: *mut NsProcView, now: i64) -> bool {
    let state = view(v);
    if state.closed != 0 || state.opened == 0 || glide(state).idle() {
        state.wheel_tick_id = 0;
        state.wheel_last_us = 0;
        return false;
    }
    let dt_ms = draw::frame_interval_ms(now, state.wheel_last_us);
    state.wheel_last_us = now;
    let mut motion = glide(state);
    let (mx, my) = motion.step(dt_ms);
    state.wheel_left_x = motion.left_x;
    state.wheel_left_y = motion.left_y;
    state.fling_vx = motion.fling_x;
    state.fling_vy = motion.fling_y;
    queue_wheel_scroll(v, mx, my);
    true
}

fn arm_wheel_animation(v: *mut NsProcView) {
    let state = view(v);
    if state.wheel_tick_id != 0 || glide(state).idle() {
        return;
    }
    let Some(area) = area(v) else {
        return;
    };
    state.wheel_last_us = 0;
    state.wheel_tick_id = area.add_tick_callback(move |_, now| wheel_tick(v, now));
}

pub(super) fn scroll_target_x(v: *const NsProcView) -> f64 {
    let state = view(v.cast_mut());
    f64::from(state.scroll_x) + state.wheel_pend_x + state.wheel_left_x
}

pub(super) fn scroll_target_y(v: *const NsProcView) -> f64 {
    let state = view(v.cast_mut());
    f64::from(state.scroll_y) + state.wheel_pend_y + state.wheel_left_y
}

pub(super) fn scroll_view_to(v: *mut NsProcView, x: f64, y: f64) {
    let max = |adj: Option<gtk::Adjustment>| adj.map_or(0.0, |adj| adj.upper() - adj.page_size());
    let max_x = max(hadj(v));
    let max_y = max(vadj(v));
    let x = x.clamp(0.0, max_x.max(0.0));
    let y = y.clamp(0.0, max_y.max(0.0));
    let state = view(v);
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
    state.wheel_viewport = 1;
    state.wheel_left_x += x - scroll_target_x(v);
    state.wheel_left_y += y - scroll_target_y(v);
    arm_wheel_animation(v);
}

fn on_device_scale_changed(v: *mut NsProcView) {
    let state = view(v);
    if state.closed != 0 || state.opened == 0 {
        return;
    }
    if !maybe_update_viewport(v) {
        request_render(v);
    }
}

fn disconnect_scale_surface(v: *mut NsProcView) {
    let state = view(v);
    if state.surface_scale_handler == 0 {
        return;
    }
    // SAFETY: scale_surface is null or the surface the area was realized on, alive until the area unrealizes.
    if let Some(surface) = unsafe { gtk::from_borrowed::<gtk::Surface>(state.scale_surface) } {
        gtk::SignalHandlerId::from_raw(state.surface_scale_handler).disconnect(surface.object());
    }
}

fn on_area_realize(v: *mut NsProcView, area: &Widget) {
    set_im_client(v, Some(area));
    let Some(surface) = area.surface() else {
        return;
    };
    let state = view(v);
    if surface.as_ptr() == state.scale_surface {
        return;
    }
    disconnect_scale_surface(v);
    let state = view(v);
    state.scale_surface = surface.as_ptr();
    state.surface_scale_handler = surface
        .object()
        .connect_notify(c"scale", move || on_device_scale_changed(v))
        .raw();
}

fn set_im_client(v: *mut NsProcView, area: Option<&Widget>) {
    // SAFETY: im is null or the view's live GtkIMContext, owned until the view is freed.
    if let Some(im) = unsafe { gtk::from_borrowed::<gtk::IMContext>(view(v).im) } {
        im.set_client_widget(area);
    }
}

fn on_area_unrealize(v: *mut NsProcView) {
    set_im_client(v, None);
    disconnect_scale_surface(v);
    let state = view(v);
    state.scale_surface = ptr::null_mut();
    state.surface_scale_handler = 0;
}

fn on_area_focus_notify(v: *mut NsProcView) {
    view(v).last_anim_frame_us = 0;
    request_render(v);
}

fn anim_tick(v: *mut NsProcView, frame_time: i64) -> bool {
    let state = view(v);
    if state.closed != 0 || state.opened == 0 {
        state.anim_tick_id = 0;
        state.last_anim_frame_us = 0;
        return false;
    }
    if ns_pv_media_video_playing(v) {
        return true;
    }
    let now = if frame_time > 0 {
        frame_time
    } else {
        glib::monotonic_us()
    };
    let state = view(v);
    let interval = if state.page_animating != 0 {
        draw::ANIM_FRAME_US
    } else {
        draw::CARET_BLINK_US
    };
    if state.last_anim_frame_us > 0 && now - state.last_anim_frame_us < interval {
        return true;
    }
    state.last_anim_frame_us = now;
    if state.page_animating != 0 && !wheel_scrolling(v) {
        request_tick(v);
    } else {
        request_render(v);
    }
    true
}

pub(super) fn arm_anim(v: *mut NsProcView) {
    let state = view(v);
    if state.anim_tick_id != 0 || state.closed != 0 {
        return;
    }
    let Some(area) = area(v) else {
        return;
    };
    state.last_anim_frame_us = 0;
    state.anim_tick_id = area.add_tick_callback(move |_, now| anim_tick(v, now));
}

pub(super) fn disarm_anim(v: *mut NsProcView) {
    let state = view(v);
    if state.anim_tick_id != 0
        && let Some(area) = area(v)
    {
        area.remove_tick_callback(state.anim_tick_id);
    }
    state.anim_tick_id = 0;
    state.last_anim_frame_us = 0;
}

fn on_resize(v: *mut NsProcView, width: c_int, height: c_int) {
    let state = view(v);
    if !state.deferred_url.is_null() && width > 1 && height > 1 {
        let url = state.deferred_url;
        let record = state.deferred_record;
        let history = state.deferred_history;
        let activated = state.deferred_user_activated;
        state.deferred_url = ptr::null_mut();
        do_load(v, url, record != 0, history != 0, activated != 0);
        // SAFETY: url was the view's g_strdup'd deferred URL, detached above and freed once here.
        unsafe { glib::g_free(url.cast()) };
        return;
    }
    if state.opened != 0 {
        if maybe_update_viewport(v) {
            return;
        }
        configure_adjustments(v);
        request_render(v);
    }
}

fn on_scroll(v: *mut NsProcView, ctrl: &EventControllerScroll, dx: f64, dy: f64) -> bool {
    let state = view(v);
    if state.opened == 0 {
        return false;
    }
    let mut mods = ctrl.current_event_state();
    if let Some(event_mods) = ctrl.current_event_modifier_state() {
        mods |= event_mods;
    }
    if mods & gtk::sys::CONTROL_MASK != 0 {
        let delta = if dy != 0.0 { dy } else { dx };
        if delta < 0.0 {
            ns_proc_view_zoom_in(v);
        } else if delta > 0.0 {
            ns_proc_view_zoom_out(v);
        }
        return true;
    }
    let s = cur_scale(v);
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
    state.wheel_viewport = 0;
    if ctrl.unit() == gtk::sys::SCROLL_UNIT_SURFACE {
        queue_wheel_scroll(v, dx / s, dy / s);
        return true;
    }
    state.wheel_left_x += dx * draw::WHEEL_STEP_PX / s;
    state.wheel_left_y += dy * draw::WHEEL_STEP_PX / s;
    arm_wheel_animation(v);
    true
}

fn on_scroll_decelerate(v: *mut NsProcView, ctrl: &EventControllerScroll, vel_x: f64, vel_y: f64) {
    let state = view(v);
    if state.opened == 0 || ctrl.current_event_state() & gtk::sys::CONTROL_MASK != 0 {
        return;
    }
    let s = cur_scale(v);
    state.fling_vx = vel_x / s;
    state.fling_vy = vel_y / s;
    state.wheel_viewport = 0;
    arm_wheel_animation(v);
}

pub(super) fn ns_pv_area_new(v: *mut NsProcView) -> Widget {
    let widget = Widget::new_snapshot_area();
    widget.set_snapshot_func(move |widget, snapshot| area_snapshot(v, widget, snapshot));
    widget
}

pub(super) fn ns_pv_area_detach(v: *mut NsProcView) {
    if let Some(area) = area(v) {
        area.clear_snapshot_func();
    }
}

pub(super) fn ns_pv_install_view_signals(v: *mut NsProcView) {
    if let Some(hadj) = hadj(v) {
        hadj.connect_value_changed(move || on_adj_changed(v));
    }
    if let Some(vadj) = vadj(v) {
        vadj.connect_value_changed(move || on_adj_changed(v));
    }
}

pub(super) fn ns_pv_install_area_signals(v: *mut NsProcView) {
    let Some(area) = area(v) else {
        return;
    };
    area.connect_resize(move |_, width, height| on_resize(v, width, height));
    area.object()
        .connect_notify(c"has-focus", move || on_area_focus_notify(v));
    area.object()
        .connect_notify(c"scale-factor", move || on_device_scale_changed(v));
    let weak = area.downgrade();
    area.connect_realize(move || {
        if let Some(area) = weak.upgrade() {
            on_area_realize(v, &area);
        }
    });
    area.connect_unrealize(move || on_area_unrealize(v));
}

pub(super) fn ns_pv_install_scroll(v: *mut NsProcView) {
    let Some(area) = area(v) else {
        return;
    };
    let scroll = EventControllerScroll::new(
        gtk::sys::EVENT_CONTROLLER_SCROLL_BOTH_AXES | gtk::sys::EVENT_CONTROLLER_SCROLL_KINETIC,
    );
    scroll.connect_scroll(move |ctrl, dx, dy| on_scroll(v, ctrl, dx, dy));
    scroll
        .connect_decelerate(move |ctrl, vel_x, vel_y| on_scroll_decelerate(v, ctrl, vel_x, vel_y));
    area.add_controller(scroll);
}

pub(super) fn ns_pv_stop_wheel_animation(v: *mut NsProcView) {
    let state = view(v);
    state.wheel_left_x = 0.0;
    state.wheel_left_y = 0.0;
    state.wheel_pend_x = 0.0;
    state.wheel_pend_y = 0.0;
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
}
