//! Southstar — the tab view's drawing area, page frame and tile snapshot, cursors, scroll adjustments, wheel and fling scrolling, and the render and tick requests that keep the page painted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk_ffi::{
    self as gtk, Cairo, GdkCursor, GdkFrameClock, GdkRgba, GdkTexture, GraphenePoint, GrapheneRect,
    GtkEventController, GtkSnapshot, GtkWidget,
};

use super::pagelayers;
use super::procview_input::{
    cur_scale, maybe_update_viewport, page_dpr, push_req, raster_scale, request_hover, viewport_h,
    viewport_w,
};
use super::procview_media::{ns_pv_media_on_page, ns_pv_media_snapshot, ns_pv_media_video_playing};
use super::procview_result::{do_load, ns_proc_view_zoom_in, ns_proc_view_zoom_out};
use super::procview_state::{NsProcView, Res};
use crate::pagelayers::{LayerUpdate, PageLayers};
use crate::procview::draw::{self, Cursor, Glide, PathOp};

const REQ_RENDER: c_int = 1;
const REQ_TICK: c_int = 21;
const SOURCE_REMOVE: GBoolean = 0;
const SOURCE_CONTINUE: GBoolean = 1;

unsafe extern "C" {
    fn g_async_queue_push_front(queue: *mut c_void, data: *mut c_void);
    fn g_malloc(n_bytes: usize) -> *mut c_void;
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_timeout_add(
        interval: c_uint,
        func: unsafe extern "C" fn(*mut c_void) -> GBoolean,
        data: *mut c_void,
    ) -> c_uint;
    fn g_source_remove(id: c_uint) -> GBoolean;
    fn g_get_monotonic_time() -> i64;
}

fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    unsafe { &mut *v }
}

fn flag(value: bool) -> GBoolean {
    GBoolean::from(value)
}

fn connect(
    instance: *mut c_void,
    signal: &CStr,
    handler: *const (),
    data: *mut c_void,
) -> core::ffi::c_ulong {
    unsafe {
        gtk::g_signal_connect_data(
            instance,
            signal.as_ptr(),
            core::mem::transmute::<*const (), unsafe extern "C" fn()>(handler),
            data,
            None,
            0,
        )
    }
}

pub(super) fn layers<'a>(v: *mut NsProcView) -> Option<&'a mut PageLayers> {
    unsafe { view(v).layers.cast::<PageLayers>().as_mut() }
}

struct AreaType {
    type_id: gtk::GType,
    view_offset: usize,
}

fn area_type() -> &'static AreaType {
    static TYPE: OnceLock<AreaType> = OnceLock::new();
    TYPE.get_or_init(|| unsafe {
        let mut query = gtk::GTypeQuery {
            type_: 0,
            type_name: ptr::null(),
            class_size: 0,
            instance_size: 0,
        };
        gtk::g_type_query(gtk::gtk_drawing_area_get_type(), &mut query);
        let align = core::mem::align_of::<*mut c_void>();
        let view_offset = (query.instance_size as usize).div_ceil(align) * align;
        let type_id = gtk::g_type_register_static_simple(
            gtk::gtk_drawing_area_get_type(),
            c"NsProcViewArea".as_ptr(),
            query.class_size,
            Some(area_class_init),
            (view_offset + core::mem::size_of::<*mut c_void>()) as c_uint,
            None,
            0,
        );
        AreaType {
            type_id,
            view_offset,
        }
    })
}

unsafe extern "C" fn area_class_init(klass: *mut c_void, _data: *mut c_void) {
    unsafe { (*klass.cast::<gtk::GtkWidgetClass>()).snapshot = Some(area_snapshot) };
}

fn area_view_slot(widget: *mut GtkWidget) -> *mut *mut NsProcView {
    unsafe { widget.cast::<u8>().add(area_type().view_offset).cast() }
}

