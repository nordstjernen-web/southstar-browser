//! Southstar — decoding a standalone MP3 file to PCM with the vendored minimp3.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use crate::decode::{Collector, Decoded};

const MAX_SAMPLES_PER_FRAME: usize = 1152 * 2;
const DECODER_BYTES: usize =
    ((2 * 9 * 32 + 15 * 2 * 32) * 4 + 2 * 4 + 4 + 511usize).next_multiple_of(4);

#[repr(C, align(4))]
struct Mp3Dec {
    _state: [u8; DECODER_BYTES],
}

#[repr(C)]
#[derive(Default)]
struct FrameInfo {
    frame_bytes: c_int,
    _frame_offset: c_int,
    channels: c_int,
    hz: c_int,
    _layer: c_int,
    _bitrate_kbps: c_int,
}

unsafe extern "C" {
    fn mp3dec_init(dec: *mut Mp3Dec);
    fn mp3dec_decode_frame(
        dec: *mut Mp3Dec,
        mp3: *const u8,
        mp3_bytes: c_int,
        pcm: *mut f32,
        info: *mut FrameInfo,
    ) -> c_int;
}

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    if bytes.len() > c_int::MAX as usize {
        return None;
    }
    let mut dec = Box::new(Mp3Dec {
        _state: [0; DECODER_BYTES],
    });
    unsafe { mp3dec_init(&mut *dec) };
    let mut frame = vec![0f32; MAX_SAMPLES_PER_FRAME];
    let mut info = FrameInfo::default();
    let (mut rate, mut channels) = (0, 0);
    let mut pcm: Option<Collector> = None;
    let mut pos = 0usize;
    while pos < bytes.len() {
        let samples = unsafe {
            mp3dec_decode_frame(
                &mut *dec,
                bytes.as_ptr().add(pos),
                (bytes.len() - pos) as c_int,
                frame.as_mut_ptr(),
                &mut info,
            )
        };
        if info.frame_bytes <= 0 {
            break;
        }
        if samples > 0 {
            if rate == 0 {
                rate = info.hz;
                channels = info.channels;
                if rate <= 0 || channels < 1 {
                    return None;
                }
                pcm = Some(Collector::new(rate, channels));
            }
            if info.channels == channels
                && info.hz == rate
                && let Some(pcm) = pcm.as_mut()
            {
                let add = (samples as usize * channels as usize).min(frame.len());
                if pcm.push(&frame[..add])? {
                    break;
                }
            }
        }
        pos += info.frame_bytes as usize;
    }
    pcm?.finish(rate, channels)
}
