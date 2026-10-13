//! Southstar — reading an MPEG audio frame header: version, layer, rates, stereo mode and frame size. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::tables::HALFRATE;

pub const HDR_SIZE: usize = 4;

pub fn is_mono(h: &[u8]) -> bool {
    h[3] & 0xC0 == 0xC0
}

pub fn is_ms_stereo(h: &[u8]) -> bool {
    h[3] & 0xE0 == 0x60
}

pub fn is_free_format(h: &[u8]) -> bool {
    h[2] & 0xF0 == 0
}

pub fn is_crc(h: &[u8]) -> bool {
    h[1] & 1 == 0
}

pub fn test_padding(h: &[u8]) -> bool {
    h[2] & 0x2 != 0
}

pub fn test_mpeg1(h: &[u8]) -> bool {
    h[1] & 0x8 != 0
}

pub fn test_not_mpeg25(h: &[u8]) -> bool {
    h[1] & 0x10 != 0
}

pub fn test_i_stereo(h: &[u8]) -> bool {
    h[3] & 0x10 != 0
}

pub fn test_ms_stereo(h: &[u8]) -> bool {
    h[3] & 0x20 != 0
}

pub fn stereo_mode(h: &[u8]) -> u32 {
    ((h[3] >> 6) & 3) as u32
}

pub fn stereo_mode_ext(h: &[u8]) -> u32 {
    ((h[3] >> 4) & 3) as u32
}

pub fn layer(h: &[u8]) -> u32 {
    ((h[1] >> 1) & 3) as u32
}

pub fn bitrate_index(h: &[u8]) -> u32 {
    (h[2] >> 4) as u32
}

pub fn sample_rate_index(h: &[u8]) -> u32 {
    ((h[2] >> 2) & 3) as u32
}

pub fn my_sample_rate(h: &[u8]) -> u32 {
    sample_rate_index(h) + ((((h[1] >> 3) & 1) + ((h[1] >> 4) & 1)) as u32) * 3
}

pub fn is_frame_576(h: &[u8]) -> bool {
    h[1] & 14 == 2
}

pub fn is_layer_1(h: &[u8]) -> bool {
    h[1] & 6 == 6
}

pub fn valid(h: &[u8]) -> bool {
    h[0] == 0xff
        && ((h[1] & 0xF0) == 0xf0 || (h[1] & 0xFE) == 0xe2)
        && layer(h) != 0
        && bitrate_index(h) != 15
        && sample_rate_index(h) != 3
}

pub fn compare(h1: &[u8], h2: &[u8]) -> bool {
    valid(h2)
        && ((h1[1] ^ h2[1]) & 0xFE) == 0
        && ((h1[2] ^ h2[2]) & 0x0C) == 0
        && is_free_format(h1) == is_free_format(h2)
}

pub fn bitrate_kbps(h: &[u8]) -> u32 {
    2 * HALFRATE[test_mpeg1(h) as usize][layer(h) as usize - 1][bitrate_index(h) as usize] as u32
}

pub fn sample_rate_hz(h: &[u8]) -> u32 {
    const HZ: [u32; 3] = [44100, 48000, 32000];
    HZ[sample_rate_index(h) as usize] >> (!test_mpeg1(h) as u32) >> (!test_not_mpeg25(h) as u32)
}

pub fn frame_samples(h: &[u8]) -> u32 {
    if is_layer_1(h) {
        384
    } else {
        1152 >> (is_frame_576(h) as u32)
    }
}

pub fn frame_bytes(h: &[u8], free_format_size: i32) -> i32 {
    let mut bytes = (frame_samples(h) * bitrate_kbps(h) * 125 / sample_rate_hz(h)) as i32;
    if is_layer_1(h) {
        bytes &= !3;
    }
    if bytes != 0 { bytes } else { free_format_size }
}

pub fn padding(h: &[u8]) -> i32 {
    if test_padding(h) {
        if is_layer_1(h) { 4 } else { 1 }
    } else {
        0
    }
}
