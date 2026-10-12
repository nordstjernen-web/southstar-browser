//! Southstar — the MPEG-1 backend: the vendored pl_mpeg decoder over an in-memory stream.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::NonNull;

use super::{Next, pixel_buffer};
use crate::ffi::texture_from_bgra;

#[repr(C)]
struct Plm {
    _private: [u8; 0],
}

#[repr(C)]
struct PlmFrame {
    time: f64,
}

unsafe extern "C" {
    fn plm_create_with_memory(bytes: *mut u8, length: usize, free_when_done: c_int) -> *mut Plm;
    fn plm_destroy(plm: *mut Plm);
    fn plm_get_width(plm: *mut Plm) -> c_int;
    fn plm_get_height(plm: *mut Plm) -> c_int;
    fn plm_set_audio_enabled(plm: *mut Plm, enabled: c_int);
    fn plm_get_num_audio_streams(plm: *mut Plm) -> c_int;
    fn plm_get_duration(plm: *mut Plm) -> f64;
    fn plm_rewind(plm: *mut Plm);
    fn plm_set_loop(plm: *mut Plm, looping: c_int);
    fn plm_has_ended(plm: *mut Plm) -> c_int;
    fn plm_decode_video(plm: *mut Plm) -> *mut PlmFrame;
    fn plm_frame_to_bgra(frame: *mut PlmFrame, dest: *mut u8, stride: c_int);
}

pub struct Mpeg1 {
    plm: NonNull<Plm>,
    _stream: Vec<u8>,
}

impl Mpeg1 {
    pub fn open(mut stream: Vec<u8>) -> Option<Mpeg1> {
        let plm =
            NonNull::new(unsafe { plm_create_with_memory(stream.as_mut_ptr(), stream.len(), 0) })?;
        unsafe {
            plm_set_audio_enabled(plm.as_ptr(), 0);
            plm_set_loop(plm.as_ptr(), 0);
        }
        Some(Mpeg1 {
            plm,
            _stream: stream,
        })
    }

    pub fn width(&self) -> i32 {
        unsafe { plm_get_width(self.plm.as_ptr()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { plm_get_height(self.plm.as_ptr()) }
    }

    pub fn audio_streams(&self) -> i32 {
        unsafe { plm_get_num_audio_streams(self.plm.as_ptr()) }
    }

    pub fn duration(&self) -> f64 {
        unsafe { plm_get_duration(self.plm.as_ptr()) }
    }

    pub fn rewind(&mut self) {
        unsafe { plm_rewind(self.plm.as_ptr()) };
    }

    pub fn next(&mut self, w: i32, h: i32) -> Next {
        let frame = unsafe { plm_decode_video(self.plm.as_ptr()) };
        if frame.is_null() {
            return if unsafe { plm_has_ended(self.plm.as_ptr()) } != 0 {
                Next::Ended
            } else {
                Next::Stalled
            };
        }
        let stride = w as usize * 4;
        let tex = pixel_buffer(stride * h as usize, 0xFF).and_then(|mut bgra| {
            unsafe { plm_frame_to_bgra(frame, bgra.as_mut_ptr(), stride as c_int) };
            texture_from_bgra(w, h, &bgra, stride)
        });
        Next::Frame(tex, unsafe { (*frame).time })
    }
}

impl Drop for Mpeg1 {
    fn drop(&mut self) {
        unsafe { plm_destroy(self.plm.as_ptr()) };
    }
}
