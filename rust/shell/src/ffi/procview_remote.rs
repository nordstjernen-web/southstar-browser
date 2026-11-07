//! Southstar — the tab view's remote-control entry points: script evaluation, console reads, clicks, keys, text, scrolling and screenshots through the view's own input path.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{c_int, c_uint};
use std::collections::HashMap;

use southstar_glib as glib;
use southstar_gtk as gtk;
use southstar_gtk_ffi::GdkModifierType;

use super::procview_chrome::ns_pv_console_append;
use super::procview_draw::{queue_wheel_scroll, request_render};
use super::procview_input::{
    area, cur_scale, new_req, on_key, on_key_released, push_req, request_hover, request_link,
    start_click, start_key_text, start_release, start_select, view, viewport_h, viewport_w,
};
use super::procview_state::{NsProcView, Res};
use super::view::View;
use crate::remote::{MOD_ALT, MOD_CONTROL, MOD_META, MOD_SHIFT};

const REQ_CONSOLE: c_int = 11;
const REQ_EVAL: c_int = 12;
const RES_CONSOLE: c_int = 12;
const ACT_NEWTAB: c_int = 2;

pub type TextFn = Box<dyn FnOnce(Option<Vec<u8>>)>;

thread_local! {
    static PENDING: RefCell<HashMap<c_int, TextFn>> = RefCell::new(HashMap::new());
    static NEXT: Cell<c_int> = const { Cell::new(1) };
}

fn register(cb: TextFn) -> c_int {
    let id = NEXT.with(|next| {
        let id = next.get();
        next.set(if id >= c_int::MAX - 1 { 1 } else { id + 1 });
        id
    });
    PENDING.with(|p| p.borrow_mut().insert(id, cb));
    id
}

fn usable(v: View) -> bool {
    let state = view(v.raw());
    state.closed == 0 && state.opened != 0
}

fn send(v: View, kind: c_int, js: Option<&[u8]>, cb: TextFn) {
    let state = view(v.raw());
    if state.closed != 0 {
        cb(None);
        return;
    }
    let id = register(cb);
    push_req(
        v.raw(),
        new_req(kind, |r| {
            r.remote = id;
            if let Some(js) = js {
                r.query = glib::strdup(js);
            }
        }),
    );
}

pub(super) fn finish(v: *mut NsProcView, res: &Res) {
    let Some(cb) = PENDING.with(|p| p.borrow_mut().remove(&res.remote)) else {
        return;
    };
    let closed = view(v).closed != 0;
    let text = super::procview_result::text(res.href).map(|t| t.to_bytes().to_vec());
    if closed {
        cb(None);
        return;
    }
    if res.r#type == RES_CONSOLE {
        if let Some(text) = &text {
            ns_pv_console_append(v, text);
        }
        cb(Some(text.unwrap_or_default()));
    } else {
        cb(text);
        request_render(v);
    }
}

pub fn eval(v: View, js: &[u8], cb: TextFn) {
    send(v, REQ_EVAL, Some(js), cb);
}

pub fn console(v: View, cb: TextFn) {
    send(v, REQ_CONSOLE, None, cb);
}

fn to_page(v: View, x: f64, y: f64, page: bool) -> (c_int, c_int) {
    let state = view(v.raw());
    if page {
        (x as c_int, y as c_int)
    } else {
        (state.scroll_x + x as c_int, state.scroll_y + y as c_int)
    }
}

fn place_pointer(v: View, px: c_int, py: c_int) {
    let s = cur_scale(v.raw());
    let state = view(v.raw());
    state.pointer_x = f64::from(px - state.scroll_x) * s;
    state.pointer_y = f64::from(py - state.scroll_y) * s;
}

fn gdk_mods(mods: i32) -> GdkModifierType {
    let mut out = 0;
    for (bit, mask) in [
        (MOD_SHIFT, gtk::sys::SHIFT_MASK),
        (MOD_CONTROL, gtk::sys::CONTROL_MASK),
        (MOD_ALT, gtk::sys::ALT_MASK),
        (MOD_META, gtk::sys::META_MASK),
    ] {
        if mods & bit != 0 {
            out |= mask;
        }
    }
    out
}

