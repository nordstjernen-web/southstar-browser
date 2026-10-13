//! Southstar — Layer I and Layer II: bit allocation, scalefactors and dequantizing subband samples. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::bits::BitReader;
use crate::header;
use crate::tables::BITALLOC_CODE_TAB;

const MODE_MONO: u32 = 3;
const MODE_JOINT_STEREO: u32 = 1;

pub struct ScaleInfo {
    pub scf: [f32; 3 * 64],
    pub total_bands: u8,
    pub stereo_bands: u8,
    pub bitalloc: [u8; 64],
    pub scfcod: [u8; 64],
}

impl ScaleInfo {
    pub fn new() -> ScaleInfo {
        ScaleInfo {
            scf: [0.0; 3 * 64],
            total_bands: 0,
            stereo_bands: 0,
            bitalloc: [0; 64],
            scfcod: [0; 64],
        }
    }
}

#[derive(Clone, Copy)]
struct SubbandAlloc {
    tab_offset: u8,
    code_tab_width: u8,
    band_count: u8,
}

const fn alloc(tab_offset: u8, code_tab_width: u8, band_count: u8) -> SubbandAlloc {
    SubbandAlloc {
        tab_offset,
        code_tab_width,
        band_count,
    }
}

const ALLOC_L1: [SubbandAlloc; 1] = [alloc(76, 4, 32)];
const ALLOC_L2M2: [SubbandAlloc; 3] = [alloc(60, 4, 4), alloc(44, 3, 7), alloc(44, 2, 19)];
const ALLOC_L2M1: [SubbandAlloc; 4] = [
    alloc(0, 4, 3),
    alloc(16, 4, 8),
    alloc(32, 3, 12),
    alloc(40, 2, 7),
];
const ALLOC_L2M1_LOWRATE: [SubbandAlloc; 2] = [alloc(44, 4, 2), alloc(44, 3, 10)];

const fn deq_l12() -> [f32; 54] {
    const DIVISORS: [f32; 18] = [
        3.0, 7.0, 15.0, 31.0, 63.0, 127.0, 255.0, 511.0, 1023.0, 2047.0, 4095.0, 8191.0, 16383.0,
        32767.0, 65535.0, 3.0, 5.0, 9.0,
    ];
    let mut out = [0.0f32; 54];
    let mut i = 0;
    while i < 18 {
        out[3 * i] = 9.536_743_16e-7_f32 / DIVISORS[i];
        out[3 * i + 1] = 7.569_318_07e-7_f32 / DIVISORS[i];
        out[3 * i + 2] = 6.007_771_73e-7_f32 / DIVISORS[i];
        i += 1;
    }
    out
}

const DEQ_L12: [f32; 54] = deq_l12();

fn subband_alloc_table(hdr: &[u8], sci: &mut ScaleInfo) -> &'static [SubbandAlloc] {
    let mode = header::stereo_mode(hdr);
    let stereo_bands: u32 = if mode == MODE_MONO {
        0
    } else if mode == MODE_JOINT_STEREO {
        (header::stereo_mode_ext(hdr) << 2) + 4
    } else {
        32
    };
    let table: &'static [SubbandAlloc];
    let nbands: u32;
    if header::is_layer_1(hdr) {
        table = &ALLOC_L1;
        nbands = 32;
    } else if !header::test_mpeg1(hdr) {
        table = &ALLOC_L2M2;
        nbands = 30;
    } else {
        let sample_rate_idx = header::sample_rate_index(hdr);
        let mut kbps = header::bitrate_kbps(hdr) >> ((mode != MODE_MONO) as u32);
        if kbps == 0 {
            kbps = 192;
        }
        if kbps < 56 {
            table = &ALLOC_L2M1_LOWRATE;
            nbands = if sample_rate_idx == 2 { 12 } else { 8 };
        } else {
            table = &ALLOC_L2M1;
            nbands = if kbps >= 96 && sample_rate_idx != 1 {
                30
            } else {
                27
            };
        }
    }
    sci.total_bands = nbands as u8;
    sci.stereo_bands = stereo_bands.min(nbands) as u8;
    table
}

