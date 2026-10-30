//! Southstar — forwarding the tab's pointer, key, text, drag and file-drop input to the renderer worker, and the viewport geometry the requests are measured in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};

use southstar_glib::{self as glib, GBoolean};
use southstar_gtk::{
    self as gtk, Adjustment, DropTarget, EventControllerKey, EventControllerMotion, GestureClick,
    GestureDrag, IMContext, Widget,
};
use southstar_gtk_ffi::GdkModifierType;

use super::procview_chrome::{ns_pv_console_set_open, request_find, search_open, view_save};
use super::procview_draw::{scroll_target_x, scroll_target_y};
use super::procview_media::ns_pv_media_forget_rect;
use super::procview_result::{
    ns_proc_view_zoom_in, ns_proc_view_zoom_out, ns_proc_view_zoom_reset,
};
use super::procview_state::{NsProcView, Req};
use super::procview_view::{ns_proc_view_ref, ns_proc_view_unref};
use crate::procview::input::{
    self, ControlKey, EditOp, ScrollFrame, ScrollKey, edit_op, js_code, js_key, js_keycode,
    modifier_bits,
};

const MAX_WIDTH: f64 = 6144.0;
const MAX_HEIGHT: f64 = 3456.0;

const REQ_LINK: c_int = 2;
const REQ_CLICK: c_int = 3;
const REQ_VIEWPORT: c_int = 4;
const REQ_KEY: c_int = 5;
const REQ_SELECT: c_int = 6;
const REQ_HOVER: c_int = 7;
const REQ_RELEASE: c_int = 8;
const REQ_DROPFILES: c_int = 14;
const REQ_SCROLLBAR: c_int = 15;

const ACT_HOVER: c_int = 0;
const ACT_NEWTAB: c_int = 2;
const ACT_CONTEXT: c_int = 3;

unsafe extern "C" {
    fn g_async_queue_push(queue: *mut c_void, data: *mut c_void);
    fn g_async_queue_push_front(queue: *mut c_void, data: *mut c_void);
}

pub(super) fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    // SAFETY: v is the live view whose widgets and signal handlers call in here; the main loop is its only user.
    unsafe { &mut *v }
}

fn opened(v: *mut NsProcView) -> bool {
    view(v).opened != 0
}

pub(super) fn area(v: *mut NsProcView) -> Option<Widget> {
    // SAFETY: area is null or the view's live drawing area; the area's destroy handler clears it.
    unsafe { gtk::from_borrowed(view(v).area.cast()) }
}

pub(super) fn hadj(v: *mut NsProcView) -> Option<Adjustment> {
    // SAFETY: hadj is null or the view's live horizontal adjustment, kept until the view is freed.
    unsafe { gtk::from_borrowed(view(v).hadj) }
}

pub(super) fn vadj(v: *mut NsProcView) -> Option<Adjustment> {
    // SAFETY: vadj is null or the view's live vertical adjustment, kept until the view is freed.
    unsafe { gtk::from_borrowed(view(v).vadj) }
}

pub(super) fn new_req(kind: c_int, fill: impl FnOnce(&mut Req)) -> *mut Req {
    // SAFETY: g_malloc0 aborts rather than return null; the worker frees the request with g_free.
    let req = unsafe { glib::g_malloc0(core::mem::size_of::<Req>()) }.cast::<Req>();
    // SAFETY: req is a fresh, zeroed Req (every field of which may be zero) that nothing else sees until it is queued.
    let r = unsafe { &mut *req };
    r.r#type = kind;
    fill(r);
    req
}

fn send_req(v: *mut NsProcView, kind: c_int, fill: impl FnOnce(&mut Req)) {
    push_req(v, new_req(kind, fill));
}

fn dup(bytes: &[u8]) -> *mut c_char {
    glib::strdup(bytes)
}

fn page_point(v: *mut NsProcView, x: f64, y: f64) -> (c_int, c_int) {
    let s = cur_scale(v);
    let state = view(v);
    (
        state.scroll_x + (x / s) as c_int,
        state.scroll_y + (y / s) as c_int,
    )
}

pub(super) fn viewport_w(v: *mut NsProcView) -> c_int {
    let w = area(v).map_or(0, |area| area.width());
    if w > 0 { w } else { 1 }
}

pub(super) fn viewport_h(v: *mut NsProcView) -> c_int {
    let h = area(v).map_or(0, |area| area.height());
    if h > 0 { h } else { 1 }
}

pub(super) fn cur_scale(v: *mut NsProcView) -> f64 {
    let scale = view(v).scale;
    if scale > 0.0 { scale } else { 1.0 }
}

