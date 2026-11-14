//! Southstar — a baseline JPEG encoder for canvas export: 4:4:4 YCbCr, the standard quantization and Huffman tables.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f32::consts::PI;

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

const LUMA_QUANT: [u8; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

const CHROMA_QUANT: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
const AC_LUMA_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
    0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_CHROMA_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
    0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
    0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
    0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
    0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

const MAX_SIDE: usize = 65535;

struct Codes {
    code: [u16; 256],
    length: [u8; 256],
}

impl Codes {
    fn new(bits: &[u8; 16], values: &[u8]) -> Codes {
        let mut codes = Codes {
            code: [0; 256],
            length: [0; 256],
        };
        let mut next = 0u16;
        let mut index = 0;
        for (length, &count) in (1u8..).zip(bits) {
            for _ in 0..count {
                let symbol = usize::from(values[index]);
                codes.code[symbol] = next;
                codes.length[symbol] = length;
                next += 1;
                index += 1;
            }
            next <<= 1;
        }
        codes
    }
}

struct Bits {
    out: Vec<u8>,
    pending: u32,
    count: u32,
}

impl Bits {
    fn put(&mut self, value: u32, length: u32) {
        self.pending = (self.pending << length) | (value & ((1 << length) - 1));
        self.count += length;
        while self.count >= 8 {
            let byte = (self.pending >> (self.count - 8)) as u8;
            self.out.push(byte);
            if byte == 0xff {
                self.out.push(0);
            }
            self.count -= 8;
        }
    }

    fn symbol(&mut self, codes: &Codes, symbol: usize) {
        self.put(
            u32::from(codes.code[symbol]),
            u32::from(codes.length[symbol]),
        );
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.put(0x7f, 8 - self.count);
        }
        self.out
    }
}

fn magnitude(value: i32) -> (u32, u32) {
    let size = 32 - value.unsigned_abs().leading_zeros();
    let bits = if value < 0 { value - 1 } else { value };
    (size, bits as u32 & ((1u32 << size) - 1))
}

fn scaled_table(base: &[u8; 64], quality: u8) -> [u8; 64] {
    let quality = i32::from(quality.clamp(1, 100));
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - 2 * quality
    };
    base.map(|entry| ((i32::from(entry) * scale + 50) / 100).clamp(1, 255) as u8)
}

fn cosines() -> [[f32; 8]; 8] {
    let mut table = [[0.0; 8]; 8];
    for (frequency, row) in table.iter_mut().enumerate() {
        let norm = if frequency == 0 {
            0.5 / 2f32.sqrt()
        } else {
            0.5
        };
        for (sample, entry) in row.iter_mut().enumerate() {
            *entry = norm * ((2 * sample + 1) as f32 * frequency as f32 * PI / 16.0).cos();
        }
    }
    table
}

fn forward_dct(block: &mut [f32; 64], cosines: &[[f32; 8]; 8]) {
    let mut rows = [0.0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            rows[y * 8 + u] = (0..8).map(|x| block[y * 8 + x] * cosines[u][x]).sum();
        }
    }
    for u in 0..8 {
        for v in 0..8 {
            block[v * 8 + u] = (0..8).map(|y| rows[y * 8 + u] * cosines[v][y]).sum();
        }
    }
}

struct Component<'a> {
    quant: &'a [u8; 64],
    dc: &'a Codes,
    ac: &'a Codes,
    previous_dc: i32,
}

fn encode_block(bits: &mut Bits, block: &[f32; 64], component: &mut Component<'_>) {
    let mut quantized = [0i32; 64];
    for (slot, &natural) in quantized.iter_mut().zip(&ZIGZAG) {
        *slot = (block[natural] / f32::from(component.quant[natural])).round() as i32;
    }
    let (size, extra) = magnitude(quantized[0] - component.previous_dc);
    component.previous_dc = quantized[0];
    bits.symbol(component.dc, size as usize);
    bits.put(extra, size);
    let mut run = 0;
    for &level in &quantized[1..] {
        if level == 0 {
            run += 1;
            continue;
        }
        while run > 15 {
            bits.symbol(component.ac, 0xf0);
            run -= 16;
        }
        let (size, extra) = magnitude(level);
        bits.symbol(component.ac, (run << 4) | size as usize);
        bits.put(extra, size);
        run = 0;
    }
    if run > 0 {
        bits.symbol(component.ac, 0);
    }
}

