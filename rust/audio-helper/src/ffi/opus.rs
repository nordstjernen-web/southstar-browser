//! Southstar — decoding Ogg/Opus to PCM with libopusfile.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use crate::decode::{Collector, Decoded};

const RATE: c_int = 48000;
const MAX_FRAME_SAMPLES: c_int = 5760;

#[repr(C)]
struct OggOpusFile {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn op_open_memory(data: *const u8, size: usize, error: *mut c_int) -> *mut OggOpusFile;
    fn op_channel_count(of: *const OggOpusFile, li: c_int) -> c_int;
    fn op_read_float(of: *mut OggOpusFile, pcm: *mut f32, buf_size: c_int, li: *mut c_int)
    -> c_int;
    fn op_free(of: *mut OggOpusFile);
}

struct File(*mut OggOpusFile);

impl Drop for File {
    fn drop(&mut self) {
        unsafe { op_free(self.0) };
    }
}

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    let of = unsafe { op_open_memory(bytes.as_ptr(), bytes.len(), ptr::null_mut()) };
    if of.is_null() {
        return None;
    }
    let of = File(of);
    let channels = unsafe { op_channel_count(of.0, -1) };
    if !(1..=8).contains(&channels) {
        return None;
    }
    let frame_cap = MAX_FRAME_SAMPLES * channels;
    let mut frame = vec![0f32; frame_cap as usize];
    let mut pcm = Collector::new(RATE, channels);
    loop {
        let got = unsafe { op_read_float(of.0, frame.as_mut_ptr(), frame_cap, ptr::null_mut()) };
        if got == 0 {
            break;
        }
        if got < 0 {
            return None;
        }
        let add = (got as usize * channels as usize).min(frame.len());
        if pcm.push(&frame[..add])? {
            break;
        }
    }
    drop(of);
    pcm.finish(RATE, channels)
}
