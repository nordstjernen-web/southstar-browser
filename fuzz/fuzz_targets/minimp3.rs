//! Southstar — fuzz target: decodes arbitrary bytes frame by frame as MPEG-1/2/2.5 Layer I/II/III audio.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_minimp3::{Decoder, MAX_SAMPLES_PER_FRAME};

fuzz_target!(|data: &[u8]| {
    let mut decoder = Decoder::new();
    let mut pcm = [0f32; MAX_SAMPLES_PER_FRAME];
    let mut rest = data;
    while !rest.is_empty() {
        let (samples, info) = decoder.decode_frame(rest, Some(&mut pcm));
        assert!(samples <= MAX_SAMPLES_PER_FRAME);
        if info.frame_bytes == 0 || info.frame_bytes > rest.len() {
            break;
        }
        rest = &rest[info.frame_bytes..];
    }
});