fn device_scale(v: *mut NsProcView) -> f64 {
    let mut s = 0.0;
    if let Some(area) = area(v) {
        if let Some(surface) = area.surface() {
            s = surface.scale();
        }
        if s.is_nan() || s <= 0.0 {
            s = f64::from(area.scale_factor());
        }
    }
    if (1.0..=8.0).contains(&s) { s } else { 1.0 }
}

pub(super) fn raster_scale(v: *mut NsProcView) -> f64 {
    let mut s = device_scale(v);
    let fit_w = MAX_WIDTH / f64::from(viewport_w(v));
    let fit_h = MAX_HEIGHT / f64::from(viewport_h(v));
    if fit_w < s {
        s = fit_w;
    }
    if fit_h < s {
        s = fit_h;
    }
    if s > 0.0 { s } else { 1.0 }
}

pub(super) fn page_dpr(v: *mut NsProcView) -> f64 {
    cur_scale(v) * device_scale(v)
}

fn css_viewport(size: c_int, scale: f64) -> c_int {
    let css = (f64::from(size) / scale).round() as c_int;
    if css > 0 { css } else { 1 }
}

pub(super) fn push_req(v: *mut NsProcView, req: *mut Req) {
    // SAFETY: the queue lives until the view is freed and takes over req, which the worker frees.
    unsafe { g_async_queue_push(view(v).queue, req.cast()) };
}

pub(super) fn push_req_front(v: *mut NsProcView, req: *mut Req) {
    // SAFETY: the queue lives until the view is freed and takes over req, which the worker frees.
    unsafe { g_async_queue_push_front(view(v).queue, req.cast()) };
}

pub(super) fn start_link(v: *mut NsProcView, x: c_int, y: c_int, action: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.link_inflight = 1;
    state.link_seq += 1;
    let seq = state.link_seq;
    send_req(v, REQ_LINK, |r| {
        r.seq = seq;
        r.x = x;
        r.y = y;
        r.action = action;
    });
}

fn request_link(v: *mut NsProcView, x: c_int, y: c_int, action: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    if state.link_inflight != 0 {
        if action != ACT_HOVER || state.link_pending_action == ACT_HOVER {
            state.link_pending_x = x;
            state.link_pending_y = y;
            state.link_pending_action = action;
        }
        state.link_pending = 1;
        return;
    }
    start_link(v, x, y, action);
}

pub(super) fn start_hover(v: *mut NsProcView, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.hover_inflight = 1;
    state.hover_seq += 1;
    let seq = state.hover_seq;
    send_req(v, REQ_HOVER, |r| {
        r.seq = seq;
        r.x = x;
        r.y = y;
    });
}

pub(super) fn request_hover(v: *mut NsProcView, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    if state.hover_inflight != 0 {
        state.hover_pending_x = x;
        state.hover_pending_y = y;
        state.hover_pending = 1;
        return;
    }
    start_hover(v, x, y);
}

fn start_key_full(
    v: *mut NsProcView,
    kind: c_int,
    keyval: c_uint,
    state: GdkModifierType,
    fallback: Option<(f64, f64)>,
) {
    if !opened(v) {
        return;
    }
    let view_state = view(v);
    if kind == 0 {
        view_state.key_seq += 1;
    }
    let seq = view_state.key_seq;
    let unicode = gtk::keyval_to_unicode(keyval);
    let key = js_key(keyval, unicode, || {
        gtk::keyval_name(keyval).unwrap_or_default()
    });
    send_req(v, REQ_KEY, |r| {
        r.seq = seq;
        r.kind = kind;
        r.keycode = js_keycode(keyval);
        if let Some((x, y)) = fallback {
            r.fallback_scroll = 1;
            r.fallback_x = x;
            r.fallback_y = y;
        }
        r.mods = modifier_bits(state);
        r.key = dup(&key);
        r.code = dup(js_code(keyval).as_bytes());
    });
}

fn start_key(v: *mut NsProcView, kind: c_int, keyval: c_uint, state: GdkModifierType) {
    start_key_full(v, kind, keyval, state, None);
}

fn start_key_text(v: *mut NsProcView, kind: c_int, text: &[u8]) {
    if !opened(v) || text.is_empty() {
        return;
    }
    let state = view(v);
    state.key_seq += 1;
    let seq = state.key_seq;
    send_req(v, REQ_KEY, |r| {
        r.seq = seq;
        r.kind = kind;
        r.key = dup(text);
        r.code = dup(b"");
    });
}