fn frame_covers(v: *mut NsProcView, fs: f64, width: f32, height: f32) -> bool {
    let frame = view(v).frame.cast::<GdkTexture>();
    !frame.is_null()
        && f64::from(unsafe { gtk::gdk_texture_get_width(frame) }) / fs >= f64::from(width) - 0.5
        && f64::from(unsafe { gtk::gdk_texture_get_height(frame) }) / fs >= f64::from(height) - 0.5
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
    let fs = raster_scale(v);
    unsafe {
        gtk::gtk_snapshot_save(snapshot);
        let point = GraphenePoint {
            x: (-offset_x / fs) as f32,
            y: (-offset_y / fs) as f32,
        };
        gtk::gtk_snapshot_translate(snapshot, &point);
        ns_pv_media_snapshot(v, fs, snapshot);
        gtk::gtk_snapshot_restore(snapshot);
    }
}

unsafe extern "C" fn area_snapshot(widget: *mut GtkWidget, snapshot: *mut GtkSnapshot) {
    let v = unsafe { *area_view_slot(widget) };
    if v.is_null() {
        return;
    }
    let width = unsafe { gtk::gtk_widget_get_width(widget) };
    let height = unsafe { gtk::gtk_widget_get_height(widget) };
    let state = view(v);
    if state.tiles_mode != 0 {
        pagelayers::ns_page_layers_snapshot(
            layers(v).as_deref(),
            snapshot,
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
    if !frame_covers(v, fs, area.size.width, area.size.height) {
        let white = GdkRgba {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        };
        unsafe { gtk::gtk_snapshot_append_color(snapshot, &white, &area) };
    }
    if !ns_pv_media_on_page(v) {
        ns_pv_media_snapshot(v, fs, snapshot);
    }
    let frame = state.frame.cast::<GdkTexture>();
    if frame.is_null() {
        return;
    }
    let bounds = GrapheneRect::new(
        0.0,
        0.0,
        (f64::from(unsafe { gtk::gdk_texture_get_width(frame) }) / fs) as f32,
        (f64::from(unsafe { gtk::gdk_texture_get_height(frame) }) / fs) as f32,
    );
    unsafe {
        gtk::gtk_snapshot_push_clip(snapshot, &area);
        gtk::gtk_snapshot_append_texture(snapshot, frame, &bounds);
        gtk::gtk_snapshot_pop(snapshot);
    }
}

fn path(cr: *mut Cairo, ops: &[PathOp]) {
    for op in ops {
        unsafe {
            match *op {
                PathOp::Move(x, y) => gtk::cairo_move_to(cr, x, y),
                PathOp::Line(x, y) => gtk::cairo_line_to(cr, x, y),
                PathOp::Curve(x1, y1, x2, y2, x3, y3) => {
                    gtk::cairo_curve_to(cr, x1, y1, x2, y2, x3, y3)
                }
                PathOp::Close => gtk::cairo_close_path(cr),
            }
        }
    }
}

fn stroke(cr: *mut Cairo, width: f64, ops: &[PathOp]) {
    unsafe { gtk::cairo_set_line_width(cr, width) };
    path(cr, ops);
    unsafe { gtk::cairo_stroke(cr) };
}

fn hourglass_texture() -> *mut GdkTexture {
    const SIZE: c_int = 32;
    unsafe {
        let surface = gtk::cairo_image_surface_create(gtk::CAIRO_FORMAT_ARGB32, SIZE, SIZE);
        if gtk::cairo_surface_status(surface) != gtk::CAIRO_STATUS_SUCCESS {
            gtk::cairo_surface_destroy(surface);
            return ptr::null_mut();
        }
        let cr = gtk::cairo_create(surface);
        gtk::cairo_set_antialias(cr, gtk::CAIRO_ANTIALIAS_BEST);
        gtk::cairo_set_line_cap(cr, gtk::CAIRO_LINE_CAP_ROUND);
        gtk::cairo_set_line_join(cr, gtk::CAIRO_LINE_JOIN_ROUND);

        gtk::cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 0.22);
        stroke(cr, 3.0, &draw::hourglass_caps(0.7, 0.9));
        stroke(cr, 2.6, &draw::hourglass_bulbs(0.7, 0.9));

        let glass = gtk::cairo_pattern_create_linear(16.0, 7.0, 16.0, 26.0);
        gtk::cairo_pattern_add_color_stop_rgba(glass, 0.0, 0.97, 1.0, 1.0, 0.55);
        gtk::cairo_pattern_add_color_stop_rgba(glass, 0.5, 0.62, 0.80, 0.95, 0.30);
        gtk::cairo_pattern_add_color_stop_rgba(glass, 1.0, 0.93, 0.98, 1.0, 0.52);
        gtk::cairo_set_source(cr, glass);
        path(cr, &draw::hourglass_bulbs(0.0, 0.0));
        gtk::cairo_fill(cr);
        gtk::cairo_pattern_destroy(glass);

        let sand = gtk::cairo_pattern_create_linear(16.0, 7.0, 16.0, 25.0);
        gtk::cairo_pattern_add_color_stop_rgba(sand, 0.0, 1.0, 0.85, 0.40, 0.98);
        gtk::cairo_pattern_add_color_stop_rgba(sand, 1.0, 0.90, 0.52, 0.14, 0.98);
        gtk::cairo_set_source(cr, sand);
        let grains = draw::hourglass_sand();
        path(cr, &grains[..5]);
        gtk::cairo_fill(cr);
        path(cr, &grains[5..]);
        gtk::cairo_fill(cr);
        stroke(
            cr,
            1.1,
            &[PathOp::Move(16.0, 15.3), PathOp::Line(16.0, 20.9)],
        );
        gtk::cairo_pattern_destroy(sand);

        gtk::cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 0.80);
        stroke(
            cr,
            0.9,
            &[
                PathOp::Move(12.6, 8.2),
                PathOp::Curve(13.7, 10.4, 14.6, 12.2, 15.3, 13.8),
            ],
        );
        gtk::cairo_move_to(cr, 16.7, 18.4);
        gtk::cairo_curve_to(cr, 17.6, 20.2, 18.7, 22.2, 19.6, 24.0);
        gtk::cairo_stroke(cr);

        gtk::cairo_set_source_rgba(cr, 0.10, 0.12, 0.14, 0.98);
        stroke(cr, 2.0, &draw::hourglass_caps(0.0, 0.0));
        stroke(cr, 1.5, &draw::hourglass_bulbs(0.0, 0.0));

        gtk::cairo_set_source_rgba(cr, 0.74, 0.82, 0.90, 0.85);
        stroke(cr, 0.6, &draw::hourglass_bulbs(0.0, 0.0));

        gtk::cairo_destroy(cr);
        gtk::cairo_surface_flush(surface);

        let stride = gtk::cairo_image_surface_get_stride(surface) as usize;
        let data = gtk::cairo_image_surface_get_data(surface);
        let bytes = gtk::g_bytes_new(data.cast(), stride * SIZE as usize);
        let texture = gtk::gdk_memory_texture_new(SIZE, SIZE, gtk::MEMORY_DEFAULT, bytes, stride);
        gtk::g_bytes_unref(bytes);
        gtk::cairo_surface_destroy(surface);
        texture
    }
}

