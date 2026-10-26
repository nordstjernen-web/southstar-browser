//! Southstar — the MPEG-1 video decoder: sequence and picture headers, macroblocks, motion compensation and the IDCT. Ported from pl_mpeg by Dominic Szablewski (MIT).
//! Copyright 2019 Dominic Szablewski
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

use crate::buffer::Bits;
use crate::video_tables::{
    CODE_BLOCK_PATTERN, DCT_COEFF, DCT_SIZE_CHROMINANCE, DCT_SIZE_LUMINANCE, INTRA_QUANT_MATRIX,
    MACROBLOCK_ADDRESS_INCREMENT, MACROBLOCK_TYPE_B, MACROBLOCK_TYPE_INTRA,
    MACROBLOCK_TYPE_PREDICTIVE, MOTION, NON_INTRA_QUANT_MATRIX, PICTURE_RATE, PIXEL_ASPECT_RATIO,
    PREMULTIPLIER_MATRIX, ZIG_ZAG,
};

const PICTURE_TYPE_INTRA: i32 = 1;
const PICTURE_TYPE_PREDICTIVE: i32 = 2;
const PICTURE_TYPE_B: i32 = 3;

const START_SEQUENCE: i32 = 0xB3;
const START_SLICE_FIRST: i32 = 0x01;
const START_SLICE_LAST: i32 = 0xAF;
const START_PICTURE: i32 = 0x00;
const START_EXTENSION: i32 = 0xB5;
const START_USER_DATA: i32 = 0xB2;

#[derive(Clone, Copy, Default)]
struct Motion {
    full_px: bool,
    is_set: bool,
    r_size: i32,
    h: i32,
    v: i32,
}

pub struct Plane<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8],
}

pub struct Frame<'a> {
    pub time: f64,
    pub width: usize,
    pub height: usize,
    pub y: Plane<'a>,
    pub cr: Plane<'a>,
    pub cb: Plane<'a>,
}

fn clamp(n: i32) -> u8 {
    n.clamp(0, 255) as u8
}

impl Frame<'_> {
    pub fn to_bgra(&self, dest: &mut [u8], stride: usize) {
        self.convert::<4, 2, 1, 0>(dest, stride);
    }

    pub fn to_rgba(&self, dest: &mut [u8], stride: usize) {
        self.convert::<4, 0, 1, 2>(dest, stride);
    }

    pub fn to_rgb(&self, dest: &mut [u8], stride: usize) {
        self.convert::<3, 0, 1, 2>(dest, stride);
    }

    fn convert<const BPP: usize, const RI: usize, const GI: usize, const BI: usize>(
        &self,
        dest: &mut [u8],
        stride: usize,
    ) {
        let cols = self.width >> 1;
        let rows = self.height >> 1;
        if rows == 0 || cols == 0 || dest.len() < (rows * 2 - 1) * stride + cols * 2 * BPP {
            return;
        }
        let yw = self.y.width;
        let cw = self.cb.width;
        let (yp, crp, cbp) = (self.y.data, self.cr.data, self.cb.data);
        for row in 0..rows {
            for col in 0..cols {
                let c_index = row * cw + col;
                let y_index = row * 2 * yw + col * 2;
                let d_index = row * 2 * stride + col * 2 * BPP;
                let cr = crp[c_index] as i32 - 128;
                let cb = cbp[c_index] as i32 - 128;
                let r = (cr * 104597) >> 16;
                let g = (cb * 25674 + cr * 53278) >> 16;
                let b = (cb * 132201) >> 16;
                for (y_offset, d_offset) in [(0, 0), (1, BPP), (yw, stride), (yw + 1, stride + BPP)]
                {
                    let y = ((yp[y_index + y_offset] as i32 - 16) * 76309) >> 16;
                    let d = d_index + d_offset;
                    dest[d + RI] = clamp(y + r);
                    dest[d + GI] = clamp(y - g);
                    dest[d + BI] = clamp(y + b);
                }
            }
        }
    }
}