fn focus(v: View) {
    if let Some(area) = area(v.raw()) {
        area.grab_focus();
    }
}

pub fn click(v: View, x: f64, y: f64, page: bool, count: i32, mods: i32) -> bool {
    if !usable(v) {
        return false;
    }
    let (px, py) = to_page(v, x, y, page);
    place_pointer(v, px, py);
    focus(v);
    if mods & (MOD_CONTROL | MOD_META) != 0 {
        request_link(v.raw(), px, py, ACT_NEWTAB);
        return true;
    }
    for n in 1..=count.clamp(1, 3) {
        if mods & MOD_SHIFT == 0 {
            view(v.raw()).has_selection = 0;
        }
        start_click(v.raw(), px, py, mods);
        view(v.raw()).multi_click = c_int::from(n >= 2);
        if n >= 2 {
            start_select(v.raw(), if n == 2 { 5 } else { 6 }, px, py);
            view(v.raw()).has_selection = 1;
        }
        start_release(v.raw(), px, py);
    }
    true
}

pub fn hover(v: View, x: f64, y: f64, page: bool) -> bool {
    if !usable(v) {
        return false;
    }
    let (px, py) = to_page(v, x, y, page);
    place_pointer(v, px, py);
    request_hover(v.raw(), px, py);
    true
}

pub fn key(v: View, keyval: c_uint, mods: i32) -> bool {
    if !usable(v) {
        return false;
    }
    focus(v);
    let state = gdk_mods(mods);
    let handled = on_key(v.raw(), keyval, state);
    let uc = gtk::keyval_to_unicode(keyval);
    if !handled
        && uc >= 0x20
        && uc != 0x7f
        && mods & (MOD_CONTROL | MOD_ALT | MOD_META) == 0
        && let Some(c) = char::from_u32(uc)
    {
        let mut buf = [0u8; 4];
        start_key_text(v.raw(), 2, c.encode_utf8(&mut buf).as_bytes());
    }
    on_key_released(v.raw(), keyval, state);
    true
}

pub fn text(v: View, text: &str) -> bool {
    if !usable(v) {
        return false;
    }
    focus(v);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            key(v, crate::procview::input::KEY_RETURN, 0);
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.is_empty() {
            start_key_text(v.raw(), 2, line.as_bytes());
        }
    }
    true
}

pub fn scroll(v: View, x: f64, y: f64, dx: f64, dy: f64) -> bool {
    if !usable(v) {
        return false;
    }
    let s = cur_scale(v.raw());
    let state = view(v.raw());
    state.pointer_x = x * s;
    state.pointer_y = y * s;
    state.fling_vx = 0.0;
    state.fling_vy = 0.0;
    state.wheel_viewport = 0;
    queue_wheel_scroll(v.raw(), dx, dy);
    true
}

pub fn rendering(v: View) -> bool {
    let state = view(v.raw());
    state.render_inflight != 0 || state.render_pending != 0 || state.wheel_tick_id != 0
}

pub fn refresh(v: View) {
    if usable(v) {
        request_render(v.raw());
    }
}

pub struct Geometry {
    pub width: c_int,
    pub height: c_int,
    pub scroll_x: c_int,
    pub scroll_y: c_int,
    pub scale: f64,
}

pub fn geometry(v: View) -> Geometry {
    let s = cur_scale(v.raw());
    let state = view(v.raw());
    Geometry {
        width: (f64::from(viewport_w(v.raw())) / s).round() as c_int,
        height: (f64::from(viewport_h(v.raw())) / s).round() as c_int,
        scroll_x: state.scroll_x,
        scroll_y: state.scroll_y,
        scale: s,
    }
}

pub fn screenshot(v: View) -> Option<(Vec<u8>, c_int, c_int)> {
    if view(v.raw()).closed != 0 {
        return None;
    }
    let area = area(v.raw())?;
    let s = cur_scale(v.raw());
    let w = (f64::from(area.width()) / s).round().max(1.0) as c_int;
    let h = (f64::from(area.height()) / s).round().max(1.0) as c_int;
    area.render_png(w, h)
}