fn busy_cursor(v: *mut NsProcView) -> *mut GdkCursor {
    let state = view(v);
    if !state.hourglass_cursor.is_null() {
        return state.hourglass_cursor.cast();
    }
    let texture = hourglass_texture();
    if !texture.is_null() {
        unsafe {
            let fallback = gtk::gdk_cursor_new_from_name(c"wait".as_ptr(), ptr::null_mut());
            state.hourglass_cursor =
                gtk::gdk_cursor_new_from_texture(texture, 16, 16, fallback).cast();
            if !fallback.is_null() {
                gtk::g_object_unref(fallback.cast());
            }
            gtk::g_object_unref(texture.cast());
        }
    }
    if state.hourglass_cursor.is_null() {
        state.hourglass_cursor =
            unsafe { gtk::gdk_cursor_new_from_name(c"wait".as_ptr(), ptr::null_mut()) }.cast();
    }
    state.hourglass_cursor.cast()
}

pub(super) fn set_busy_cursor(v: *mut NsProcView) {
    let state = view(v);
    state.busy_cursor = 1;
    if state.area.is_null() {
        return;
    }
    let cursor = busy_cursor(v);
    unsafe {
        if cursor.is_null() {
            gtk::gtk_widget_set_cursor_from_name(state.area, c"wait".as_ptr());
        } else {
            gtk::gtk_widget_set_cursor(state.area, cursor);
        }
    }
}