pub struct Video {
    framerate: f64,
    pixel_aspect_ratio: f64,
    time: f64,
    frames_decoded: i32,
    width: i32,
    height: i32,
    mb_width: i32,
    mb_height: i32,
    mb_size: i32,
    luma_width: i32,
    luma_height: i32,
    chroma_width: i32,
    chroma_height: i32,
    start_code: i32,
    picture_type: i32,
    motion_forward: Motion,
    motion_backward: Motion,
    has_sequence_header: bool,
    quantizer_scale: i32,
    slice_begin: bool,
    macroblock_address: i32,
    mb_row: i32,
    mb_col: i32,
    macroblock_type: i32,
    macroblock_intra: bool,
    dc_predictor: [i32; 3],
    frame_current: usize,
    frame_forward: usize,
    frame_backward: usize,
    frames: Vec<u8>,
    block_data: [i32; 64],
    intra_quant_matrix: [u8; 64],
    non_intra_quant_matrix: [u8; 64],
    has_reference_frame: bool,
    assume_no_b_frames: bool,
}

impl Video {
    pub fn new<B: Bits>(src: &mut B) -> Video {
        let mut video = Video {
            framerate: 0.0,
            pixel_aspect_ratio: 0.0,
            time: 0.0,
            frames_decoded: 0,
            width: 0,
            height: 0,
            mb_width: 0,
            mb_height: 0,
            mb_size: 0,
            luma_width: 0,
            luma_height: 0,
            chroma_width: 0,
            chroma_height: 0,
            start_code: 0,
            picture_type: 0,
            motion_forward: Motion::default(),
            motion_backward: Motion::default(),
            has_sequence_header: false,
            quantizer_scale: 0,
            slice_begin: false,
            macroblock_address: 0,
            mb_row: 0,
            mb_col: 0,
            macroblock_type: 0,
            macroblock_intra: false,
            dc_predictor: [0; 3],
            frame_current: 0,
            frame_forward: 0,
            frame_backward: 0,
            frames: Vec::new(),
            block_data: [0; 64],
            intra_quant_matrix: [0; 64],
            non_intra_quant_matrix: [0; 64],
            has_reference_frame: false,
            assume_no_b_frames: false,
        };
        video.start_code = src.find_start_code(START_SEQUENCE);
        if video.start_code != -1 {
            video.decode_sequence_header(src);
        }
        video
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn framerate(&self) -> f64 {
        self.framerate
    }

    pub fn pixel_aspect_ratio(&self) -> f64 {
        self.pixel_aspect_ratio
    }

    pub fn set_time(&mut self, time: f64) {
        self.frames_decoded = (self.framerate * time) as i32;
        self.time = time;
    }

    pub fn rewind(&mut self) {
        self.time = 0.0;
        self.frames_decoded = 0;
        self.has_reference_frame = false;
        self.start_code = -1;
    }

    pub fn frame(&self, base: usize, time: f64) -> Frame<'_> {
        let luma = (self.luma_width * self.luma_height) as usize;
        let chroma = (self.chroma_width * self.chroma_height) as usize;
        let plane = |start: usize, len: usize, w: i32, h: i32| Plane {
            width: w as usize,
            height: h as usize,
            data: &self.frames[start..start + len],
        };
        Frame {
            time,
            width: self.width as usize,
            height: self.height as usize,
            y: plane(base, luma, self.luma_width, self.luma_height),
            cr: plane(base + luma, chroma, self.chroma_width, self.chroma_height),
            cb: plane(
                base + luma + chroma,
                chroma,
                self.chroma_width,
                self.chroma_height,
            ),
        }
    }

    pub fn has_header<B: Bits>(&mut self, src: &mut B) -> bool {
        if self.has_sequence_header {
            return true;
        }
        if self.start_code != START_SEQUENCE {
            self.start_code = src.find_start_code(START_SEQUENCE);
        }
        if self.start_code == -1 {
            return false;
        }
        self.decode_sequence_header(src)
    }

