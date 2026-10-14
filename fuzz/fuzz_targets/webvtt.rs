//! Southstar — fuzz target: parses arbitrary bytes as a WebVTT subtitle file and looks up the active cue.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let cues = southstar_webvtt::parse(data);
    for cue in &cues {
        assert!(cue.start >= 0.0 && cue.end > cue.start);
        let _ = southstar_webvtt::active(&cues, cue.start);
    }
});
