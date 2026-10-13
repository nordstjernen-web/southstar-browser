//! Southstar — the MPEG-1 layer II (MP2) audio decoder to interleaved stereo float samples. Ported from pl_mpeg by Dominic Szablewski (MIT).
//! Copyright 2019 Dominic Szablewski
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

#![allow(clippy::excessive_precision)]

use crate::audio_tables::SYNTHESIS_WINDOW;
use crate::buffer::{Bits, Buffer};

pub const SAMPLES_PER_FRAME: usize = 1152;

const FRAME_SYNC: i32 = 0x7ff;
const MPEG_1: i32 = 0x3;
const LAYER_II: i32 = 0x2;
const MODE_JOINT_STEREO: i32 = 0x1;
const MODE_MONO: i32 = 0x3;

const SAMPLE_RATE: [u16; 8] = [44100, 48000, 32000, 0, 22050, 24000, 16000, 0];

const BIT_RATE: [i16; 28] = [
    32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 8, 16, 24, 32, 40, 48, 56, 64,
    80, 96, 112, 128, 144, 160,
];

const SCALEFACTOR_BASE: [i32; 3] = [0x02000000, 0x01965FEA, 0x01428A30];

const QUANT_LUT_STEP_1: [[u8; 16]; 2] = [
    [0, 0, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 0, 0],
    [0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2, 2, 0, 0],
];

const QUANT_TAB_A: u8 = 27 | 64;
const QUANT_TAB_B: u8 = 30 | 64;
const QUANT_TAB_C: u8 = 8;
const QUANT_TAB_D: u8 = 12;

const QUANT_LUT_STEP_2: [[u8; 3]; 3] = [
    [QUANT_TAB_C, QUANT_TAB_C, QUANT_TAB_D],
    [QUANT_TAB_A, QUANT_TAB_A, QUANT_TAB_A],
    [QUANT_TAB_B, QUANT_TAB_A, QUANT_TAB_B],
];

#[rustfmt::skip]
const QUANT_LUT_STEP_3: [[u8; 32]; 3] = [
    [
        0x44, 0x44,
        0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0x43, 0x43, 0x43,
        0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
        0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0, 0,
    ],
    [
        0x45, 0x45, 0x45, 0x45,
        0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34,
        0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24,
        0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24,
        0, 0,
    ],
];

