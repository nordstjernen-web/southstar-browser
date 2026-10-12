//! Southstar — decoding an MPEG-1 program stream's MP2 audio track to PCM with the vendored pl_mpeg.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint};

use crate::decode::{Collector, Decoded};

const SAMPLES_PER_FRAME: usize = 1152;

#[repr(C)]
struct Plm {
    _private: [u8; 0],
}

#[repr(C)]
struct PlmSamples {
    _time: f64,
    count: c_uint,
    interleaved: [f32; SAMPLES_PER_FRAME * 2],
}

unsafe extern "C" {
    fn plm_create_with_memory(bytes: *mut u8, length: usize, free_when_done: c_int) -> *mut Plm;
    fn plm_destroy(plm: *mut Plm);
    fn plm_set_video_enabled(plm: *mut Plm, enabled: c_int);
    fn plm_set_audio_enabled(plm: *mut Plm, enabled: c_int);
    fn plm_get_num_audio_streams(plm: *mut Plm) -> c_int;
    fn plm_get_samplerate(plm: *mut Plm) -> c_int;
    fn plm_decode_audio(plm: *mut Plm) -> *mut PlmSamples;
}

struct Handle(*mut Plm);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { plm_destroy(self.0) };
    }
}

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    let plm = unsafe { plm_create_with_memory(bytes.as_ptr().cast_mut(), bytes.len(), 0) };
    if plm.is_null() {
        return None;
    }
    let plm = Handle(plm);
    unsafe {
        plm_set_video_enabled(plm.0, 0);
        plm_set_audio_enabled(plm.0, 1);
    }
    let rate = unsafe { plm_get_samplerate(plm.0) };
    if unsafe { plm_get_num_audio_streams(plm.0) } < 1 || rate <= 0 || rate > 48000 {
        return None;
    }
    let mut pcm = Collector::new(rate, 2);
    loop {
        let samples = unsafe { plm_decode_audio(plm.0) };
        if samples.is_null() {
            break;
        }
        let samples = unsafe { &*samples };
        let add = (samples.count as usize * 2).min(samples.interleaved.len());
        if pcm.push(&samples.interleaved[..add])? {
            break;
        }
    }
    pcm.finish(rate, 2)
}