fn start_release(v: *mut NsProcView, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    send_req(v, REQ_RELEASE, |r| {
        r.x = x;
        r.y = y;
    });
}

pub(super) fn start_select(v: *mut NsProcView, kind: c_int, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.select_seq += 1;
    let seq = state.select_seq;
    send_req(v, REQ_SELECT, |r| {
        r.seq = seq;
        r.kind = kind;
        r.x = x;
        r.y = y;
    });
}

fn start_click(v: *mut NsProcView, x: c_int, y: c_int, mods: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.click_seq += 1;
    let seq = state.click_seq;
    send_req(v, REQ_CLICK, |r| {
        r.seq = seq;
        r.x = x;
        r.y = y;
        r.mods = mods;
    });
}

fn start_dropfiles(v: *mut NsProcView, x: c_int, y: c_int, paths: &[u8]) {
    if !opened(v) {
        return;
    }
    send_req(v, REQ_DROPFILES, |r| {
        r.x = x;
        r.y = y;
        r.paths = dup(paths);
    });
}

fn start_viewport(v: *mut NsProcView, width: c_int, height: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.viewport_seq += 1;
    let seq = state.viewport_seq;
    let dpr = page_dpr(v);
    send_req(v, REQ_VIEWPORT, |r| {
        r.seq = seq;
        r.vw = width;
        r.vh = height;
        r.dpr = dpr;
    });
}

pub(super) fn maybe_update_viewport(v: *mut NsProcView) -> bool {
    if !opened(v) || viewport_w(v) <= 1 || viewport_h(v) <= 1 {
        return false;
    }
    let scale = cur_scale(v);
    let w = css_viewport(viewport_w(v), scale);
    let h = css_viewport(viewport_h(v), scale);
    let dpr = page_dpr(v);
    let state = view(v);
    if w == state.last_vp_w && h == state.last_vp_h && dpr == state.last_vp_dpr {
        return false;
    }
    ns_pv_media_forget_rect(v);
    if let Some(area) = area(v) {
        area.queue_draw();
    }
    let state = view(v);
    state.last_vp_w = w;
    state.last_vp_h = h;
    state.last_vp_dpr = dpr;
    start_viewport(v, w, h);
    true
}

fn push_scrollbar(v: *mut NsProcView, kind: c_int, x: c_int, y: c_int) {
    send_req(v, REQ_SCROLLBAR, |r| {
        r.kind = kind;
        r.x = x;
        r.y = y;
    });
}

fn paste_clipboard(v: *mut NsProcView) {
    let area = area(v);
    let Some(area) = area.filter(|_| opened(v)) else {
        return;
    };
    let v = ns_proc_view_ref(v);
    area.read_clipboard_text(move |text| {
        if let Some(text) = text
            && !text.is_empty()
            && opened(v)
        {
            start_key_text(v, 4, &text);
        }
        ns_proc_view_unref(v);
    });
}

pub(super) fn run_edit_op(v: *mut NsProcView, op: EditOp) {
    match op {
        EditOp::Cut => start_select(v, 7, 0, 0),
        EditOp::Copy => start_select(v, 4, 0, 0),
        EditOp::Paste => paste_clipboard(v),
        EditOp::SelectAll => start_select(v, 3, 0, 0),
        EditOp::None => {}
    }
}

fn on_im_commit(v: *mut NsProcView, text: &[u8]) {
    if v.is_null() || !opened(v) {
        return;
    }
    if !text.is_empty() {
        start_key_text(v, 2, text);
    }
}

fn on_pressed(v: *mut NsProcView, gesture: &GestureClick, n_press: c_int, x: f64, y: f64) {
    if !opened(v) {
        return;
    }
    let mods = gesture.current_event_state();
    let button = gesture.current_button();
    if let Some(area) = area(v) {
        area.grab_focus();
    }
    let (px, py) = page_point(v, x, y);
    if button == gtk::sys::BUTTON_MIDDLE || mods & gtk::sys::CONTROL_MASK != 0 {
        request_link(v, px, py, ACT_NEWTAB);
        return;
    }
    let state = view(v);
    if mods & gtk::sys::SHIFT_MASK == 0 {
        state.has_selection = 0;
    }
    start_click(v, px, py, modifier_bits(mods));
    let state = view(v);
    state.multi_click = GBoolean::from(n_press >= 2);
    let select = if mods & gtk::sys::SHIFT_MASK != 0 {
        Some(1)
    } else if n_press == 2 {
        Some(5)
    } else if n_press >= 3 {
        Some(6)
    } else {
        None
    };
    if let Some(kind) = select {
        start_select(v, kind, px, py);
        view(v).has_selection = 1;
    }
}

