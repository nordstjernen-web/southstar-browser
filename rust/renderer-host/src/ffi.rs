//! Southstar — a renderer session over the shared framebuffer the shell maps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

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
