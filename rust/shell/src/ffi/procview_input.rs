//! Southstar — forwarding the tab's pointer, key, text, drag and file-drop input to the renderer worker, and the viewport geometry the requests are measured in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean, GStr};
use southstar_gtk_ffi::{
    self as gtk, GSList, GValue, GdkModifierType, GtkEventController, GtkGesture,
};

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
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_slist_free(list: *mut GSList);
    fn g_unichar_iscntrl(c: u32) -> GBoolean;
}

fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    unsafe { &mut *v }
}

fn opened(v: *mut NsProcView) -> bool {
    view(v).opened != 0
}

fn new_req(kind: c_int) -> *mut Req {
    let req = unsafe { g_malloc0(core::mem::size_of::<Req>()) }.cast::<Req>();
    unsafe { (*req).r#type = kind };
    req
}

fn dup(bytes: &[u8]) -> *mut c_char {
    glib::strdup(bytes)
}

fn connect(instance: *mut c_void, signal: &CStr, handler: *const (), v: *mut NsProcView) {
    unsafe {
        gtk::g_signal_connect_data(
            instance,
            signal.as_ptr(),
            core::mem::transmute::<*const (), unsafe extern "C" fn()>(handler),
            v.cast(),
            None,
            0,
        )
    };
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
    let area = view(v).area;
    let w = if area.is_null() {
        0
    } else {
        unsafe { gtk::gtk_widget_get_width(area) }
    };
    if w > 0 { w } else { 1 }
}

pub(super) fn viewport_h(v: *mut NsProcView) -> c_int {
    let area = view(v).area;
    let h = if area.is_null() {
        0
    } else {
        unsafe { gtk::gtk_widget_get_height(area) }
    };
    if h > 0 { h } else { 1 }
}

pub(super) fn cur_scale(v: *mut NsProcView) -> f64 {
    let scale = view(v).scale;
    if scale > 0.0 { scale } else { 1.0 }
}

fn device_scale(v: *mut NsProcView) -> f64 {
    let area = view(v).area;
    let mut s = 0.0;
    if !area.is_null() {
        let native = unsafe { gtk::gtk_widget_get_native(area) };
        let surface = if native.is_null() {
            ptr::null_mut()
        } else {
            unsafe { gtk::gtk_native_get_surface(native) }
        };
        if !surface.is_null() {
            s = unsafe { gtk::gdk_surface_get_scale(surface) };
        }
        if s.is_nan() || s <= 0.0 {
            s = f64::from(unsafe { gtk::gtk_widget_get_scale_factor(area) });
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
    unsafe { g_async_queue_push(view(v).queue, req.cast()) };
}

pub(super) fn start_link(v: *mut NsProcView, x: c_int, y: c_int, action: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.link_inflight = 1;
    state.link_seq += 1;
    let req = new_req(REQ_LINK);
    unsafe {
        (*req).seq = state.link_seq;
        (*req).x = x;
        (*req).y = y;
        (*req).action = action;
    }
    push_req(v, req);
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
    let req = new_req(REQ_HOVER);
    unsafe {
        (*req).seq = state.hover_seq;
        (*req).x = x;
        (*req).y = y;
    }
    push_req(v, req);
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
    let unicode = unsafe { gtk::gdk_keyval_to_unicode(keyval) };
    let key = js_key(keyval, unicode, || {
        unsafe { glib::bytes(gtk::gdk_keyval_name(keyval)) }
            .unwrap_or_default()
            .to_vec()
    });
    let req = new_req(REQ_KEY);
    unsafe {
        (*req).seq = view_state.key_seq;
        (*req).kind = kind;
        (*req).keycode = js_keycode(keyval);
        if let Some((x, y)) = fallback {
            (*req).fallback_scroll = 1;
            (*req).fallback_x = x;
            (*req).fallback_y = y;
        }
        (*req).mods = modifier_bits(state);
        (*req).key = dup(&key);
        (*req).code = dup(js_code(keyval).as_bytes());
    }
    push_req(v, req);
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
    let req = new_req(REQ_KEY);
    unsafe {
        (*req).seq = state.key_seq;
        (*req).kind = kind;
        (*req).key = dup(text);
        (*req).code = dup(b"");
    }
    push_req(v, req);
}

fn start_release(v: *mut NsProcView, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    let req = new_req(REQ_RELEASE);
    unsafe {
        (*req).x = x;
        (*req).y = y;
    }
    push_req(v, req);
}

pub(super) fn start_select(v: *mut NsProcView, kind: c_int, x: c_int, y: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.select_seq += 1;
    let req = new_req(REQ_SELECT);
    unsafe {
        (*req).seq = state.select_seq;
        (*req).kind = kind;
        (*req).x = x;
        (*req).y = y;
    }
    push_req(v, req);
}

fn start_click(v: *mut NsProcView, x: c_int, y: c_int, mods: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.click_seq += 1;
    let req = new_req(REQ_CLICK);
    unsafe {
        (*req).seq = state.click_seq;
        (*req).x = x;
        (*req).y = y;
        (*req).mods = mods;
    }
    push_req(v, req);
}

fn start_dropfiles(v: *mut NsProcView, x: c_int, y: c_int, paths: &[u8]) {
    if !opened(v) {
        return;
    }
    let req = new_req(REQ_DROPFILES);
    unsafe {
        (*req).x = x;
        (*req).y = y;
        (*req).paths = dup(paths);
    }
    push_req(v, req);
}

fn start_viewport(v: *mut NsProcView, width: c_int, height: c_int) {
    if !opened(v) {
        return;
    }
    let state = view(v);
    state.viewport_seq += 1;
    let req = new_req(REQ_VIEWPORT);
    unsafe {
        (*req).seq = state.viewport_seq;
        (*req).vw = width;
        (*req).vh = height;
        (*req).dpr = page_dpr(v);
    }
    push_req(v, req);
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
    unsafe { gtk::gtk_widget_queue_draw(state.area) };
    state.last_vp_w = w;
    state.last_vp_h = h;
    state.last_vp_dpr = dpr;
    start_viewport(v, w, h);
    true
}

fn push_scrollbar(v: *mut NsProcView, kind: c_int, x: c_int, y: c_int) {
    let req = new_req(REQ_SCROLLBAR);
    unsafe {
        (*req).kind = kind;
        (*req).x = x;
        (*req).y = y;
    }
    push_req(v, req);
}

unsafe extern "C" fn on_paste_text_ready(src: *mut c_void, result: *mut c_void, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let text = unsafe {
        GStr::take(gtk::gdk_clipboard_read_text_finish(
            src.cast(),
            result,
            ptr::null_mut(),
        ))
    };
    if let Some(text) = text
        && !text.is_empty()
        && opened(v)
    {
        start_key_text(v, 4, text.to_bytes());
    }
    ns_proc_view_unref(v);
}

fn paste_clipboard(v: *mut NsProcView) {
    let area = view(v).area;
    if !opened(v) || area.is_null() {
        return;
    }
    unsafe {
        gtk::gdk_clipboard_read_text_async(
            gtk::gtk_widget_get_clipboard(area),
            ptr::null_mut(),
            Some(on_paste_text_ready),
            ns_proc_view_ref(v).cast(),
        )
    };
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

unsafe extern "C" fn on_im_commit(_im: *mut c_void, text: *const c_char, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    if v.is_null() || !opened(v) {
        return;
    }
    if let Some(text) = unsafe { glib::bytes(text) }
        && !text.is_empty()
    {
        start_key_text(v, 2, text);
    }
}

unsafe extern "C" fn on_pressed(
    gesture: *mut GtkGesture,
    n_press: c_int,
    x: f64,
    y: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    if !opened(v) {
        return;
    }
    let mods = unsafe { gtk::gtk_event_controller_get_current_event_state(gesture.cast()) };
    let button = unsafe { gtk::gtk_gesture_single_get_current_button(gesture) };
    unsafe { gtk::gtk_widget_grab_focus(view(v).area) };
    let (px, py) = page_point(v, x, y);
    if button == gtk::BUTTON_MIDDLE || mods & gtk::CONTROL_MASK != 0 {
        request_link(v, px, py, ACT_NEWTAB);
        return;
    }
    let state = view(v);
    if mods & gtk::SHIFT_MASK == 0 {
        state.has_selection = 0;
    }
    start_click(v, px, py, modifier_bits(mods));
    state.multi_click = GBoolean::from(n_press >= 2);
    let select = if mods & gtk::SHIFT_MASK != 0 {
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
        state.has_selection = 1;
    }
}

unsafe extern "C" fn on_released(
    _gesture: *mut GtkGesture,
    _n_press: c_int,
    x: f64,
    y: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    if opened(v) {
        let (px, py) = page_point(v, x, y);
        start_release(v, px, py);
    }
}

unsafe extern "C" fn on_secondary_pressed(
    gesture: *mut GtkGesture,
    _n_press: c_int,
    x: f64,
    y: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    if !opened(v) {
        return;
    }
    unsafe { gtk::gtk_gesture_set_state(gesture, gtk::EVENT_SEQUENCE_CLAIMED) };
    let state = view(v);
    state.ctx_x = x;
    state.ctx_y = y;
    let (px, py) = page_point(v, x, y);
    request_link(v, px, py, ACT_CONTEXT);
}

unsafe extern "C" fn on_motion(_ctrl: *mut GtkEventController, x: f64, y: f64, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
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

unsafe extern "C" fn on_drag_begin(_gesture: *mut GtkGesture, sx: f64, sy: f64, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
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

unsafe extern "C" fn on_drag_update(
    _gesture: *mut GtkGesture,
    ox: f64,
    oy: f64,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
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
        state.drag_anchored = 1;
    }
    let (px, py) = page_point(v, wx, wy);
    start_select(v, 1, px, py);
    state.has_selection = 1;
}

unsafe extern "C" fn on_drag_end(_gesture: *mut GtkGesture, _ox: f64, _oy: f64, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    if state.sb_dragging != 0 || state.sb_probe != 0 {
        push_scrollbar(v, 2, 0, 0);
        state.sb_dragging = 0;
        state.sb_probe = 0;
        state.sb_have_last = 0;
    }
}

fn is_cntrl(c: u32) -> bool {
    unsafe { g_unichar_iscntrl(c) != 0 }
}

unsafe extern "C" fn on_key(
    _ctrl: *mut GtkEventController,
    keyval: c_uint,
    _keycode: c_uint,
    state: GdkModifierType,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    if !opened(v) {
        return 0;
    }
    let toggle_console = keyval == input::KEY_F12
        || (state & gtk::CONTROL_MASK != 0
            && state & gtk::SHIFT_MASK != 0
            && (keyval == input::KEY_J || keyval == input::KEY_UPPER_J));
    if toggle_console {
        let open = view(v).console_open == 0;
        ns_pv_console_set_open(v, open);
        return 1;
    }
    let edit = edit_op(keyval, state, unsafe { gtk::gdk_keyval_to_lower(keyval) });
    if edit != EditOp::None {
        start_key(v, 0, keyval, state);
        run_edit_op(v, edit);
        return 1;
    }
    let uc = unsafe { gtk::gdk_keyval_to_unicode(keyval) };
    if uc != 0
        && uc != u32::from(b' ')
        && !is_cntrl(uc)
        && state & (gtk::CONTROL_MASK | gtk::ALT_MASK | gtk::META_MASK) == 0
    {
        start_key(v, 0, keyval, state);
        start_key(v, 3, keyval, state);
        return 0;
    }
    if state & gtk::CONTROL_MASK != 0 {
        start_key(v, 0, keyval, state);
        match input::control_key(keyval, state) {
            ControlKey::ZoomIn => ns_proc_view_zoom_in(v),
            ControlKey::ZoomOut => ns_proc_view_zoom_out(v),
            ControlKey::ZoomReset => ns_proc_view_zoom_reset(v),
            ControlKey::Find => search_open(v),
            ControlKey::FindNext { backwards } => request_find(v, if backwards { 2 } else { 1 }),
            ControlKey::Print => view_save(v, true),
            ControlKey::Other => return 0,
        }
        return 1;
    }
    if state & gtk::ALT_MASK != 0 && state & gtk::CONTROL_MASK == 0 && state & gtk::META_MASK == 0 {
        let a = unsafe { gtk::gdk_keyval_to_unicode(keyval) };
        if a != 0 && !is_cntrl(a) {
            start_key(v, 0, keyval, state);
            return 1;
        }
    }
    let line = input::LINE_PX;
    let page = (f64::from(viewport_h(v)) / cur_scale(v) - line).max(line);
    let frame = ScrollFrame {
        x: scroll_target_x(v),
        y: scroll_target_y(v),
        page,
        bottom: unsafe { gtk::gtk_adjustment_get_upper(view(v).vadj.cast()) },
    };
    match input::scroll_key(keyval, &frame) {
        ScrollKey::Tab => {
            start_key(v, 0, keyval, state);
            1
        }
        ScrollKey::To { x, y } => {
            start_key_full(v, 0, keyval, state, Some((x, y)));
            1
        }
        ScrollKey::Unhandled => {
            start_key(v, 0, keyval, state);
            0
        }
    }
}

unsafe extern "C" fn on_key_released(
    _ctrl: *mut GtkEventController,
    keyval: c_uint,
    _keycode: c_uint,
    state: GdkModifierType,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    if opened(v) {
        start_key(v, 1, keyval, state);
    }
}

unsafe extern "C" fn on_file_drop(
    _target: *mut c_void,
    value: *const GValue,
    x: f64,
    y: f64,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    if !opened(v)
        || unsafe { gtk::g_type_check_value_holds(value, gtk::gdk_file_list_get_type()) } == 0
    {
        return 0;
    }
    let list = unsafe { gtk::g_value_get_boxed(value) };
    if list.is_null() {
        return 0;
    }
    let files = unsafe { gtk::gdk_file_list_get_files(list.cast()) };
    let mut paths: Vec<u8> = Vec::new();
    let mut node = files;
    while let Some(item) = unsafe { node.as_ref() } {
        if let Some(path) = unsafe { GStr::take(gtk::g_file_get_path(item.data)) } {
            if !paths.is_empty() {
                paths.push(b'\n');
            }
            paths.extend_from_slice(path.to_bytes());
        }
        node = item.next;
    }
    unsafe { g_slist_free(files) };
    if paths.is_empty() {
        return 0;
    }
    let (px, py) = page_point(v, x, y);
    start_dropfiles(v, px, py, &paths);
    1
}

fn click_gesture(button: c_uint) -> *mut GtkGesture {
    let gesture = unsafe { gtk::gtk_gesture_click_new() }.cast::<GtkGesture>();
    unsafe { gtk::gtk_gesture_single_set_button(gesture, button) };
    gesture
}

fn add_controller(v: *mut NsProcView, controller: *mut c_void) {
    unsafe { gtk::gtk_widget_add_controller(view(v).area, controller.cast()) };
}

pub(super) fn ns_pv_input_install_pointer(v: *mut NsProcView) {
    let click = click_gesture(gtk::BUTTON_PRIMARY);
    connect(click.cast(), c"pressed", on_pressed as *const (), v);
    connect(click.cast(), c"released", on_released as *const (), v);
    add_controller(v, click.cast());

    let drag = unsafe { gtk::gtk_gesture_drag_new() };
    unsafe { gtk::gtk_gesture_single_set_button(drag, gtk::BUTTON_PRIMARY) };
    connect(drag.cast(), c"drag-begin", on_drag_begin as *const (), v);
    connect(drag.cast(), c"drag-update", on_drag_update as *const (), v);
    connect(drag.cast(), c"drag-end", on_drag_end as *const (), v);
    add_controller(v, drag.cast());

    let drop = unsafe { gtk::gtk_drop_target_new(gtk::gdk_file_list_get_type(), gtk::ACTION_COPY) };
    connect(drop.cast(), c"drop", on_file_drop as *const (), v);
    add_controller(v, drop.cast());

    let middle = click_gesture(gtk::BUTTON_MIDDLE);
    connect(middle.cast(), c"pressed", on_pressed as *const (), v);
    add_controller(v, middle.cast());
}

pub(super) fn ns_pv_input_install_secondary(v: *mut NsProcView) {
    let secondary = click_gesture(gtk::BUTTON_SECONDARY);
    connect(
        secondary.cast(),
        c"pressed",
        on_secondary_pressed as *const (),
        v,
    );
    add_controller(v, secondary.cast());

    let motion = unsafe { gtk::gtk_event_controller_motion_new() };
    connect(motion.cast(), c"motion", on_motion as *const (), v);
    add_controller(v, motion.cast());
}

pub(super) fn ns_pv_input_install_keys(v: *mut NsProcView) {
    let key = unsafe { gtk::gtk_event_controller_key_new() }.cast::<gtk::GtkEventController>();
    connect(key.cast(), c"key-pressed", on_key as *const (), v);
    connect(key.cast(), c"key-released", on_key_released as *const (), v);
    let im = unsafe { gtk::gtk_im_multicontext_new() };
    let state = view(v);
    state.im = im.cast();
    unsafe { gtk::gtk_im_context_set_client_widget(im, state.area) };
    connect(im.cast(), c"commit", on_im_commit as *const (), v);
    unsafe { gtk::gtk_event_controller_key_set_im_context(key, im) };
    add_controller(v, key.cast());
}

pub(super) fn ns_pv_css_viewport_w(v: *mut NsProcView) -> c_int {
    css_viewport(viewport_w(v), cur_scale(v))
}

pub(super) fn ns_pv_css_viewport_h(v: *mut NsProcView) -> c_int {
    css_viewport(viewport_h(v), cur_scale(v))
}