fn read_scalefactors(bs: &mut BitReader, sci: &mut ScaleInfo, bands: usize) {
    let mut out = 0;
    for i in 0..bands {
        let mut s = 0.0f32;
        let ba = sci.bitalloc[i] as i32;
        let mask = if ba != 0 {
            4 + ((19 >> sci.scfcod[i]) & 3)
        } else {
            0
        };
        let mut m = 4;
        while m != 0 {
            if mask & m != 0 {
                let b = bs.get_bits(6) as i32;
                s = DEQ_L12[(ba * 3 - 6 + b % 3) as usize] * ((1i32 << 21) >> (b / 3)) as f32;
            }
            sci.scf[out] = s;
            out += 1;
            m >>= 1;
        }
    }
}

pub fn read_scale_info(hdr: &[u8], bs: &mut BitReader, sci: &mut ScaleInfo) {
    let allocs = subband_alloc_table(hdr, sci);
    let mut next_alloc = 0;
    let mut k = 0u32;
    let mut ba_bits = 0;
    let mut ba_code_tab = 0usize;
    for i in 0..sci.total_bands as u32 {
        if i == k {
            let a = allocs[next_alloc];
            k += a.band_count as u32;
            ba_bits = a.code_tab_width as i32;
            ba_code_tab = a.tab_offset as usize;
            next_alloc += 1;
        }
        let mut ba = BITALLOC_CODE_TAB[ba_code_tab + bs.get_bits(ba_bits) as usize];
        sci.bitalloc[2 * i as usize] = ba;
        if i < sci.stereo_bands as u32 {
            ba = BITALLOC_CODE_TAB[ba_code_tab + bs.get_bits(ba_bits) as usize];
        }
        sci.bitalloc[2 * i as usize + 1] = if sci.stereo_bands != 0 { ba } else { 0 };
    }
    for i in 0..2 * sci.total_bands as usize {
        sci.scfcod[i] = if sci.bitalloc[i] != 0 {
            if header::is_layer_1(hdr) {
                2
            } else {
                bs.get_bits(2) as u8
            }
        } else {
            6
        };
    }
    read_scalefactors(bs, sci, sci.total_bands as usize * 2);
    for i in sci.stereo_bands as usize..sci.total_bands as usize {
        sci.bitalloc[2 * i + 1] = 0;
    }
}

pub fn dequantize_granule(
    grbuf: &mut [f32],
    start: usize,
    bs: &mut BitReader,
    sci: &ScaleInfo,
    group_size: usize,
) -> usize {
    for j in 0..4 {
        let mut dst = (start + group_size * j) as isize;
        let mut choff: isize = 576;
        for i in 0..2 * sci.total_bands as usize {
            let ba = sci.bitalloc[i] as i32;
            if ba != 0 {
                let d = dst as usize;
                if ba < 17 {
                    let half = (1 << (ba - 1)) - 1;
                    for k in 0..group_size {
                        grbuf[d + k] = (bs.get_bits(ba) as i32 - half) as f32;
                    }
                } else {
                    let modulus: u32 = (2 << (ba - 17)) + 1;
                    let mut code = bs.get_bits((modulus + 2 - (modulus >> 3)) as i32);
                    for k in 0..group_size {
                        grbuf[d + k] = ((code % modulus).wrapping_sub(modulus / 2) as i32) as f32;
                        code /= modulus;
                    }
                }
            }
            dst += choff;
            choff = 18 - choff;
        }
    }
    group_size * 4
}

pub fn apply_scf_384(sci: &ScaleInfo, scf_offset: usize, dst: &mut [f32]) {
    let stereo = sci.stereo_bands as usize * 18;
    let count = (sci.total_bands as usize - sci.stereo_bands as usize) * 18;
    dst.copy_within(stereo..stereo + count, 576 + stereo);
    let mut d = 0;
    let mut scf = scf_offset;
    for _ in 0..sci.total_bands {
        for k in 0..12 {
            dst[d + k] *= sci.scf[scf];
            dst[d + k + 576] *= sci.scf[scf + 3];
        }
        d += 18;
        scf += 6;
    }
}