    pub fn decode<B: Bits>(&mut self, src: &mut B) -> Option<(usize, f64)> {
        if !self.has_header(src) {
            return None;
        }
        let frame = loop {
            if self.start_code != START_PICTURE {
                self.start_code = src.find_start_code(START_PICTURE);
                if self.start_code == -1 {
                    if self.has_reference_frame
                        && !self.assume_no_b_frames
                        && src.buf().has_ended
                        && (self.picture_type == PICTURE_TYPE_INTRA
                            || self.picture_type == PICTURE_TYPE_PREDICTIVE)
                    {
                        self.has_reference_frame = false;
                        break self.frame_backward;
                    }
                    return None;
                }
            }
            if src.has_start_code(START_PICTURE) == -1 && !src.buf().has_ended {
                return None;
            }
            src.buf().discard_read();
            self.decode_picture(src);
            if self.assume_no_b_frames {
                break self.frame_backward;
            } else if self.picture_type == PICTURE_TYPE_B {
                break self.frame_current;
            } else if self.has_reference_frame {
                break self.frame_forward;
            }
            self.has_reference_frame = true;
        };
        let time = self.time;
        self.frames_decoded += 1;
        self.time = self.frames_decoded as f64 / self.framerate;
        Some((frame, time))
    }

    fn decode_sequence_header<B: Bits>(&mut self, src: &mut B) -> bool {
        if !src.has(64 + 2 * 64 * 8) {
            return false;
        }
        self.width = src.read(12);
        self.height = src.read(12);
        if self.width <= 0 || self.height <= 0 {
            return false;
        }
        let par_code = (src.read(4) - 1).clamp(0, PIXEL_ASPECT_RATIO.len() as i32 - 1);
        self.pixel_aspect_ratio = PIXEL_ASPECT_RATIO[par_code as usize] as f64;
        self.framerate = PICTURE_RATE[src.read(4) as usize];
        src.skip(18 + 1 + 10 + 1);
        if src.read(1) != 0 {
            for &idx in &ZIG_ZAG {
                self.intra_quant_matrix[idx as usize] = src.read(8) as u8;
            }
        } else {
            self.intra_quant_matrix = INTRA_QUANT_MATRIX;
        }
        if src.read(1) != 0 {
            for &idx in &ZIG_ZAG {
                self.non_intra_quant_matrix[idx as usize] = src.read(8) as u8;
            }
        } else {
            self.non_intra_quant_matrix = NON_INTRA_QUANT_MATRIX;
        }
        self.mb_width = (self.width + 15) >> 4;
        self.mb_height = (self.height + 15) >> 4;
        self.mb_size = self.mb_width * self.mb_height;
        self.luma_width = self.mb_width << 4;
        self.luma_height = self.mb_height << 4;
        self.chroma_width = self.mb_width << 3;
        self.chroma_height = self.mb_height << 3;
        let luma = (self.luma_width * self.luma_height) as usize;
        let chroma = (self.chroma_width * self.chroma_height) as usize;
        let frame_size = luma + 2 * chroma;
        self.frames = vec![0; frame_size * 3 + self.luma_width as usize + 1];
        self.frame_current = 0;
        self.frame_forward = frame_size;
        self.frame_backward = frame_size * 2;
        self.has_sequence_header = true;
        true
    }

    fn decode_picture<B: Bits>(&mut self, src: &mut B) {
        src.skip(10);
        self.picture_type = src.read(3);
        src.skip(16);
        if self.picture_type <= 0 || self.picture_type > PICTURE_TYPE_B {
            self.start_code = -1;
            return;
        }
        if self.picture_type == PICTURE_TYPE_PREDICTIVE || self.picture_type == PICTURE_TYPE_B {
            self.motion_forward.full_px = src.read(1) != 0;
            let f_code = src.read(3);
            if f_code == 0 {
                self.start_code = -1;
                return;
            }
            self.motion_forward.r_size = f_code - 1;
        }
        if self.picture_type == PICTURE_TYPE_B {
            self.motion_backward.full_px = src.read(1) != 0;
            let f_code = src.read(3);
            if f_code == 0 {
                self.start_code = -1;
                return;
            }
            self.motion_backward.r_size = f_code - 1;
        }
        let reference =
            self.picture_type == PICTURE_TYPE_INTRA || self.picture_type == PICTURE_TYPE_PREDICTIVE;
        let frame_temp = self.frame_forward;
        if reference {
            self.frame_forward = self.frame_backward;
        }
        loop {
            self.start_code = src.next_start_code();
            if self.start_code != START_EXTENSION && self.start_code != START_USER_DATA {
                break;
            }
        }
        while (START_SLICE_FIRST..=START_SLICE_LAST).contains(&self.start_code) {
            self.decode_slice(src, self.start_code & 0xFF);
            if self.macroblock_address >= self.mb_size - 1 {
                break;
            }
            self.start_code = src.next_start_code();
        }
        if reference {
            self.frame_backward = self.frame_current;
            self.frame_current = frame_temp;
        }
    }

