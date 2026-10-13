//! Southstar — decoding a standalone MP3 file to PCM with the in-tree southstar-minimp3 decoder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_minimp3::{Decoder, MAX_SAMPLES_PER_FRAME};

use crate::decode::{Collector, Decoded};

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    let mut dec = Decoder::new();
    let mut frame = [0f32; MAX_SAMPLES_PER_FRAME];
    let (mut rate, mut channels): (c_int, c_int) = (0, 0);
    let mut pcm: Option<Collector> = None;
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (samples, info) = dec.decode_frame(&bytes[pos..], Some(&mut frame));
        if info.frame_bytes == 0 {
            break;
        }
        if samples > 0 {
            if rate == 0 {
                rate = info.hz as c_int;
                channels = info.channels as c_int;
                if rate <= 0 || channels < 1 {
                    return None;
                }
                pcm = Some(Collector::new(rate, channels));
            }
            if info.channels as c_int == channels
                && info.hz as c_int == rate
                && let Some(pcm) = pcm.as_mut()
            {
                let add = (samples * channels as usize).min(frame.len());
                if pcm.push(&frame[..add])? {
                    break;
                }
            }
        }
        pos += info.frame_bytes;
    }
    pcm?.finish(rate, channels)
}
