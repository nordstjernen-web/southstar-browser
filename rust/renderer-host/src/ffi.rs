//! Southstar — the C ABI of the renderer session, as declared in src/renderer_serve.h and src/print.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_glib::GPtrArray;
use southstar_ipc::Head;

use crate::engine::PrintSetup;
use crate::serve::{Framebuffer, Session};

pub struct SharedFramebuffer {
    pixels: *mut u8,
    len: usize,
}

impl Framebuffer for SharedFramebuffer {
    fn bytes(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.pixels, self.len) }
    }
}

pub type RendererSession = Session<SharedFramebuffer>;

pub fn new_session(
    ctrl_w: c_int,
    fb: *mut u8,
    max_w: c_int,
    max_h: c_int,
    shm_mode: bool,
) -> Option<RendererSession> {
    if fb.is_null() || max_w <= 0 || max_h <= 0 {
        return None;
    }
    let framebuffer = SharedFramebuffer {
        pixels: fb,
        len: max_w as usize * max_h as usize * 4,
    };
    Some(Session::new(ctrl_w, framebuffer, max_w, max_h, shm_mode))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_session_new(
    ctrl_w: c_int,
    fb: *mut u8,
    max_w: c_int,
    max_h: c_int,
    shm_mode: c_int,
) -> *mut RendererSession {
    new_session(ctrl_w, fb, max_w, max_h, shm_mode != 0)
        .map_or(ptr::null_mut(), |s| Box::into_raw(Box::new(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_session_handle(
    s: *mut RendererSession,
    head: *const Head,
    body: *const c_char,
) -> c_int {
    let (Some(s), Some(head)) = (unsafe { s.as_mut() }, unsafe { head.as_ref() }) else {
        return 0;
    };
    let path = head.path.split(|&b| b == 0).next().unwrap_or_default();
    let body = if body.is_null() {
        &[][..]
    } else {
        unsafe { CStr::from_ptr(body) }.to_bytes()
    };
    c_int::from(s.handle(path, body))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_session_free(s: *mut RendererSession) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_session_busy(s: *mut RendererSession) -> c_int {
    unsafe { s.as_mut() }.map_or(0, |s| c_int::from(s.busy()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_session_print(
    s: *mut RendererSession,
    out_setup: *mut PrintSetup,
) -> *mut GPtrArray {
    let (Some(s), Some(setup)) = (unsafe { s.as_mut() }, unsafe { out_setup.as_mut() }) else {
        return ptr::null_mut();
    };
    s.print(setup)
        .map_or(ptr::null_mut(), |pages| pages.into_raw())
}