    fn reset_motion(&mut self) {
        self.motion_forward.h = 0;
        self.motion_forward.v = 0;
        self.motion_backward.h = 0;
        self.motion_backward.v = 0;
    }

    fn decode_slice<B: Bits>(&mut self, src: &mut B, slice: i32) {
        self.slice_begin = true;
        self.macroblock_address = (slice - 1) * self.mb_width - 1;
        self.reset_motion();
        self.dc_predictor = [128; 3];
        self.quantizer_scale = src.read(5);
        while src.read(1) != 0 {
            src.skip(8);
        }
        loop {
            self.decode_macroblock(src);
            if !(self.macroblock_address < self.mb_size - 1 && src.peek_non_zero(23)) {
                break;
            }
        }
    }

    fn decode_macroblock<B: Bits>(&mut self, src: &mut B) {
        let mut increment = 0;
        let mut t = src.read_vlc(&MACROBLOCK_ADDRESS_INCREMENT) as i32;
        while t == 34 {
            t = src.read_vlc(&MACROBLOCK_ADDRESS_INCREMENT) as i32;
        }
        while t == 35 {
            increment += 33;
            t = src.read_vlc(&MACROBLOCK_ADDRESS_INCREMENT) as i32;
        }
        increment += t;

        if self.slice_begin {
            self.slice_begin = false;
            self.macroblock_address += increment;
        } else {
            if self.macroblock_address + increment >= self.mb_size {
                return;
            }
            if increment > 1 {
                self.dc_predictor = [128; 3];
                if self.picture_type == PICTURE_TYPE_PREDICTIVE {
                    self.motion_forward.h = 0;
                    self.motion_forward.v = 0;
                }
            }
            while increment > 1 {
                self.macroblock_address += 1;
                self.mb_row = self.macroblock_address / self.mb_width;
                self.mb_col = self.macroblock_address % self.mb_width;
                self.predict_macroblock();
                increment -= 1;
            }
            self.macroblock_address += 1;
        }

        self.mb_row = self.macroblock_address / self.mb_width;
        self.mb_col = self.macroblock_address % self.mb_width;
        if self.mb_col >= self.mb_width || self.mb_row >= self.mb_height {
            return;
        }

        let table = match self.picture_type {
            PICTURE_TYPE_INTRA => &MACROBLOCK_TYPE_INTRA[..],
            PICTURE_TYPE_PREDICTIVE => &MACROBLOCK_TYPE_PREDICTIVE[..],
            _ => &MACROBLOCK_TYPE_B[..],
        };
        self.macroblock_type = src.read_vlc(table) as i32;
        self.macroblock_intra = self.macroblock_type & 0x01 != 0;
        self.motion_forward.is_set = self.macroblock_type & 0x08 != 0;
        self.motion_backward.is_set = self.macroblock_type & 0x04 != 0;

        if self.macroblock_type & 0x10 != 0 {
            self.quantizer_scale = src.read(5);
        }

        if self.macroblock_intra {
            self.reset_motion();
        } else {
            self.dc_predictor = [128; 3];
            self.decode_motion_vectors(src);
            self.predict_macroblock();
        }

        let cbp = if self.macroblock_type & 0x02 != 0 {
            src.read_vlc(&CODE_BLOCK_PATTERN) as i32
        } else if self.macroblock_intra {
            0x3f
        } else {
            0
        };
        let mut mask = 0x20;
        for block in 0..6 {
            if cbp & mask != 0 {
                self.decode_block(src, block);
            }
            mask >>= 1;
        }
    }