fn on_released(v: *mut NsProcView, x: f64, y: f64) {
    if opened(v) {
        let (px, py) = page_point(v, x, y);
        start_release(v, px, py);
    }
}

fn on_secondary_pressed(v: *mut NsProcView, gesture: &GestureClick, x: f64, y: f64) {
    if !opened(v) {
        return;
    }
    gesture.claim();
    let state = view(v);
    state.ctx_x = x;
    state.ctx_y = y;
    let (px, py) = page_point(v, x, y);
    request_link(v, px, py, ACT_CONTEXT);
}

fn on_motion(v: *mut NsProcView, x: f64, y: f64) {
    let state = view(v);
    if (x - state.pointer_x).abs() < 0.01 && (y - state.pointer_y).abs() < 0.01 {
        return;
    }
    state.pointer_x = x;
    state.pointer_y = y;
    if opened(v) && state.hover_after_scroll_id == 0 {
        let (px, py) = page_point(v, x, y);
        request_hover(v, px, py);
    }
}

fn on_drag_begin(v: *mut NsProcView, sx: f64, sy: f64) {
    let state = view(v);
    state.drag_start_x = sx;
    state.drag_start_y = sy;
    state.drag_anchored = 0;
    state.sb_dragging = 0;
    state.sb_have_last = 0;
    state.sb_probe = 0;
    if opened(v) {
        state.sb_probe = 1;
        let (px, py) = page_point(v, sx, sy);
        push_scrollbar(v, 0, px, py);
    }
}

fn on_drag_update(v: *mut NsProcView, ox: f64, oy: f64) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    let wx = state.drag_start_x + ox;
    let wy = state.drag_start_y + oy;
    if state.sb_dragging != 0 {
        let (px, py) = page_point(v, wx, wy);
        push_scrollbar(v, 1, px, py);
        return;
    }
    if state.sb_probe != 0 {
        state.sb_last_x = wx;
        state.sb_last_y = wy;
        state.sb_have_last = 1;
        return;
    }
    if state.multi_click != 0 {
        let (px, py) = page_point(v, wx, wy);
        start_select(v, 1, px, py);
        return;
    }
    if state.drag_anchored == 0 {
        let (ax, ay) = page_point(v, state.drag_start_x, state.drag_start_y);
        start_select(v, 0, ax, ay);
        view(v).drag_anchored = 1;
    }
    let (px, py) = page_point(v, wx, wy);
    start_select(v, 1, px, py);
    view(v).has_selection = 1;
}

fn on_drag_end(v: *mut NsProcView) {
    let state = view(v);
    if state.sb_dragging != 0 || state.sb_probe != 0 {
        push_scrollbar(v, 2, 0, 0);
        let state = view(v);
        state.sb_dragging = 0;
        state.sb_probe = 0;
        state.sb_have_last = 0;
    }
}

fn is_cntrl(c: u32) -> bool {
    southstar_unicode::is_control(c)
}

fn on_key(v: *mut NsProcView, keyval: c_uint, state: GdkModifierType) -> bool {
    if !opened(v) {
        return false;
    }
    let control = gtk::sys::CONTROL_MASK;
    let shift = gtk::sys::SHIFT_MASK;
    let alt = gtk::sys::ALT_MASK;
    let meta = gtk::sys::META_MASK;
    let toggle_console = keyval == input::KEY_F12
        || (state & control != 0
            && state & shift != 0
            && (keyval == input::KEY_J || keyval == input::KEY_UPPER_J));
    if toggle_console {
        let open = view(v).console_open == 0;
        ns_pv_console_set_open(v, open);
        return true;
    }
    let edit = edit_op(keyval, state, gtk::keyval_to_lower(keyval));
    if edit != EditOp::None {
        start_key(v, 0, keyval, state);
        run_edit_op(v, edit);
        return true;
    }
    let uc = gtk::keyval_to_unicode(keyval);
    if uc != 0 && uc != u32::from(b' ') && !is_cntrl(uc) && state & (control | alt | meta) == 0 {
        start_key(v, 0, keyval, state);
        start_key(v, 3, keyval, state);
        return false;
    }
    if state & control != 0 {
        start_key(v, 0, keyval, state);
        match input::control_key(keyval, state) {
            ControlKey::ZoomIn => ns_proc_view_zoom_in(v),
            ControlKey::ZoomOut => ns_proc_view_zoom_out(v),
            ControlKey::ZoomReset => ns_proc_view_zoom_reset(v),
            ControlKey::Find => search_open(v),
            ControlKey::FindNext { backwards } => request_find(v, if backwards { 2 } else { 1 }),
            ControlKey::Print => view_save(v, true),
            ControlKey::Other => return false,
        }
        return true;
    }
    if state & alt != 0 && state & control == 0 && state & meta == 0 {
        let a = gtk::keyval_to_unicode(keyval);
        if a != 0 && !is_cntrl(a) {
            start_key(v, 0, keyval, state);
            return true;
        }
    }
    let line = input::LINE_PX;
    let page = (f64::from(viewport_h(v)) / cur_scale(v) - line).max(line);
    let frame = ScrollFrame {
        x: scroll_target_x(v),
        y: scroll_target_y(v),
        page,
        bottom: vadj(v).map_or(0.0, |adj| adj.upper()),
    };
    match input::scroll_key(keyval, &frame) {
        ScrollKey::Tab => {
            start_key(v, 0, keyval, state);
            true
        }
        ScrollKey::To { x, y } => {
            start_key_full(v, 0, keyval, state, Some((x, y)));
            true
        }
        ScrollKey::Unhandled => {
            start_key(v, 0, keyval, state);
            false
        }
    }
}