pub(super) fn clear_busy_cursor(v: *mut NsProcView) {
    let state = view(v);
    if state.busy_cursor == 0 {
        return;
    }
    state.busy_cursor = 0;
    if !state.area.is_null() {
        unsafe { gtk::gtk_widget_set_cursor_from_name(state.area, ptr::null()) };
    }
}

pub(super) fn set_named_cursor(widget: *mut GtkWidget, name: &[u8]) {
    let Cursor::Named(fallback) = draw::cursor_for(name) else {
        unsafe { gtk::gtk_widget_set_cursor(widget, ptr::null_mut()) };
        return;
    };
    let name = std::ffi::CString::new(name).unwrap_or_default();
    unsafe {
        let fallback_cursor = match fallback {
            Some(fallback) => {
                let fallback = std::ffi::CString::new(fallback).unwrap_or_default();
                gtk::gdk_cursor_new_from_name(fallback.as_ptr(), ptr::null_mut())
            }
            None => ptr::null_mut(),
        };
        let cursor = gtk::gdk_cursor_new_from_name(name.as_ptr(), fallback_cursor);
        gtk::gtk_widget_set_cursor(
            widget,
            if cursor.is_null() {
                fallback_cursor
            } else {
                cursor
            },
        );
        if !cursor.is_null() {
            gtk::g_object_unref(cursor.cast());
        }
        if !fallback_cursor.is_null() {
            gtk::g_object_unref(fallback_cursor.cast());
        }
    }
}

pub(super) fn stage_fill(px: *const u8, w: c_int, h: c_int, stride: c_int) -> *mut GdkTexture {
    let row = w as usize * 4;
    if w <= 0 || h <= 0 || (stride as usize) < row {
        return ptr::null_mut();
    }
    unsafe {
        let bytes = if stride as usize == row {
            gtk::g_bytes_new(px.cast(), row * h as usize)
        } else {
            let dst = g_malloc(row * h as usize).cast::<u8>();
            for y in 0..h as usize {
                ptr::copy_nonoverlapping(px.add(y * stride as usize), dst.add(y * row), row);
            }
            gtk::g_bytes_new_take(dst.cast(), row * h as usize)
        };
        let texture = gtk::gdk_memory_texture_new(w, h, gtk::MEMORY_DEFAULT, bytes, row);
        gtk::g_bytes_unref(bytes);
        texture
    }
}