    fn decode_motion_vectors<B: Bits>(&mut self, src: &mut B) {
        if self.motion_forward.is_set {
            let r_size = self.motion_forward.r_size;
            self.motion_forward.h = decode_motion_vector(src, r_size, self.motion_forward.h);
            self.motion_forward.v = decode_motion_vector(src, r_size, self.motion_forward.v);
        } else if self.picture_type == PICTURE_TYPE_PREDICTIVE {
            self.motion_forward.h = 0;
            self.motion_forward.v = 0;
        }
        if self.motion_backward.is_set {
            let r_size = self.motion_backward.r_size;
            self.motion_backward.h = decode_motion_vector(src, r_size, self.motion_backward.h);
            self.motion_backward.v = decode_motion_vector(src, r_size, self.motion_backward.v);
        }
    }

    fn predict_macroblock(&mut self) {
        let mut fw_h = self.motion_forward.h;
        let mut fw_v = self.motion_forward.v;
        if self.motion_forward.full_px {
            fw_h <<= 1;
            fw_v <<= 1;
        }
        if self.picture_type == PICTURE_TYPE_B {
            let mut bw_h = self.motion_backward.h;
            let mut bw_v = self.motion_backward.v;
            if self.motion_backward.full_px {
                bw_h <<= 1;
                bw_v <<= 1;
            }
            if self.motion_forward.is_set {
                self.copy_macroblock(self.frame_forward, fw_h, fw_v, false);
                if self.motion_backward.is_set {
                    self.copy_macroblock(self.frame_backward, bw_h, bw_v, true);
                }
            } else {
                self.copy_macroblock(self.frame_backward, bw_h, bw_v, false);
            }
        } else {
            self.copy_macroblock(self.frame_forward, fw_h, fw_v, false);
        }
    }

    fn copy_macroblock(&mut self, source: usize, motion_h: i32, motion_v: i32, interpolate: bool) {
        let luma = (self.luma_width * self.luma_height) as usize;
        let chroma = (self.chroma_width * self.chroma_height) as usize;
        let dest = self.frame_current;
        self.process_macroblock(source, dest, motion_h, motion_v, 16, interpolate);
        self.process_macroblock(
            source + luma,
            dest + luma,
            motion_h / 2,
            motion_v / 2,
            8,
            interpolate,
        );
        self.process_macroblock(
            source + luma + chroma,
            dest + luma + chroma,
            motion_h / 2,
            motion_v / 2,
            8,
            interpolate,
        );
    }

    fn process_macroblock(
        &mut self,
        s: usize,
        d: usize,
        motion_h: i32,
        motion_v: i32,
        block_size: i32,
        interpolate: bool,
    ) {
        let dw = self.mb_width * block_size;
        let hp = motion_h >> 1;
        let vp = motion_v >> 1;
        let odd_h = (motion_h & 1) == 1;
        let odd_v = (motion_v & 1) == 1;
        let si = (((self.mb_row * block_size) + vp) * dw + (self.mb_col * block_size) + hp) as u32;
        let di = ((self.mb_row * dw + self.mb_col) * block_size) as u32;
        let max_address = (dw * (self.mb_height * block_size - block_size + 1) - block_size) as u32;
        if si > max_address || di > max_address {
            return;
        }
        let dw = dw as usize;
        let bs = block_size as usize;
        let scan = dw - bs;
        let mut si = s + si as usize;
        let mut di = d + di as usize;
        let f = &mut self.frames;
        for _ in 0..bs {
            for _ in 0..bs {
                let p = match (odd_h, odd_v) {
                    (false, false) => f[si] as u32,
                    (false, true) => (f[si] as u32 + f[si + dw] as u32 + 1) >> 1,
                    (true, false) => (f[si] as u32 + f[si + 1] as u32 + 1) >> 1,
                    (true, true) => {
                        (f[si] as u32
                            + f[si + 1] as u32
                            + f[si + dw] as u32
                            + f[si + dw + 1] as u32
                            + 2)
                            >> 2
                    }
                };
                f[di] = if interpolate {
                    ((f[di] as u32 + p + 1) >> 1) as u8
                } else {
                    p as u8
                };
                si += 1;
                di += 1;
            }
            si += scan;
            di += scan;
        }
    }