fn segment(out: &mut Vec<u8>, marker: u8, body: &[u8]) {
    out.extend_from_slice(&[0xff, marker]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(body);
}

fn quant_segment(out: &mut Vec<u8>, id: u8, table: &[u8; 64]) {
    let mut body = vec![id];
    body.extend(ZIGZAG.iter().map(|&natural| table[natural]));
    segment(out, 0xdb, &body);
}

fn huffman_segment(out: &mut Vec<u8>, id: u8, bits: &[u8; 16], values: &[u8]) {
    let mut body = vec![id];
    body.extend_from_slice(bits);
    body.extend_from_slice(values);
    segment(out, 0xc4, &body);
}

fn headers(width: usize, height: usize, luma: &[u8; 64], chroma: &[u8; 64]) -> Vec<u8> {
    let mut out = vec![0xff, 0xd8];
    segment(
        &mut out,
        0xe0,
        &[b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
    );
    quant_segment(&mut out, 0, luma);
    quant_segment(&mut out, 1, chroma);
    let (h, w) = ((height as u16).to_be_bytes(), (width as u16).to_be_bytes());
    segment(
        &mut out,
        0xc0,
        &[
            8, h[0], h[1], w[0], w[1], 3, 1, 0x11, 0, 2, 0x11, 1, 3, 0x11, 1,
        ],
    );
    huffman_segment(&mut out, 0x00, &DC_LUMA_BITS, &DC_VALUES);
    huffman_segment(&mut out, 0x10, &AC_LUMA_BITS, &AC_LUMA_VALUES);
    huffman_segment(&mut out, 0x01, &DC_CHROMA_BITS, &DC_VALUES);
    huffman_segment(&mut out, 0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALUES);
    segment(&mut out, 0xda, &[3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);
    out
}

fn fill_blocks(
    bgra: &[u8],
    stride: usize,
    (width, height): (usize, usize),
    (block_x, block_y): (usize, usize),
    planes: &mut [[f32; 64]; 3],
) {
    for y in 0..8 {
        let row = (block_y + y).min(height - 1) * stride;
        for x in 0..8 {
            let pixel = row + (block_x + x).min(width - 1) * 4;
            let (b, g, r) = (
                f32::from(bgra[pixel]),
                f32::from(bgra[pixel + 1]),
                f32::from(bgra[pixel + 2]),
            );
            let at = y * 8 + x;
            planes[0][at] = 0.299 * r + 0.587 * g + 0.114 * b - 128.0;
            planes[1][at] = -0.168_736 * r - 0.331_264 * g + 0.5 * b;
            planes[2][at] = 0.5 * r - 0.418_688 * g - 0.081_312 * b;
        }
    }
}

pub fn encode_jpeg(
    bgra: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    quality: u8,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || stride < width * 4 {
        return None;
    }
    if bgra.len() < stride * (height - 1) + width * 4 {
        return None;
    }
    let luma = scaled_table(&LUMA_QUANT, quality);
    let chroma = scaled_table(&CHROMA_QUANT, quality);
    let dc_luma = Codes::new(&DC_LUMA_BITS, &DC_VALUES);
    let ac_luma = Codes::new(&AC_LUMA_BITS, &AC_LUMA_VALUES);
    let dc_chroma = Codes::new(&DC_CHROMA_BITS, &DC_VALUES);
    let ac_chroma = Codes::new(&AC_CHROMA_BITS, &AC_CHROMA_VALUES);
    let mut components = [
        Component {
            quant: &luma,
            dc: &dc_luma,
            ac: &ac_luma,
            previous_dc: 0,
        },
        Component {
            quant: &chroma,
            dc: &dc_chroma,
            ac: &ac_chroma,
            previous_dc: 0,
        },
        Component {
            quant: &chroma,
            dc: &dc_chroma,
            ac: &ac_chroma,
            previous_dc: 0,
        },
    ];
    let cosines = cosines();
    let mut bits = Bits {
        out: headers(width, height, &luma, &chroma),
        pending: 0,
        count: 0,
    };
    let mut planes = [[0.0f32; 64]; 3];
    for block_y in (0..height).step_by(8) {
        for block_x in (0..width).step_by(8) {
            fill_blocks(
                bgra,
                stride,
                (width, height),
                (block_x, block_y),
                &mut planes,
            );
            for (plane, component) in planes.iter_mut().zip(&mut components) {
                forward_dct(plane, &cosines);
                encode_block(&mut bits, plane, component);
            }
        }
    }
    let mut out = bits.finish();
    out.extend_from_slice(&[0xff, 0xd9]);
    Some(out)
}