fn adjustment(raw: *mut c_void) -> *mut gtk::GtkAdjustment {
    raw.cast()
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
    unsafe {
        if !state.hscroll.is_null() {
            gtk::gtk_widget_set_visible(state.hscroll, flag(can_scroll_x));
        }
        if !state.vscroll.is_null() {
            gtk::gtk_widget_set_visible(state.vscroll, flag(can_scroll_y));
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
        gtk::gtk_adjustment_configure(
            adjustment(state.hadj),
            f64::from(state.scroll_x),
            0.0,
            upper_w,
            60.0,
            cw,
            cw,
        );
        gtk::gtk_adjustment_configure(
            adjustment(state.vadj),
            f64::from(state.scroll_y),
            0.0,
            upper_h,
            60.0,
            ch,
            ch,
        );
        state.scroll_x = gtk::gtk_adjustment_get_value(adjustment(state.hadj)) as c_int;
        state.scroll_y = gtk::gtk_adjustment_get_value(adjustment(state.vadj)) as c_int;
    }
}

unsafe extern "C" fn on_adj_changed(_adj: *mut c_void, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.closed != 0 {
        return;
    }
    unsafe {
        state.scroll_x = gtk::gtk_adjustment_get_value(adjustment(state.hadj)) as c_int;
        state.scroll_y = gtk::gtk_adjustment_get_value(adjustment(state.vadj)) as c_int;
        if state.tiles_mode != 0 {
            gtk::gtk_widget_queue_draw(state.area);
        }
    }
    if state.opened != 0 && state.adopting_scroll == 0 {
        request_render(v);
    }
}

fn tiles_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        unsafe { glib::bytes(glib::g_getenv(c"NS_TILES".as_ptr())) }.is_none_or(|v| v != b"0")
    })
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
    unsafe {
        (gtk::gtk_adjustment_get_upper(adjustment(state.hadj)) - f64::from(state.page_w).max(cw))
            .abs()
            > 0.5
            || (gtk::gtk_adjustment_get_upper(adjustment(state.vadj))
                - f64::from(state.page_h).max(ch))
            .abs()
                > 0.5
    }
}