    fn decode_block<B: Bits>(&mut self, src: &mut B, block: i32) {
        let mut n: i32 = 0;
        let intra = self.macroblock_intra;
        if intra {
            let plane_index = (if block > 3 { block - 3 } else { 0 }) as usize;
            let predictor = self.dc_predictor[plane_index];
            let table = if plane_index == 0 {
                &DCT_SIZE_LUMINANCE
            } else {
                &DCT_SIZE_CHROMINANCE
            };
            let dct_size = src.read_vlc(table) as i32;
            self.block_data[0] = if dct_size > 0 {
                let differential = src.read(dct_size as u32);
                if differential & (1 << (dct_size - 1)) != 0 {
                    predictor + differential
                } else {
                    predictor + (-(1 << dct_size) | (differential + 1))
                }
            } else {
                predictor
            };
            self.dc_predictor[plane_index] = self.block_data[0];
            self.block_data[0] <<= 3 + 5;
            n = 1;
        }

        loop {
            let run;
            let mut level;
            let coeff = src.read_vlc(&DCT_COEFF) as u16;
            if coeff == 0x0001 && n > 0 && src.read(1) == 0 {
                break;
            }
            if coeff == 0xffff {
                run = src.read(6);
                level = src.read(8);
                if level == 0 {
                    level = src.read(8);
                } else if level == 128 {
                    level = src.read(8) - 256;
                } else if level > 128 {
                    level -= 256;
                }
            } else {
                run = (coeff >> 8) as i32;
                level = (coeff & 0xff) as i32;
                if src.read(1) != 0 {
                    level = -level;
                }
            }
            n += run;
            if !(0..64).contains(&n) {
                return;
            }
            let de_zig_zagged = ZIG_ZAG[n as usize] as usize;
            n += 1;
            level <<= 1;
            if !intra {
                level += if level < 0 { -1 } else { 1 };
            }
            let quant = if intra {
                self.intra_quant_matrix[de_zig_zagged]
            } else {
                self.non_intra_quant_matrix[de_zig_zagged]
            };
            level = (level * self.quantizer_scale * quant as i32) >> 4;
            if (level & 1) == 0 {
                level -= if level > 0 { 1 } else { -1 };
            }
            level = level.clamp(-2048, 2047);
            self.block_data[de_zig_zagged] = level * PREMULTIPLIER_MATRIX[de_zig_zagged] as i32;
        }

        let luma = (self.luma_width * self.luma_height) as usize;
        let chroma = (self.chroma_width * self.chroma_height) as usize;
        let (plane, dw, di) = if block < 4 {
            let mut di = (self.mb_row * self.luma_width + self.mb_col) << 4;
            if (block & 1) != 0 {
                di += 8;
            }
            if (block & 2) != 0 {
                di += self.luma_width << 3;
            }
            (self.frame_current, self.luma_width, di)
        } else {
            let plane = if block == 4 {
                self.frame_current + luma + chroma
            } else {
                self.frame_current + luma
            };
            let di = ((self.mb_row * self.luma_width) << 2) + (self.mb_col << 3);
            (plane, self.chroma_width, di)
        };

        let dc_only = n == 1;
        if !dc_only {
            idct(&mut self.block_data);
        }
        let dw = dw as usize;
        let fits = di >= 0 && plane + di as usize + 7 * dw + 8 <= self.frames.len();
        if fits {
            let s = &self.block_data;
            let d = &mut self.frames;
            let mut di = plane + di as usize;
            let dc = (s[0] + 128) >> 8;
            let mut si = 0;
            for _ in 0..8 {
                for _ in 0..8 {
                    let value = if dc_only { dc } else { s[si] };
                    d[di] = if intra {
                        clamp(value)
                    } else {
                        clamp(d[di] as i32 + value)
                    };
                    si += 1;
                    di += 1;
                }
                di += dw - 8;
            }
        }
        if dc_only {
            self.block_data[0] = 0;
        } else {
            self.block_data = [0; 64];
        }
    }
}

