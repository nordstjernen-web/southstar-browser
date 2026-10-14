//! Southstar — fuzz target: decodes arbitrary bytes as PNG/APNG, GIF, JPEG or BMP, still and animated.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(canvas) = southstar_raster::decode(data) {
        assert_eq!(
            canvas.pixels.len(),
            canvas.stride() * canvas.height as usize
        );
    }
    let _ = southstar_raster::decode_animation(data);
});