fn on_key_released(v: *mut NsProcView, keyval: c_uint, state: GdkModifierType) {
    if opened(v) {
        start_key(v, 1, keyval, state);
    }
}

fn on_file_drop(v: *mut NsProcView, files: &[Vec<u8>], x: f64, y: f64) -> bool {
    if !opened(v) {
        return false;
    }
    let paths = files.join(&b'\n');
    if paths.is_empty() {
        return false;
    }
    let (px, py) = page_point(v, x, y);
    start_dropfiles(v, px, py, &paths);
    true
}

pub(super) fn ns_pv_input_install_pointer(v: *mut NsProcView) {
    let Some(area) = area(v) else {
        return;
    };
    let click = GestureClick::new(gtk::sys::BUTTON_PRIMARY);
    click.connect_pressed(move |gesture, n_press, x, y| on_pressed(v, gesture, n_press, x, y));
    click.connect_released(move |_, _, x, y| on_released(v, x, y));
    area.add_controller(click);

    let drag = GestureDrag::new(gtk::sys::BUTTON_PRIMARY);
    drag.connect_drag_begin(move |_, sx, sy| on_drag_begin(v, sx, sy));
    drag.connect_drag_update(move |_, ox, oy| on_drag_update(v, ox, oy));
    drag.connect_drag_end(move |_, _, _| on_drag_end(v));
    area.add_controller(drag);

    let drop = DropTarget::for_files(gtk::sys::ACTION_COPY);
    drop.connect_drop_files(move |files, x, y| on_file_drop(v, files, x, y));
    area.add_controller(drop);

    let middle = GestureClick::new(gtk::sys::BUTTON_MIDDLE);
    middle.connect_pressed(move |gesture, n_press, x, y| on_pressed(v, gesture, n_press, x, y));
    area.add_controller(middle);
}

pub(super) fn ns_pv_input_install_secondary(v: *mut NsProcView) {
    let Some(area) = area(v) else {
        return;
    };
    let secondary = GestureClick::new(gtk::sys::BUTTON_SECONDARY);
    secondary.connect_pressed(move |gesture, _, x, y| on_secondary_pressed(v, gesture, x, y));
    area.add_controller(secondary);

    let motion = EventControllerMotion::new();
    motion.connect_motion(move |_, x, y| on_motion(v, x, y));
    area.add_controller(motion);
}

pub(super) fn ns_pv_input_install_keys(v: *mut NsProcView) {
    let Some(area) = area(v) else {
        return;
    };
    let key = EventControllerKey::new();
    key.connect_key_pressed(move |keyval, _, state| on_key(v, keyval, state));
    key.connect_key_released(move |keyval, _, state| on_key_released(v, keyval, state));
    let im = IMContext::new_multicontext();
    im.connect_commit(move |text| on_im_commit(v, text));
    key.set_im_context(&im);
    view(v).im = im.into_raw();
    area.add_controller(key);
}

pub(super) fn ns_pv_css_viewport_w(v: *mut NsProcView) -> c_int {
    css_viewport(viewport_w(v), cur_scale(v))
}

pub(super) fn ns_pv_css_viewport_h(v: *mut NsProcView) -> c_int {
    css_viewport(viewport_h(v), cur_scale(v))
}