fn decode_motion_vector<B: Bits>(src: &mut B, r_size: i32, motion: i32) -> i32 {
    let fscale = 1 << r_size;
    let m_code = src.read_vlc(&MOTION) as i32;
    let d = if m_code != 0 && fscale != 1 {
        let r = src.read(r_size as u32);
        let d = ((m_code.abs() - 1) << r_size) + r + 1;
        if m_code < 0 { -d } else { d }
    } else {
        m_code
    };
    let mut motion = motion + d;
    if motion > (fscale << 4) - 1 {
        motion -= fscale << 5;
    } else if motion < (-fscale) << 4 {
        motion += fscale << 5;
    }
    motion
}

fn idct(block: &mut [i32; 64]) {
    for i in 0..8 {
        let b1 = block[4 * 8 + i];
        let b3 = block[2 * 8 + i] + block[6 * 8 + i];
        let b4 = block[5 * 8 + i] - block[3 * 8 + i];
        let tmp1 = block[8 + i] + block[7 * 8 + i];
        let tmp2 = block[3 * 8 + i] + block[5 * 8 + i];
        let b6 = block[8 + i] - block[7 * 8 + i];
        let b7 = tmp1 + tmp2;
        let m0 = block[i];
        let x4 = ((b6 * 473 - b4 * 196 + 128) >> 8) - b7;
        let x0 = x4 - (((tmp1 - tmp2) * 362 + 128) >> 8);
        let x1 = m0 - b1;
        let x2 = (((block[2 * 8 + i] - block[6 * 8 + i]) * 362 + 128) >> 8) - b3;
        let x3 = m0 + b1;
        let y3 = x1 + x2;
        let y4 = x3 + b3;
        let y5 = x1 - x2;
        let y6 = x3 - b3;
        let y7 = -x0 - ((b4 * 473 + b6 * 196 + 128) >> 8);
        block[i] = b7 + y4;
        block[8 + i] = x4 + y3;
        block[2 * 8 + i] = y5 - x0;
        block[3 * 8 + i] = y6 - y7;
        block[4 * 8 + i] = y6 + y7;
        block[5 * 8 + i] = x0 + y5;
        block[6 * 8 + i] = y3 - x4;
        block[7 * 8 + i] = y4 - b7;
    }
    for i in (0..64).step_by(8) {
        let b1 = block[4 + i];
        let b3 = block[2 + i] + block[6 + i];
        let b4 = block[5 + i] - block[3 + i];
        let tmp1 = block[1 + i] + block[7 + i];
        let tmp2 = block[3 + i] + block[5 + i];
        let b6 = block[1 + i] - block[7 + i];
        let b7 = tmp1 + tmp2;
        let m0 = block[i];
        let x4 = ((b6 * 473 - b4 * 196 + 128) >> 8) - b7;
        let x0 = x4 - (((tmp1 - tmp2) * 362 + 128) >> 8);
        let x1 = m0 - b1;
        let x2 = (((block[2 + i] - block[6 + i]) * 362 + 128) >> 8) - b3;
        let x3 = m0 + b1;
        let y3 = x1 + x2;
        let y4 = x3 + b3;
        let y5 = x1 - x2;
        let y6 = x3 - b3;
        let y7 = -x0 - ((b4 * 473 + b6 * 196 + 128) >> 8);
        block[i] = (b7 + y4 + 128) >> 8;
        block[1 + i] = (x4 + y3 + 128) >> 8;
        block[2 + i] = (y5 - x0 + 128) >> 8;
        block[3 + i] = (y6 - y7 + 128) >> 8;
        block[4 + i] = (y6 + y7 + 128) >> 8;
        block[5 + i] = (x0 + y5 + 128) >> 8;
        block[6 + i] = (y3 - x4 + 128) >> 8;
        block[7 + i] = (y4 - b7 + 128) >> 8;
    }
}