pub(super) fn tiles_apply(v: *mut NsProcView, res: &Res) {
    let update = unsafe { res.layers.cast::<LayerUpdate>().as_mut() };
    let changed = pagelayers::ns_page_layers_apply(layers(v), update);
    if !pagelayers::ns_page_layers_active(layers(v).as_deref()) {
        return;
    }
    let state = view(v);
    if state.tiles_mode == 0 {
        state.tiles_mode = 1;
        if !state.frame.is_null() {
            unsafe { gtk::g_object_unref(state.frame) };
            state.frame = ptr::null_mut();
        }
    }
    state.render_restarts = 0;
    clear_busy_cursor(v);
    let vh = f64::from(viewport_h(v)) / cur_scale(v);
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
        state.adopting_scroll = 0;
    }
    let (y0, y1) = tiles_range(v);
    if changed {
        unsafe { gtk::gtk_widget_queue_draw(state.area) };
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

fn new_req(kind: c_int) -> *mut super::procview_state::Req {
    let req = unsafe { g_malloc0(core::mem::size_of::<super::procview_state::Req>()) }
        .cast::<super::procview_state::Req>();
    unsafe { (*req).r#type = kind };
    req
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
    let req = new_req(REQ_TICK);
    unsafe {
        (*req).seq = state.load_seq;
        g_async_queue_push_front(state.queue, req.cast());
    }
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
    let req = new_req(REQ_RENDER);
    let r = unsafe { &mut *req };
    r.seq = state.render_seq;
    r.raster = raster;
    r.w = (f64::from(viewport_w(v)) * raster).ceil() as c_int;
    r.h = (f64::from(viewport_h(v)) * raster).ceil() as c_int;
    r.sx = state.scroll_x;
    r.sy = state.scroll_y;
    r.scale = cur_scale(v) * raster;
    r.dpr = page_dpr(v);
    r.caret_active = unsafe { gtk::gtk_widget_has_focus(state.area) };
    tiles_request(v, r);
    r.wheel.dx = state.wheel_pend_x as c_int;
    r.wheel.dy = state.wheel_pend_y as c_int;
    if r.wheel.dx != 0 || r.wheel.dy != 0 {
        let s = cur_scale(v);
        state.wheel_pend_x -= f64::from(r.wheel.dx);
        state.wheel_pend_y -= f64::from(r.wheel.dy);
        r.wheel.x = state.scroll_x + (state.pointer_x / s) as c_int;
        r.wheel.y = state.scroll_y + (state.pointer_y / s) as c_int;
        r.wheel.viewport = state.wheel_viewport;
    }
    push_req(v, req);
}

unsafe extern "C" fn hover_after_scroll(data: *mut c_void) -> GBoolean {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    state.hover_after_scroll_id = 0;
    if state.closed != 0 || state.opened == 0 {
        return SOURCE_REMOVE;
    }
    if unsafe { gtk::gtk_widget_contains(state.area, state.pointer_x, state.pointer_y) } != 0 {
        let s = cur_scale(v);
        request_hover(
            v,
            state.scroll_x + (state.pointer_x / s) as c_int,
            state.scroll_y + (state.pointer_y / s) as c_int,
        );
    }
    SOURCE_REMOVE
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
    unsafe {
        gtk::gtk_adjustment_set_value(adjustment(state.hadj), f64::from(want_x));
        gtk::gtk_adjustment_set_value(adjustment(state.vadj), f64::from(want_y));
    }
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
        unsafe { g_source_remove(state.hover_after_scroll_id) };
    } else {
        route_wheel(v, dx, dy);
    }
    state.hover_after_scroll_id =
        unsafe { g_timeout_add(draw::HOVER_AFTER_SCROLL_MS, hover_after_scroll, v.cast()) };
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

unsafe extern "C" fn wheel_tick(
    _widget: *mut GtkWidget,
    clock: *mut GdkFrameClock,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.closed != 0 || state.opened == 0 || glide(state).idle() {
        state.wheel_tick_id = 0;
        state.wheel_last_us = 0;
        return SOURCE_REMOVE;
    }
    let now = unsafe { gtk::gdk_frame_clock_get_frame_time(clock) };
    let dt_ms = draw::frame_interval_ms(now, state.wheel_last_us);
    state.wheel_last_us = now;
    let mut motion = glide(state);
    let (mx, my) = motion.step(dt_ms);
    state.wheel_left_x = motion.left_x;
    state.wheel_left_y = motion.left_y;
    state.fling_vx = motion.fling_x;
    state.fling_vy = motion.fling_y;
    queue_wheel_scroll(v, mx, my);
    SOURCE_CONTINUE
}

fn arm_wheel_animation(v: *mut NsProcView) {
    let state = view(v);
    if state.wheel_tick_id != 0 || state.area.is_null() || glide(state).idle() {
        return;
    }
    state.wheel_last_us = 0;
    state.wheel_tick_id =
        unsafe { gtk::gtk_widget_add_tick_callback(state.area, Some(wheel_tick), v.cast(), None) };
}

pub(super) fn scroll_target_x(v: *const NsProcView) -> f64 {
    let state = unsafe { &*v };
    f64::from(state.scroll_x) + state.wheel_pend_x + state.wheel_left_x
}

pub(super) fn scroll_target_y(v: *const NsProcView) -> f64 {
    let state = unsafe { &*v };
    f64::from(state.scroll_y) + state.wheel_pend_y + state.wheel_left_y
}

pub(super) fn scroll_view_to(v: *mut NsProcView, x: f64, y: f64) {
    let state = view(v);
    let (max_x, max_y) = unsafe {
        (
            gtk::gtk_adjustment_get_upper(adjustment(state.hadj))
                - gtk::gtk_adjustment_get_page_size(adjustment(state.hadj)),
            gtk::gtk_adjustment_get_upper(adjustment(state.vadj))
                - gtk::gtk_adjustment_get_page_size(adjustment(state.vadj)),
        )
    };
    let x = x.clamp(0.0, max_x.max(0.0));
    let y = y.clamp(0.0, max_y.max(0.0));
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
    state.wheel_viewport = 1;
    state.wheel_left_x += x - scroll_target_x(v);
    state.wheel_left_y += y - scroll_target_y(v);
    arm_wheel_animation(v);
}

unsafe extern "C" fn on_device_scale_changed(
    _object: *mut c_void,
    _pspec: *mut c_void,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.closed != 0 || state.opened == 0 {
        return;
    }
    if !maybe_update_viewport(v) {
        request_render(v);
    }
}

unsafe extern "C" fn on_area_realize(area: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let native = unsafe { gtk::gtk_widget_get_native(area) };
    let surface = if native.is_null() {
        ptr::null_mut()
    } else {
        unsafe { gtk::gtk_native_get_surface(native) }
    };
    let state = view(v);
    if surface.is_null() || surface.cast() == state.scale_surface {
        return;
    }
    if !state.scale_surface.is_null() && state.surface_scale_handler != 0 {
        unsafe {
            gtk::g_signal_handler_disconnect(state.scale_surface, state.surface_scale_handler)
        };
    }
    state.scale_surface = surface.cast();
    state.surface_scale_handler = connect(
        surface.cast(),
        c"notify::scale",
        on_device_scale_changed as *const (),
        v.cast(),
    );
}

unsafe extern "C" fn on_area_unrealize(_area: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if !state.scale_surface.is_null() && state.surface_scale_handler != 0 {
        unsafe {
            gtk::g_signal_handler_disconnect(state.scale_surface, state.surface_scale_handler)
        };
    }
    state.scale_surface = ptr::null_mut();
    state.surface_scale_handler = 0;
}

unsafe extern "C" fn on_area_focus_notify(
    _object: *mut c_void,
    _pspec: *mut c_void,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    view(v).last_anim_frame_us = 0;
    request_render(v);
}

unsafe extern "C" fn anim_tick(
    _widget: *mut GtkWidget,
    clock: *mut GdkFrameClock,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.closed != 0 || state.opened == 0 {
        state.anim_tick_id = 0;
        state.last_anim_frame_us = 0;
        return SOURCE_REMOVE;
    }
    if ns_pv_media_video_playing(v) {
        return SOURCE_CONTINUE;
    }
    let mut now = if clock.is_null() {
        0
    } else {
        unsafe { gtk::gdk_frame_clock_get_frame_time(clock) }
    };
    if now <= 0 {
        now = unsafe { g_get_monotonic_time() };
    }
    let interval = if state.page_animating != 0 {
        draw::ANIM_FRAME_US
    } else {
        draw::CARET_BLINK_US
    };
    if state.last_anim_frame_us > 0 && now - state.last_anim_frame_us < interval {
        return SOURCE_CONTINUE;
    }
    state.last_anim_frame_us = now;
    if state.page_animating != 0 && !wheel_scrolling(v) {
        request_tick(v);
    } else {
        request_render(v);
    }
    SOURCE_CONTINUE
}

pub(super) fn arm_anim(v: *mut NsProcView) {
    let state = view(v);
    if state.anim_tick_id != 0 || state.closed != 0 || state.area.is_null() {
        return;
    }
    state.last_anim_frame_us = 0;
    state.anim_tick_id =
        unsafe { gtk::gtk_widget_add_tick_callback(state.area, Some(anim_tick), v.cast(), None) };
}

pub(super) fn disarm_anim(v: *mut NsProcView) {
    let state = view(v);
    if state.anim_tick_id != 0 && !state.area.is_null() {
        unsafe { gtk::gtk_widget_remove_tick_callback(state.area, state.anim_tick_id) };
    }
    state.anim_tick_id = 0;
    state.last_anim_frame_us = 0;
}

unsafe extern "C" fn on_resize(
    _area: *mut GtkWidget,
    width: c_int,
    height: c_int,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if !state.deferred_url.is_null() && width > 1 && height > 1 {
        let url = state.deferred_url;
        let record = state.deferred_record;
        let history = state.deferred_history;
        let activated = state.deferred_user_activated;
        state.deferred_url = ptr::null_mut();
        do_load(v, url, record != 0, history != 0, activated != 0);
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

unsafe extern "C" fn on_scroll(
    ctrl: *mut GtkEventController,
    dx: f64,
    dy: f64,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.opened == 0 {
        return 0;
    }
    let mut mods = unsafe { gtk::gtk_event_controller_get_current_event_state(ctrl) };
    let event = unsafe { gtk::gtk_event_controller_get_current_event(ctrl) };
    if !event.is_null() {
        mods |= unsafe { gtk::gdk_event_get_modifier_state(event) };
    }
    if mods & gtk::CONTROL_MASK != 0 {
        let delta = if dy != 0.0 { dy } else { dx };
        if delta < 0.0 {
            ns_proc_view_zoom_in(v);
        } else if delta > 0.0 {
            ns_proc_view_zoom_out(v);
        }
        return 1;
    }
    let s = cur_scale(v);
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
    state.wheel_viewport = 0;
    if unsafe { gtk::gtk_event_controller_scroll_get_unit(ctrl) } == gtk::SCROLL_UNIT_SURFACE {
        queue_wheel_scroll(v, dx / s, dy / s);
        return 1;
    }
    state.wheel_left_x += dx * draw::WHEEL_STEP_PX / s;
    state.wheel_left_y += dy * draw::WHEEL_STEP_PX / s;
    arm_wheel_animation(v);
    1
}

unsafe extern "C" fn on_scroll_decelerate(
    ctrl: *mut GtkEventController,
    vel_x: f64,
    vel_y: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.opened == 0
        || unsafe { gtk::gtk_event_controller_get_current_event_state(ctrl) } & gtk::CONTROL_MASK
            != 0
    {
        return;
    }
    let s = cur_scale(v);
    state.fling_vx = vel_x / s;
    state.fling_vy = vel_y / s;
    state.wheel_viewport = 0;
    arm_wheel_animation(v);
}

pub(super) fn ns_pv_area_new(v: *mut NsProcView) -> *mut GtkWidget {
    let widget = unsafe { gtk::g_object_new(area_type().type_id, ptr::null()) }.cast::<GtkWidget>();
    unsafe { *area_view_slot(widget) = v };
    widget
}

/// # Safety
/// `widget` is a drawing area `ns_pv_area_new` created.
pub(super) unsafe fn ns_pv_area_set_view(widget: *mut GtkWidget, v: *mut NsProcView) {
    unsafe { *area_view_slot(widget) = v };
}

pub(super) fn ns_pv_install_view_signals(v: *mut NsProcView) {
    let state = view(v);
    connect(
        state.hadj,
        c"value-changed",
        on_adj_changed as *const (),
        v.cast(),
    );
    connect(
        state.vadj,
        c"value-changed",
        on_adj_changed as *const (),
        v.cast(),
    );
}

pub(super) fn ns_pv_install_area_signals(v: *mut NsProcView) {
    let area = view(v).area.cast::<c_void>();
    connect(area, c"resize", on_resize as *const (), v.cast());
    connect(
        area,
        c"notify::has-focus",
        on_area_focus_notify as *const (),
        v.cast(),
    );
    connect(
        area,
        c"notify::scale-factor",
        on_device_scale_changed as *const (),
        v.cast(),
    );
    connect(area, c"realize", on_area_realize as *const (), v.cast());
    connect(area, c"unrealize", on_area_unrealize as *const (), v.cast());
}

pub(super) fn ns_pv_install_scroll(v: *mut NsProcView) {
    let scroll = unsafe {
        gtk::gtk_event_controller_scroll_new(
            gtk::EVENT_CONTROLLER_SCROLL_BOTH_AXES | gtk::EVENT_CONTROLLER_SCROLL_KINETIC,
        )
    };
    connect(scroll.cast(), c"scroll", on_scroll as *const (), v.cast());
    connect(
        scroll.cast(),
        c"decelerate",
        on_scroll_decelerate as *const (),
        v.cast(),
    );
    unsafe { gtk::gtk_widget_add_controller(view(v).area, scroll.cast()) };
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
