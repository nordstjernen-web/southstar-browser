//! Southstar — pointer lock in a tab: hides the cursor, keeps it on the page and turns mouse motion into movement while the page holds the lock.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use super::procview_input::{area, new_req, push_req, view};
use super::procview_state::NsProcView;
use super::procview_worker::REQ_POINTER_UNLOCK;

#[derive(Default)]
pub struct PointerLock {
    active: bool,
    suppressed: bool,
    anchor: (f64, f64),
    last: (f64, f64),
    rest: (f64, f64),
    pending: (c_int, c_int),
    warping: bool,
    centre: (i32, i32),
}

fn lock<'a>(v: *mut NsProcView) -> &'a mut PointerLock {
    &mut view(v).plock
}

pub(super) fn active(v: *mut NsProcView) -> bool {
    lock(v).active
}

pub(super) fn anchor(v: *mut NsProcView) -> (f64, f64) {
    lock(v).anchor
}

pub(super) fn take_movement(v: *mut NsProcView) -> (c_int, c_int) {
    core::mem::take(&mut lock(v).pending)
}

fn whole(rest: &mut f64, delta: f64) -> c_int {
    *rest += delta;
    let whole = rest.trunc();
    *rest -= whole;
    whole as c_int
}

fn engage(v: *mut NsProcView) {
    let (x, y) = match area(v) {
        Some(area) => (
            f64::from(area.width()) / 2.0,
            f64::from(area.height()) / 2.0,
        ),
        None => {
            let state = view(v);
            (state.pointer_x, state.pointer_y)
        }
    };
    let pl = lock(v);
    pl.active = true;
    pl.anchor = (x, y);
    pl.last = (x, y);
    pl.rest = (0.0, 0.0);
    pl.pending = (0, 0);
    if let Some(area) = area(v) {
        area.set_cursor_from_name(Some(c"none"));
    }
    let centre = confine(v);
    let pl = lock(v);
    pl.warping = centre.is_some();
    pl.centre = centre.unwrap_or_default();
}

fn release(v: *mut NsProcView) {
    let pl = lock(v);
    if !pl.active {
        return;
    }
    pl.active = false;
    pl.pending = (0, 0);
    if core::mem::take(&mut pl.warping) {
        unconfine();
    }
    if let Some(area) = area(v) {
        area.set_cursor_from_name(None);
    }
}

fn ask_unlock(v: *mut NsProcView) {
    push_req(v, new_req(REQ_POINTER_UNLOCK, |_| {}));
}

pub(super) fn sync(v: *mut NsProcView, locked: bool) {
    if !locked {
        lock(v).suppressed = false;
        release(v);
        return;
    }
    let pl = lock(v);
    if pl.suppressed || pl.active {
        return;
    }
    if area(v).is_some_and(|area| area.has_focus()) {
        engage(v);
        return;
    }
    lock(v).suppressed = true;
    ask_unlock(v);
}

pub(super) fn user_release(v: *mut NsProcView) {
    if !lock(v).active {
        return;
    }
    release(v);
    lock(v).suppressed = true;
    ask_unlock(v);
}

pub(super) fn motion(v: *mut NsProcView, x: f64, y: f64) -> bool {
    let pl = lock(v);
    if !pl.active {
        return false;
    }
    let (mut mx, mut my) = (x - pl.last.0, y - pl.last.1);
    pl.last = (x, y);
    if pl.warping {
        let centre = pl.centre;
        let Some((cx, cy)) = cursor_pos() else {
            return true;
        };
        let scale = surface_scale(v);
        mx = f64::from(cx - centre.0) / scale;
        my = f64::from(cy - centre.1) / scale;
        if (cx, cy) != centre {
            let centre = confine(v);
            let pl = lock(v);
            pl.warping = centre.is_some();
            pl.centre = centre.unwrap_or(pl.centre);
        }
    }
    let pl = lock(v);
    let dx = whole(&mut pl.rest.0, mx);
    let dy = whole(&mut pl.rest.1, my);
    pl.pending.0 += dx;
    pl.pending.1 += dy;
    true
}

fn surface_scale(v: *mut NsProcView) -> f64 {
    let scale = area(v)
        .and_then(|area| area.surface())
        .map_or(1.0, |surface| surface.scale());
    if scale > 0.0 { scale } else { 1.0 }
}

#[cfg(windows)]
fn confine(v: *mut NsProcView) -> Option<(i32, i32)> {
    let area = area(v)?;
    let surface = area.surface()?;
    let scale = surface.scale().max(1.0);
    let (w, h) = (f64::from(area.width()), f64::from(area.height()));
    let top_left = area.surface_point(0.0, 0.0)?;
    let bottom_right = area.surface_point(w, h)?;
    let device = |(x, y): (f64, f64)| ((x * scale).round() as i32, (y * scale).round() as i32);
    win32::confine(surface.as_ptr(), device(top_left), device(bottom_right))
}

#[cfg(not(windows))]
fn confine(_v: *mut NsProcView) -> Option<(i32, i32)> {
    None
}

#[cfg(windows)]
fn unconfine() {
    win32::unconfine();
}

#[cfg(not(windows))]
fn unconfine() {}

#[cfg(windows)]
fn cursor_pos() -> Option<(i32, i32)> {
    win32::cursor_pos()
}

#[cfg(not(windows))]
fn cursor_pos() -> Option<(i32, i32)> {
    None
}

#[cfg(windows)]
mod win32 {
    use core::ffi::c_void;
    use core::ptr;

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    unsafe extern "C" {
        fn gdk_win32_surface_get_handle(surface: *mut c_void) -> *mut c_void;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn ClientToScreen(hwnd: *mut c_void, point: *mut Point) -> i32;
        fn ClipCursor(rect: *const Rect) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
    }

    fn to_screen(hwnd: *mut c_void, (x, y): (i32, i32)) -> Option<(i32, i32)> {
        let mut point = Point { x, y };
        // SAFETY: hwnd is the live window of the tab's surface and point is valid for the call.
        let ok = unsafe { ClientToScreen(hwnd, &mut point) };
        (ok != 0).then_some((point.x, point.y))
    }

    pub fn confine(
        surface: *mut c_void,
        top_left: (i32, i32),
        bottom_right: (i32, i32),
    ) -> Option<(i32, i32)> {
        // SAFETY: surface is the live GdkWin32Surface the tab is drawn on.
        let hwnd = unsafe { gdk_win32_surface_get_handle(surface) };
        if hwnd.is_null() {
            return None;
        }
        let (left, top) = to_screen(hwnd, top_left)?;
        let (right, bottom) = to_screen(hwnd, bottom_right)?;
        let rect = Rect {
            left,
            top,
            right,
            bottom,
        };
        // SAFETY: rect is a valid rectangle for the call.
        unsafe { ClipCursor(&rect) };
        let centre = ((left + right) / 2, (top + bottom) / 2);
        // SAFETY: SetCursorPos takes plain screen coordinates.
        unsafe { SetCursorPos(centre.0, centre.1) };
        Some(centre)
    }

    pub fn unconfine() {
        // SAFETY: a null rectangle lifts the clip.
        unsafe { ClipCursor(ptr::null()) };
    }

    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut point = Point::default();
        // SAFETY: point is valid for the call.
        let ok = unsafe { GetCursorPos(&mut point) };
        (ok != 0).then_some((point.x, point.y))
    }
}