#[rustfmt::skip]
const QUANT_LUT_STEP_4: [[u8; 16]; 6] = [
    [0, 1, 2, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 2, 3, 4, 5, 6, 17, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17],
    [0, 1, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
    [0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
];

#[derive(Clone, Copy)]
struct Quantizer {
    levels: u16,
    group: bool,
    bits: u8,
}

const fn q(levels: u16, group: bool, bits: u8) -> Quantizer {
    Quantizer {
        levels,
        group,
        bits,
    }
}

const QUANT_TAB: [Quantizer; 17] = [
    q(3, true, 5),
    q(5, true, 7),
    q(7, false, 3),
    q(9, true, 10),
    q(15, false, 4),
    q(31, false, 5),
    q(63, false, 6),
    q(127, false, 7),
    q(255, false, 8),
    q(511, false, 9),
    q(1023, false, 10),
    q(2047, false, 11),
    q(4095, false, 12),
    q(8191, false, 13),
    q(16383, false, 14),
    q(32767, false, 15),
    q(65535, false, 16),
];

pub struct Samples {
    pub time: f64,
    pub interleaved: [f32; SAMPLES_PER_FRAME * 2],
}

pub struct Audio {
    time: f64,
    samples_decoded: i32,
    samplerate_index: usize,
    bitrate_index: usize,
    mode: i32,
    bound: i32,
    v_pos: i32,
    next_frame_data_size: i32,
    has_header: bool,
    allocation: [[Option<Quantizer>; 32]; 2],
    scale_factor_info: [[u8; 32]; 2],
    scale_factor: [[[i32; 3]; 32]; 2],
    sample: [[[i32; 3]; 32]; 2],
    samples: Samples,
    d: [f32; 1024],
    v: [[f32; 1024]; 2],
    u: [f32; 32],
}

impl Audio {
    pub fn new<B: Bits>(src: &mut B) -> Box<Audio> {
        let mut d = [0.0; 1024];
        d[..512].copy_from_slice(&SYNTHESIS_WINDOW);
        d[512..].copy_from_slice(&SYNTHESIS_WINDOW);
        let mut audio = Box::new(Audio {
            time: 0.0,
            samples_decoded: 0,
            samplerate_index: 3,
            bitrate_index: 0,
            mode: 0,
            bound: 0,
            v_pos: 0,
            next_frame_data_size: 0,
            has_header: false,
            allocation: [[None; 32]; 2],
            scale_factor_info: [[0; 32]; 2],
            scale_factor: [[[0; 3]; 32]; 2],
            sample: [[[0; 3]; 32]; 2],
            samples: Samples {
                time: 0.0,
                interleaved: [0.0; SAMPLES_PER_FRAME * 2],
            },
            d,
            v: [[0.0; 1024]; 2],
            u: [0.0; 32],
        });
        audio.next_frame_data_size = audio.decode_header(src);
        audio
    }

    pub fn has_header<B: Bits>(&mut self, src: &mut B) -> bool {
        if self.has_header {
            return true;
        }
        self.next_frame_data_size = self.decode_header(src);
        self.has_header
    }

    pub fn samplerate(&self) -> i32 {
        SAMPLE_RATE[self.samplerate_index] as i32
    }

    pub fn samples(&self) -> &Samples {
        &self.samples
    }

    pub fn rewind(&mut self) {
        self.time = 0.0;
        self.samples_decoded = 0;
        self.next_frame_data_size = 0;
    }

    pub fn decode<B: Bits>(&mut self, src: &mut B) -> bool {
        if self.next_frame_data_size == 0 {
            if !src.has(48) {
                return false;
            }
            self.next_frame_data_size = self.decode_header(src);
        }
        if self.next_frame_data_size == 0 || !src.has((self.next_frame_data_size << 3) as usize) {
            return false;
        }
        self.decode_frame(src);
        self.next_frame_data_size = 0;
        self.samples.time = self.time;
        self.samples_decoded += SAMPLES_PER_FRAME as i32;
        self.time = self.samples_decoded as f64 / SAMPLE_RATE[self.samplerate_index] as f64;
        true
    }

    fn decode_header<B: Bits>(&mut self, src: &mut B) -> i32 {
        if !src.has(48) {
            return 0;
        }
        src.skip_bytes(0x00);
        let sync = src.read(11);
        if sync != FRAME_SYNC && !find_frame_sync(src.buf()) {
            return 0;
        }
        let version = src.read(2);
        let layer = src.read(2);
        let has_crc = src.read(1) == 0;
        if version != MPEG_1 || layer != LAYER_II {
            return 0;
        }
        let bitrate_index = src.read(4) - 1;
        if !(0..=13).contains(&bitrate_index) {
            return 0;
        }
        let samplerate_index = src.read(2);
        if samplerate_index == 3 {
            return 0;
        }
        let (bitrate_index, samplerate_index) = (bitrate_index as usize, samplerate_index as usize);
        let padding = src.read(1);
        src.skip(1);
        let mode = src.read(2);
        if self.has_header
            && (self.bitrate_index != bitrate_index
                || self.samplerate_index != samplerate_index
                || self.mode != mode)
        {
            return 0;
        }
        self.bitrate_index = bitrate_index;
        self.samplerate_index = samplerate_index;
        self.mode = mode;
        self.has_header = true;
        if mode == MODE_JOINT_STEREO {
            self.bound = (src.read(2) + 1) << 2;
        } else {
            src.skip(2);
            self.bound = if mode == MODE_MONO { 0 } else { 32 };
        }
        src.skip(4);
        if has_crc {
            src.skip(16);
        }
        let bitrate = BIT_RATE[bitrate_index] as i32;
        let samplerate = SAMPLE_RATE[samplerate_index] as i32;
        let frame_size = (144000 * bitrate / samplerate) + padding;
        frame_size - if has_crc { 6 } else { 4 }
    }

    fn read_allocation<B: Bits>(src: &mut B, sb: usize, tab3: usize) -> Option<Quantizer> {
        let tab4 = QUANT_LUT_STEP_3[tab3][sb] as usize;
        let qtab = QUANT_LUT_STEP_4[tab4 & 15][src.read((tab4 >> 4) as u32) as usize] as usize;
        if qtab != 0 {
            Some(QUANT_TAB[qtab - 1])
        } else {
            None
        }
    }

    fn decode_frame<B: Bits>(&mut self, src: &mut B) {
        let tab1 = if self.mode == MODE_MONO { 0 } else { 1 };
        let tab2 = QUANT_LUT_STEP_1[tab1][self.bitrate_index] as usize;
        let tab3 = QUANT_LUT_STEP_2[tab2][self.samplerate_index] as i32;
        let sblimit = (tab3 & 63) as usize;
        let tab3 = (tab3 >> 6) as usize;

        if self.bound > sblimit as i32 {
            self.bound = sblimit as i32;
        }
        let bound = self.bound as usize;

        for sb in 0..bound {
            self.allocation[0][sb] = Self::read_allocation(src, sb, tab3);
            self.allocation[1][sb] = Self::read_allocation(src, sb, tab3);
        }
        for sb in bound..sblimit {
            let allocation = Self::read_allocation(src, sb, tab3);
            self.allocation[0][sb] = allocation;
            self.allocation[1][sb] = allocation;
        }

        let mono = self.mode == MODE_MONO;
        let channels = if mono { 1 } else { 2 };
        for sb in 0..sblimit {
            for ch in 0..channels {
                if self.allocation[ch][sb].is_some() {
                    self.scale_factor_info[ch][sb] = src.read(2) as u8;
                }
            }
            if mono {
                self.scale_factor_info[1][sb] = self.scale_factor_info[0][sb];
            }
        }

        for sb in 0..sblimit {
            for ch in 0..channels {
                if self.allocation[ch][sb].is_some() {
                    let sf = &mut self.scale_factor[ch][sb];
                    match self.scale_factor_info[ch][sb] {
                        0 => {
                            sf[0] = src.read(6);
                            sf[1] = src.read(6);
                            sf[2] = src.read(6);
                        }
                        1 => {
                            let v = src.read(6);
                            sf[0] = v;
                            sf[1] = v;
                            sf[2] = src.read(6);
                        }
                        2 => {
                            let v = src.read(6);
                            *sf = [v; 3];
                        }
                        _ => {
                            sf[0] = src.read(6);
                            let v = src.read(6);
                            sf[1] = v;
                            sf[2] = v;
                        }
                    }
                }
            }
            if mono {
                self.scale_factor[1][sb] = self.scale_factor[0][sb];
            }
        }

        let mut out_pos = 0;
        for part in 0..3 {
            for _granule in 0..4 {
                for sb in 0..bound {
                    self.read_samples(src, 0, sb, part);
                    self.read_samples(src, 1, sb, part);
                }
                for sb in bound..sblimit {
                    self.read_samples(src, 0, sb, part);
                    self.sample[1][sb] = self.sample[0][sb];
                }
                for sb in sblimit..32 {
                    self.sample[0][sb] = [0; 3];
                    self.sample[1][sb] = [0; 3];
                }

                for p in 0..3 {
                    self.v_pos = (self.v_pos - 64) & 1023;
                    for ch in 0..2 {
                        let v_pos = self.v_pos as usize;
                        idct36(&self.sample[ch], p, &mut self.v[ch], v_pos);
                        self.u = [0.0; 32];
                        let mut d_index = 512 - (v_pos >> 1);
                        let mut v_index = (v_pos % 128) >> 1;
                        while v_index < 1024 {
                            for i in 0..32 {
                                self.u[i] += self.d[d_index] * self.v[ch][v_index];
                                d_index += 1;
                                v_index += 1;
                            }
                            v_index += 128 - 32;
                            d_index += 64 - 32;
                        }
                        d_index -= 512 - 32;
                        v_index = (128 - 32 + 1024) - v_index;
                        while v_index < 1024 {
                            for i in 0..32 {
                                self.u[i] += self.d[d_index] * self.v[ch][v_index];
                                d_index += 1;
                                v_index += 1;
                            }
                            v_index += 128 - 32;
                            d_index += 64 - 32;
                        }
                        for j in 0..32 {
                            self.samples.interleaved[((out_pos + j) << 1) + ch] =
                                self.u[j] / -1090519040.0f32;
                        }
                    }
                    out_pos += 32;
                }
            }
        }
        src.align();
    }

    fn read_samples<B: Bits>(&mut self, src: &mut B, ch: usize, sb: usize, part: usize) {
        let Some(q) = self.allocation[ch][sb] else {
            self.sample[ch][sb] = [0; 3];
            return;
        };
        let mut sf = self.scale_factor[ch][sb][part];
        if sf == 63 {
            sf = 0;
        } else {
            let shift = sf / 3;
            sf = (SCALEFACTOR_BASE[(sf % 3) as usize] + ((1 << shift) >> 1)) >> shift;
        }
        let mut adj = q.levels as i32;
        let sample = &mut self.sample[ch][sb];
        if q.group {
            let mut val = src.read(q.bits as u32);
            sample[0] = val % adj;
            val /= adj;
            sample[1] = val % adj;
            sample[2] = val / adj;
        } else {
            sample[0] = src.read(q.bits as u32);
            sample[1] = src.read(q.bits as u32);
            sample[2] = src.read(q.bits as u32);
        }
        let scale = 65536 / (adj + 1);
        adj = ((adj + 1) >> 1) - 1;
        for s in sample.iter_mut() {
            let val = (adj - *s) * scale;
            *s = (val * (sf >> 12) + ((val * (sf & 4095) + 2048) >> 12)) >> 12;
        }
    }
}

fn find_frame_sync(buf: &mut Buffer) -> bool {
    let end = buf.bytes.len().wrapping_sub(1);
    let mut i = buf.bit_index >> 3;
    while i < end {
        if buf.bytes[i] == 0xFF && (buf.bytes[i + 1] & 0xFE) == 0xFC {
            buf.bit_index = ((i + 1) << 3) + 3;
            return true;
        }
        i += 1;
    }
    buf.bit_index = (i + 1) << 3;
    false
}

fn idct36(s: &[[i32; 3]; 32], ss: usize, d: &mut [f32; 1024], dp: usize) {
    let mut t01 = (s[0][ss] + s[31][ss]) as f32;
    let mut t02 = (s[0][ss] - s[31][ss]) as f32 * 0.500602998235_f32;
    let mut t03 = (s[1][ss] + s[30][ss]) as f32;
    let mut t04 = (s[1][ss] - s[30][ss]) as f32 * 0.505470959898_f32;
    let mut t05 = (s[2][ss] + s[29][ss]) as f32;
    let mut t06 = (s[2][ss] - s[29][ss]) as f32 * 0.515447309923_f32;
    let mut t07 = (s[3][ss] + s[28][ss]) as f32;
    let mut t08 = (s[3][ss] - s[28][ss]) as f32 * 0.53104259109_f32;
    let mut t09 = (s[4][ss] + s[27][ss]) as f32;
    let mut t10 = (s[4][ss] - s[27][ss]) as f32 * 0.553103896034_f32;
    let mut t11 = (s[5][ss] + s[26][ss]) as f32;
    let mut t12 = (s[5][ss] - s[26][ss]) as f32 * 0.582934968206_f32;
    let mut t13 = (s[6][ss] + s[25][ss]) as f32;
    let mut t14 = (s[6][ss] - s[25][ss]) as f32 * 0.622504123036_f32;
    let mut t15 = (s[7][ss] + s[24][ss]) as f32;
    let mut t16 = (s[7][ss] - s[24][ss]) as f32 * 0.674808341455_f32;
    let mut t17 = (s[8][ss] + s[23][ss]) as f32;
    let mut t18 = (s[8][ss] - s[23][ss]) as f32 * 0.744536271002_f32;
    let mut t19 = (s[9][ss] + s[22][ss]) as f32;
    let mut t20 = (s[9][ss] - s[22][ss]) as f32 * 0.839349645416_f32;
    let mut t21 = (s[10][ss] + s[21][ss]) as f32;
    let mut t22 = (s[10][ss] - s[21][ss]) as f32 * 0.972568237862_f32;
    let mut t23 = (s[11][ss] + s[20][ss]) as f32;
    let mut t24 = (s[11][ss] - s[20][ss]) as f32 * 1.16943993343_f32;
    let mut t25 = (s[12][ss] + s[19][ss]) as f32;
    let mut t26 = (s[12][ss] - s[19][ss]) as f32 * 1.48416461631_f32;
    let mut t27 = (s[13][ss] + s[18][ss]) as f32;
    let mut t28 = (s[13][ss] - s[18][ss]) as f32 * 2.05778100995_f32;
    let mut t29 = (s[14][ss] + s[17][ss]) as f32;
    let mut t30 = (s[14][ss] - s[17][ss]) as f32 * 3.40760841847_f32;
    let mut t31 = (s[15][ss] + s[16][ss]) as f32;
    let mut t32 = (s[15][ss] - s[16][ss]) as f32 * 10.1900081235_f32;

    let mut t33 = t01 + t31;
    t31 = (t01 - t31) * C1;
    t01 = t03 + t29;
    t29 = (t03 - t29) * C2;
    t03 = t05 + t27;
    t27 = (t05 - t27) * C3;
    t05 = t07 + t25;
    t25 = (t07 - t25) * C4;
    t07 = t09 + t23;
    t23 = (t09 - t23) * C5;
    t09 = t11 + t21;
    t21 = (t11 - t21) * C6;
    t11 = t13 + t19;
    t19 = (t13 - t19) * C7;
    t13 = t15 + t17;
    t17 = (t15 - t17) * C8;
    t15 = t33 + t13;
    t13 = (t33 - t13) * E1;
    t33 = t01 + t11;
    t01 = (t01 - t11) * E2;
    t11 = t03 + t09;
    t09 = (t03 - t09) * E3;
    t03 = t05 + t07;
    t07 = (t05 - t07) * E4;
    t05 = t15 + t03;
    t15 = (t15 - t03) * F1;
    t03 = t33 + t11;
    t11 = (t33 - t11) * F2;
    t33 = t05 + t03;
    t05 = (t05 - t03) * G;
    t03 = t15 + t11;
    t15 = (t15 - t11) * G;
    t03 += t15;
    t11 = t13 + t07;
    t13 = (t13 - t07) * F1;
    t07 = t01 + t09;
    t09 = (t01 - t09) * F2;
    t01 = t11 + t07;
    t07 = (t11 - t07) * G;
    t11 = t13 + t09;
    t13 = (t13 - t09) * G;
    t11 += t13;
    t01 += t11;
    t11 += t07;
    t07 += t13;
    t09 = t31 + t17;
    t31 = (t31 - t17) * E1;
    t17 = t29 + t19;
    t29 = (t29 - t19) * E2;
    t19 = t27 + t21;
    t21 = (t27 - t21) * E3;
    t27 = t25 + t23;
    t23 = (t25 - t23) * E4;
    t25 = t09 + t27;
    t09 = (t09 - t27) * F1;
    t27 = t17 + t19;
    t19 = (t17 - t19) * F2;
    t17 = t25 + t27;
    t27 = (t25 - t27) * G;
    t25 = t09 + t19;
    t19 = (t09 - t19) * G;
    t25 += t19;
    t09 = t31 + t23;
    t31 = (t31 - t23) * F1;
    t23 = t29 + t21;
    t21 = (t29 - t21) * F2;
    t29 = t09 + t23;
    t23 = (t09 - t23) * G;
    t09 = t31 + t21;
    t31 = (t31 - t21) * G;
    t09 += t31;
    t29 += t09;
    t09 += t23;
    t23 += t31;
    t17 += t29;
    t29 += t25;
    t25 += t09;
    t09 += t27;
    t27 += t23;
    t23 += t19;
    t19 += t31;
    t21 = t02 + t32;
    t02 = (t02 - t32) * C1;
    t32 = t04 + t30;
    t04 = (t04 - t30) * C2;
    t30 = t06 + t28;
    t28 = (t06 - t28) * C3;
    t06 = t08 + t26;
    t08 = (t08 - t26) * C4;
    t26 = t10 + t24;
    t10 = (t10 - t24) * C5;
    t24 = t12 + t22;
    t22 = (t12 - t22) * C6;
    t12 = t14 + t20;
    t20 = (t14 - t20) * C7;
    t14 = t16 + t18;
    t16 = (t16 - t18) * C8;
    t18 = t21 + t14;
    t14 = (t21 - t14) * E1;
    t21 = t32 + t12;
    t32 = (t32 - t12) * E2;
    t12 = t30 + t24;
    t24 = (t30 - t24) * E3;
    t30 = t06 + t26;
    t26 = (t06 - t26) * E4;
    t06 = t18 + t30;
    t18 = (t18 - t30) * F1;
    t30 = t21 + t12;
    t12 = (t21 - t12) * F2;
    t21 = t06 + t30;
    t30 = (t06 - t30) * G;
    t06 = t18 + t12;
    t12 = (t18 - t12) * G;
    t06 += t12;
    t18 = t14 + t26;
    t26 = (t14 - t26) * F1;
    t14 = t32 + t24;
    t24 = (t32 - t24) * F2;
    t32 = t18 + t14;
    t14 = (t18 - t14) * G;
    t18 = t26 + t24;
    t24 = (t26 - t24) * G;
    t18 += t24;
    t32 += t18;
    t18 += t14;
    t26 = t14 + t24;
    t14 = t02 + t16;
    t02 = (t02 - t16) * E1;
    t16 = t04 + t20;
    t04 = (t04 - t20) * E2;
    t20 = t28 + t22;
    t22 = (t28 - t22) * E3;
    t28 = t08 + t10;
    t10 = (t08 - t10) * E4;
    t08 = t14 + t28;
    t14 = (t14 - t28) * F1;
    t28 = t16 + t20;
    t20 = (t16 - t20) * F2;
    t16 = t08 + t28;
    t28 = (t08 - t28) * G;
    t08 = t14 + t20;
    t20 = (t14 - t20) * G;
    t08 += t20;
    t14 = t02 + t10;
    t02 = (t02 - t10) * F1;
    t10 = t04 + t22;
    t22 = (t04 - t22) * F2;
    t04 = t14 + t10;
    t10 = (t14 - t10) * G;
    t14 = t02 + t22;
    t02 = (t02 - t22) * G;
    t14 += t02;
    t04 += t14;
    t14 += t10;
    t10 += t02;
    t16 += t04;
    t04 += t08;
    t08 += t14;
    t14 += t28;
    t28 += t10;
    t10 += t20;
    t20 += t02;
    t21 += t16;
    t16 += t32;
    t32 += t04;
    t04 += t06;
    t06 += t08;
    t08 += t18;
    t18 += t14;
    t14 += t30;
    t30 += t28;
    t28 += t26;
    t26 += t10;
    t10 += t12;
    t12 += t20;
    t20 += t24;
    t24 += t02;

    let mirrored = [
        t21, t17, t16, t01, t32, t29, t04, t03, t06, t25, t08, t11, t18, t09, t14,
    ];
    d[dp + 48] = -t33;
    for (k, &t) in mirrored.iter().enumerate() {
        d[dp + 49 + k] = -t;
        d[dp + 47 - k] = -t;
    }
    d[dp + 32] = -t05;
    let paired = [
        t05, t30, t27, t28, t07, t26, t23, t10, t15, t12, t19, t20, t13, t24, t31, t02,
    ];
    for (k, &t) in paired.iter().enumerate() {
        d[dp + k] = t;
    }
    let negated = [
        t30, t27, t28, t07, t26, t23, t10, t15, t12, t19, t20, t13, t24, t31, t02,
    ];
    for (k, &t) in negated.iter().enumerate() {
        d[dp + 31 - k] = -t;
    }
    d[dp + 16] = 0.0;
}

const C1: f32 = 0.502419286188;
const C2: f32 = 0.52249861494;
const C3: f32 = 0.566944034816;
const C4: f32 = 0.64682178336;
const C5: f32 = 0.788154623451;
const C6: f32 = 1.06067768599;
const C7: f32 = 1.72244709824;
const C8: f32 = 5.10114861869;
const E1: f32 = 0.509795579104;
const E2: f32 = 0.601344886935;
const E3: f32 = 0.899976223136;
const E4: f32 = 2.56291544774;
const F1: f32 = 0.541196100146;
const F2: f32 = 1.30656296488;
const G: f32 = core::f32::consts::FRAC_1_SQRT_2;
