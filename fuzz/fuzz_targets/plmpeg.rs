//! Southstar — fuzz target: demuxes arbitrary bytes as an MPEG-1 program stream and decodes its video and MP2 audio.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_plmpeg::Plm;

fuzz_target!(|data: &[u8]| {
    let mut plm = Plm::new(data.to_vec());
    if !plm.has_headers() {
        return;
    }
    let _ = (
        plm.width(),
        plm.height(),
        plm.framerate(),
        plm.samplerate(),
        plm.duration(),
    );
    for _ in 0..16 {
        let video = plm.decode_video().is_some();
        let audio = plm.decode_audio().is_some();
        if !video && !audio {
            break;
        }
    }
    let _ = plm.seek_frame(0.5, true).is_some();
});
