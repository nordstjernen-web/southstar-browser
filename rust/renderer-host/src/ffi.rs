//! Southstar — a renderer session over the framebuffer memory it paints into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use crate::frame::FrameMemory;
use crate::serve::{Framebuffer, Session};

impl Framebuffer for FrameMemory {
    fn bytes(&mut self) -> &mut [u8] {
        FrameMemory::bytes(self)
    }
}

pub type RendererSession = Session<FrameMemory>;

pub fn new_session(
    ctrl_w: c_int,
    fb: FrameMemory,
    max_w: c_int,
    max_h: c_int,
    shm_mode: bool,
) -> Option<RendererSession> {
    let needed = usize::try_from(max_w)
        .ok()
        .zip(usize::try_from(max_h).ok())
        .filter(|&(w, h)| w > 0 && h > 0)
        .and_then(|(w, h)| w.checked_mul(h)?.checked_mul(4))?;
    if fb.len() < needed {
        return None;
    }
    Some(Session::new(ctrl_w, fb, max_w, max_h, shm_mode))
}
