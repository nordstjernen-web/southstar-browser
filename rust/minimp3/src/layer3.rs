//! Southstar — Layer III: side information, scalefactors, Huffman decoding, stereo processing and the IMDCT. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::bits::BitReader;
use crate::header;
use crate::tables::{
    AA, EXPFRAC, LINBITS, MDCT_WINDOW, MOD, PAN, POW43, PREAMP, SCF_LONG, SCF_MIXED,
    SCF_PARTITIONS, SCF_SHORT, SCFC_DECODE, TAB32, TAB33, TABINDEX, TABS, TWID3, TWID9,
};

pub const MAX_BITRESERVOIR_BYTES: usize = 511;
pub const MAX_L3_FRAME_PAYLOAD_BYTES: usize = 2304;
const SHORT_BLOCK_TYPE: u8 = 2;
const STOP_BLOCK_TYPE: u8 = 3;
const MAX_SCFI: i32 = 44;

#[derive(Clone, Copy)]
pub struct GrInfo {
    sfbtab: &'static [u8],
    part_23_length: u16,
    big_values: u16,
    scalefac_compress: u16,
    global_gain: u8,
    block_type: u8,
    mixed_block_flag: u8,
    n_long_sfb: u8,
    n_short_sfb: u8,
    table_select: [u8; 3],
    region_count: [u8; 3],
    subblock_gain: [u8; 3],
    preflag: u8,
    scalefac_scale: u8,
    count1_table: u8,
    scfsi: u8,
}

impl Default for GrInfo {
    fn default() -> GrInfo {
        GrInfo {
            sfbtab: &SCF_LONG[0],
            part_23_length: 0,
            big_values: 0,
            scalefac_compress: 0,
            global_gain: 0,
            block_type: 0,
            mixed_block_flag: 0,
            n_long_sfb: 0,
            n_short_sfb: 0,
            table_select: [0; 3],
            region_count: [0; 3],
            subblock_gain: [0; 3],
            preflag: 0,
            scalefac_scale: 0,
            count1_table: 0,
            scfsi: 0,
        }
    }
}

pub const MAINDATA_BYTES: usize = MAX_BITRESERVOIR_BYTES + MAX_L3_FRAME_PAYLOAD_BYTES;

pub const GRBUF: usize = 2 * 576;
const WORK: usize = GRBUF + 40;

pub struct Scratch {
    pub gr_info: [GrInfo; 4],
    pub work: [f32; WORK],
    pub syn: [f32; 33 * 64],
    pub ist_pos: [[u8; 39]; 2],
}

impl Scratch {
    pub fn new() -> Scratch {
        Scratch {
            gr_info: [GrInfo::default(); 4],
            work: [0.0; WORK],
            syn: [0.0; 33 * 64],
            ist_pos: [[0; 39]; 2],
        }
    }
}

pub struct Reservoir<'a> {
    pub reserv: &'a mut i32,
    pub reserv_buf: &'a mut [u8; MAX_BITRESERVOIR_BYTES],
}

pub fn read_side_info(bs: &mut BitReader, gr: &mut [GrInfo; 4], hdr: &[u8]) -> i32 {
    let mut scfsi: u32 = 0;
    let main_data_begin: i32;
    let mut part_23_sum: i32 = 0;
    let mut sr_idx = header::my_sample_rate(hdr) as usize;
    sr_idx -= (sr_idx != 0) as usize;
    let mut gr_count: i32 = if header::is_mono(hdr) { 1 } else { 2 };
    let mpeg1 = header::test_mpeg1(hdr);

    if mpeg1 {
        gr_count *= 2;
        main_data_begin = bs.get_bits(9) as i32;
        scfsi = bs.get_bits(7 + gr_count);
    } else {
        main_data_begin = (bs.get_bits(8 + gr_count) >> gr_count) as i32;
    }

    for g in gr.iter_mut().take(gr_count as usize) {
        if header::is_mono(hdr) {
            scfsi <<= 4;
        }
        g.part_23_length = bs.get_bits(12) as u16;
        part_23_sum += g.part_23_length as i32;
        g.big_values = bs.get_bits(9) as u16;
        if g.big_values > 288 {
            return -1;
        }
        g.global_gain = bs.get_bits(8) as u8;
        g.scalefac_compress = bs.get_bits(if mpeg1 { 4 } else { 9 }) as u16;
        g.sfbtab = &SCF_LONG[sr_idx];
        g.n_long_sfb = 22;
        g.n_short_sfb = 0;
        let mut tables: u32;
        if bs.get_bits(1) != 0 {
            g.block_type = bs.get_bits(2) as u8;
            if g.block_type == 0 {
                return -1;
            }
            g.mixed_block_flag = bs.get_bits(1) as u8;
            g.region_count[0] = 7;
            g.region_count[1] = 255;
            if g.block_type == SHORT_BLOCK_TYPE {
                scfsi &= 0x0F0F;
                if g.mixed_block_flag == 0 {
                    g.region_count[0] = 8;
                    g.sfbtab = &SCF_SHORT[sr_idx];
                    g.n_long_sfb = 0;
                    g.n_short_sfb = 39;
                } else {
                    g.sfbtab = &SCF_MIXED[sr_idx];
                    g.n_long_sfb = if mpeg1 { 8 } else { 6 };
                    g.n_short_sfb = 30;
                }
            }
            tables = bs.get_bits(10);
            tables <<= 5;
            g.subblock_gain[0] = bs.get_bits(3) as u8;
            g.subblock_gain[1] = bs.get_bits(3) as u8;
            g.subblock_gain[2] = bs.get_bits(3) as u8;
        } else {
            g.block_type = 0;
            g.mixed_block_flag = 0;
            tables = bs.get_bits(15);
            g.region_count[0] = bs.get_bits(4) as u8;
            g.region_count[1] = bs.get_bits(3) as u8;
            g.region_count[2] = 255;
        }
        g.table_select[0] = (tables >> 10) as u8;
        g.table_select[1] = ((tables >> 5) & 31) as u8;
        g.table_select[2] = (tables & 31) as u8;
        g.preflag = if mpeg1 {
            bs.get_bits(1) as u8
        } else {
            (g.scalefac_compress >= 500) as u8
        };
        g.scalefac_scale = bs.get_bits(1) as u8;
        g.count1_table = bs.get_bits(1) as u8;
        g.scfsi = ((scfsi >> 12) & 15) as u8;
        scfsi <<= 4;
    }

    if part_23_sum + bs.pos > bs.limit + main_data_begin * 8 {
        return -1;
    }
    main_data_begin
}

fn read_scalefactors(
    iscf: &mut [u8; 40],
    ist_pos: &mut [u8; 39],
    scf_size: &[u8; 4],
    scf_count: &[u8],
    bs: &mut BitReader,
    mut scfsi: i32,
) {
    let mut off = 0usize;
    let mut i = 0;
    while i < 4 && scf_count[i] != 0 {
        let cnt = scf_count[i] as usize;
        if scfsi & 8 != 0 {
            iscf[off..off + cnt].copy_from_slice(&ist_pos[off..off + cnt]);
        } else {
            let bits = scf_size[i] as i32;
            if bits == 0 {
                iscf[off..off + cnt].fill(0);
                ist_pos[off..off + cnt].fill(0);
            } else {
                let max_scf = if scfsi < 0 { (1 << bits) - 1 } else { -1 };
                for k in 0..cnt {
                    let s = bs.get_bits(bits) as i32;
                    ist_pos[off + k] = if s == max_scf { 255 } else { s as u8 };
                    iscf[off + k] = s as u8;
                }
            }
        }
        off += cnt;
        i += 1;
        scfsi = scfsi.wrapping_mul(2);
    }
    iscf[off] = 0;
    iscf[off + 1] = 0;
    iscf[off + 2] = 0;
}

pub fn ldexp_q2(mut y: f32, mut exp_q2: i32) -> f32 {
    loop {
        let e = exp_q2.min(30 * 4);
        y *= EXPFRAC[(e & 3) as usize] * (1i32 << 30).wrapping_shr((e >> 2) as u32) as f32;
        exp_q2 -= e;
        if exp_q2 <= 0 {
            break;
        }
    }
    y
}

fn decode_scalefactors(
    hdr: &[u8],
    ist_pos: &mut [u8; 39],
    bs: &mut BitReader,
    gr: &GrInfo,
    scf: &mut [f32; 40],
    ch: usize,
) {
    let row = (gr.n_short_sfb != 0) as usize + (gr.n_long_sfb == 0) as usize;
    let mut partition_offset = 0usize;
    let mut scf_size = [0u8; 4];
    let mut iscf = [0u8; 40];
    let scf_shift = gr.scalefac_scale as i32 + 1;
    let mut scfsi = gr.scfsi as i32;

    if header::test_mpeg1(hdr) {
        let part = SCFC_DECODE[gr.scalefac_compress as usize & 15];
        scf_size[0] = part >> 2;
        scf_size[1] = part >> 2;
        scf_size[2] = part & 3;
        scf_size[3] = part & 3;
    } else {
        let ist = (header::test_i_stereo(hdr) && ch != 0) as usize;
        let mut sfc = (gr.scalefac_compress >> ist) as i32;
        let mut k = ist * 3 * 4;
        while sfc >= 0 {
            let mut modprod = 1i32;
            for i in (0..4).rev() {
                scf_size[i] = (sfc / modprod % MOD[k + i] as i32) as u8;
                modprod *= MOD[k + i] as i32;
            }
            sfc -= modprod;
            k += 4;
        }
        partition_offset = k;
        scfsi = -16;
    }
    read_scalefactors(
        &mut iscf,
        ist_pos,
        &scf_size,
        &SCF_PARTITIONS[row][partition_offset..],
        bs,
        scfsi,
    );

    let n_long = gr.n_long_sfb as usize;
    if gr.n_short_sfb != 0 {
        let sh = 3 - scf_shift;
        let mut i = 0usize;
        while i < gr.n_short_sfb as usize && n_long + i + 2 < iscf.len() {
            iscf[n_long + i] = iscf[n_long + i].wrapping_add(gr.subblock_gain[0] << sh);
            iscf[n_long + i + 1] = iscf[n_long + i + 1].wrapping_add(gr.subblock_gain[1] << sh);
            iscf[n_long + i + 2] = iscf[n_long + i + 2].wrapping_add(gr.subblock_gain[2] << sh);
            i += 3;
        }
    } else if gr.preflag != 0 {
        for (i, &p) in PREAMP.iter().enumerate() {
            iscf[11 + i] = iscf[11 + i].wrapping_add(p);
        }
    }

    let gain_exp = gr.global_gain as i32 - 4 - 210 - if header::is_ms_stereo(hdr) { 2 } else { 0 };
    let gain = ldexp_q2((1i32 << (MAX_SCFI / 4)) as f32, MAX_SCFI - gain_exp);
    for i in 0..(gr.n_long_sfb as usize + gr.n_short_sfb as usize) {
        scf[i] = ldexp_q2(gain, (iscf[i] as i32) << scf_shift);
    }
}

fn pow_43(mut x: i32) -> f32 {
    let mut mult = 256;
    if x < 129 {
        return POW43[(16 + x) as usize];
    }
    if x < 1024 {
        mult = 16;
        x <<= 3;
    }
    let sign = (2 * x) & 64;
    let frac = ((x & 63) - sign) as f32 / ((x & !63) + sign) as f32;
    POW43[(16 + ((x + sign) >> 6)) as usize]
        * (1.0 + frac * ((4.0f32 / 3.0) + frac * (2.0f32 / 9.0)))
        * mult as f32
}

struct HuffReader<'a> {
    buf: &'a [u8],
    next: usize,
    cache: u32,
    sh: i32,
}

impl HuffReader<'_> {
    fn byte(&self, index: usize) -> u32 {
        self.buf.get(index).copied().unwrap_or(0) as u32
    }

    fn peek(&self, n: i32) -> u32 {
        self.cache.checked_shr((32 - n) as u32).unwrap_or(0)
    }

    fn flush(&mut self, n: i32) {
        self.cache = self.cache.wrapping_shl(n as u32);
        self.sh += n;
    }

    fn check(&mut self) {
        while self.sh >= 0 {
            self.cache |= self.byte(self.next).wrapping_shl(self.sh as u32);
            self.next += 1;
            self.sh -= 8;
        }
    }

    fn pos(&self) -> i32 {
        self.next as i32 * 8 - 24 + self.sh
    }

    fn negative(&self) -> bool {
        (self.cache as i32) < 0
    }
}

fn store(dst: &mut [f32], index: usize, value: f32) {
    if let Some(slot) = dst.get_mut(index) {
        *slot = value;
    }
}

fn huffman(dst: &mut [f32], bs: &mut BitReader, gr: &GrInfo, scf: &[f32; 40], layer3gr_limit: i32) {
    let mut one = 0.0f32;
    let mut ireg = 0usize;
    let mut big_val_cnt = gr.big_values as i32;
    let sfb = gr.sfbtab;
    let sfb_at = |i: usize| sfb.get(i).copied().unwrap_or(0) as i32;
    let scf_at = |i: usize| scf.get(i).copied().unwrap_or(0.0);
    let mut sfb_i = 0usize;
    let mut scf_i = 0usize;
    let start = (bs.pos / 8) as usize;
    let mut r = HuffReader {
        buf: bs.buf,
        next: start,
        cache: 0,
        sh: (bs.pos & 7) - 8,
    };
    r.cache = ((((r.byte(start) * 256 + r.byte(start + 1)) * 256 + r.byte(start + 2)) * 256)
        .wrapping_add(r.byte(start + 3)))
    .wrapping_shl((bs.pos & 7) as u32);
    r.next += 4;
    let mut d = 0usize;

    'big: while big_val_cnt > 0 {
        let tab_num = gr.table_select[ireg.min(2)] as usize;
        let mut sfb_cnt = gr.region_count[ireg.min(2)] as i32;
        ireg += 1;
        let codebook = TABINDEX[tab_num] as i32;
        let linbits = LINBITS[tab_num] as i32;
        loop {
            let np = sfb_at(sfb_i) / 2;
            sfb_i += 1;
            if np == 0 {
                break 'big;
            }
            let mut pairs_to_decode = big_val_cnt.min(np);
            one = scf_at(scf_i);
            scf_i += 1;
            loop {
                let mut w = 5;
                let mut leaf = TABS[(codebook + r.peek(w) as i32) as usize] as i32;
                while leaf < 0 {
                    r.flush(w);
                    w = leaf & 7;
                    leaf = TABS[(codebook + r.peek(w) as i32 - (leaf >> 3)) as usize] as i32;
                }
                r.flush(leaf >> 8);
                for _ in 0..2 {
                    let mut lsb = leaf & 0x0F;
                    if linbits != 0 && lsb == 15 {
                        lsb += r.peek(linbits) as i32;
                        r.flush(linbits);
                        r.check();
                        let sign = if r.negative() { -1.0 } else { 1.0 };
                        store(dst, d, one * pow_43(lsb) * sign);
                    } else {
                        let index = 16 + lsb - 16 * (r.cache >> 31) as i32;
                        store(dst, d, POW43[index as usize] * one);
                    }
                    r.flush(if lsb != 0 { 1 } else { 0 });
                    d += 1;
                    leaf >>= 4;
                }
                r.check();
                pairs_to_decode -= 1;
                if pairs_to_decode == 0 {
                    break;
                }
            }
            big_val_cnt -= np;
            if big_val_cnt <= 0 {
                break;
            }
            sfb_cnt -= 1;
            if sfb_cnt < 0 {
                break;
            }
        }
    }

    let mut np = 1 - big_val_cnt;
    let codebook_count1: &[u8] = if gr.count1_table != 0 { &TAB33 } else { &TAB32 };
    'count1: loop {
        let mut leaf = codebook_count1[r.peek(4) as usize] as i32;
        if leaf & 8 == 0 {
            let extra = (r.cache << 4)
                .checked_shr((32 - (leaf & 3)) as u32)
                .unwrap_or(0);
            leaf = codebook_count1[((leaf >> 3) + extra as i32) as usize] as i32;
        }
        r.flush(leaf & 7);
        if r.pos() > layer3gr_limit {
            break;
        }
        for half in 0..2 {
            np -= 1;
            if np == 0 {
                np = sfb_at(sfb_i) / 2;
                sfb_i += 1;
                if np == 0 {
                    break 'count1;
                }
                one = scf_at(scf_i);
                scf_i += 1;
            }
            for s in 2 * half..2 * half + 2 {
                if leaf & (128 >> s) != 0 {
                    store(dst, d + s, if r.negative() { -one } else { one });
                    r.flush(1);
                }
            }
        }
        r.check();
        d += 4;
    }

    bs.pos = layer3gr_limit;
}

fn midside_stereo(grbuf: &mut [f32], left: usize, n: usize) {
    for i in 0..n {
        let a = grbuf[left + i];
        let b = grbuf[left + 576 + i];
        grbuf[left + i] = a + b;
        grbuf[left + 576 + i] = a - b;
    }
}

fn intensity_stereo_band(grbuf: &mut [f32], left: usize, n: usize, kl: f32, kr: f32) {
    for i in left..left + n {
        grbuf[i + 576] = grbuf[i] * kr;
        grbuf[i] *= kl;
    }
}

fn stereo_top_band(grbuf: &[f32], mut right: usize, sfb: &[u8], nbands: usize) -> [i32; 3] {
    let mut max_band = [-1i32; 3];
    for i in 0..nbands {
        let len = sfb[i] as usize;
        let mut k = 0;
        while k < len {
            if grbuf[right + k] != 0.0 || grbuf[right + k + 1] != 0.0 {
                max_band[i % 3] = i as i32;
                break;
            }
            k += 2;
        }
        right += len;
    }
    max_band
}

fn stereo_process(
    grbuf: &mut [f32],
    ist_pos: &[u8; 39],
    sfb: &[u8],
    hdr: &[u8],
    max_band: &[i32; 3],
    mpeg2_sh: u32,
) {
    let mpeg1 = header::test_mpeg1(hdr);
    let max_pos: u32 = if mpeg1 { 7 } else { 64 };
    let mut left = 0usize;
    let mut i = 0usize;
    while i < sfb.len() && sfb[i] != 0 {
        let len = sfb[i] as usize;
        let ipos = ist_pos.get(i).copied().unwrap_or(0) as u32;
        if i as i32 > max_band[i % 3] && ipos < max_pos {
            let s = if header::test_ms_stereo(hdr) {
                1.414_213_56_f32
            } else {
                1.0
            };
            let (kl, kr);
            if mpeg1 {
                kl = PAN[2 * ipos as usize];
                kr = PAN[2 * ipos as usize + 1];
            } else {
                let k = ldexp_q2(1.0, (((ipos + 1) >> 1) << mpeg2_sh) as i32);
                if ipos & 1 != 0 {
                    kl = k;
                    kr = 1.0;
                } else {
                    kl = 1.0;
                    kr = k;
                }
            }
            intensity_stereo_band(grbuf, left, len, kl * s, kr * s);
        } else if header::test_ms_stereo(hdr) {
            midside_stereo(grbuf, left, len);
        }
        left += len;
        i += 1;
    }
}

fn intensity_stereo(
    grbuf: &mut [f32],
    ist_pos: &mut [u8; 39],
    gr: &GrInfo,
    next: &GrInfo,
    hdr: &[u8],
) {
    let n_sfb = gr.n_long_sfb as usize + gr.n_short_sfb as usize;
    let max_blocks = if gr.n_short_sfb != 0 { 3 } else { 1 };
    let mut max_band = stereo_top_band(grbuf, 576, gr.sfbtab, n_sfb);
    if gr.n_long_sfb != 0 {
        let m = max_band[0].max(max_band[1]).max(max_band[2]);
        max_band = [m; 3];
    }
    for i in 0..max_blocks {
        let default_pos = if header::test_mpeg1(hdr) { 3 } else { 0 };
        let itop = n_sfb - max_blocks + i;
        let prev = itop - max_blocks;
        ist_pos[itop] = if max_band[i] >= prev as i32 {
            default_pos
        } else {
            ist_pos[prev]
        };
    }
    stereo_process(
        grbuf,
        ist_pos,
        gr.sfbtab,
        hdr,
        &max_band,
        (next.scalefac_compress & 1) as u32,
    );
}

fn reorder(work: &mut [f32], start: usize, scratch: &mut [f32], sfb: &[u8]) {
    let mut src = start;
    let mut d = 0usize;
    let mut si = 0usize;
    loop {
        let len = sfb.get(si).copied().unwrap_or(0) as usize;
        if len == 0 || d + 3 * len > scratch.len() {
            break;
        }
        for _ in 0..len {
            let at = |i: usize| work.get(i).copied().unwrap_or(0.0);
            scratch[d] = at(src);
            scratch[d + 1] = at(src + len);
            scratch[d + 2] = at(src + 2 * len);
            d += 3;
            src += 1;
        }
        si += 3;
        src += 2 * len;
    }
    let d = d.min(work.len().saturating_sub(start));
    work[start..start + d].copy_from_slice(&scratch[..d]);
}

fn antialias(grbuf: &mut [f32], mut nbands: i32) {
    let mut g = 0usize;
    while nbands > 0 {
        for i in 0..8 {
            let u = grbuf[g + 18 + i];
            let d = grbuf[g + 17 - i];
            grbuf[g + 18 + i] = u * AA[0][i] - d * AA[1][i];
            grbuf[g + 17 - i] = u * AA[1][i] + d * AA[0][i];
        }
        nbands -= 1;
        g += 18;
    }
}

fn dct3_9(y: &mut [f32; 9]) {
    let (mut s0, mut s2, mut s4, mut s6, mut s8) = (y[0], y[2], y[4], y[6], y[8]);
    let mut t0 = s0 + s6 * 0.5;
    s0 -= s6;
    let mut t4 = (s4 + s2) * 0.939_692_62;
    let mut t2 = (s8 + s2) * 0.766_044_44;
    s6 = (s4 - s8) * 0.173_648_18;
    s4 += s8 - s2;

    s2 = s0 - s4 * 0.5;
    y[4] = s4 + s0;
    s8 = t0 - t2 + s6;
    s0 = t0 - t4 + t2;
    s4 = t0 + t4 - s6;

    let (mut s1, mut s3, mut s5, mut s7) = (y[1], y[3], y[5], y[7]);

    s3 *= 0.866_025_40;
    t0 = (s5 + s1) * 0.984_807_75;
    t4 = (s5 - s7) * 0.342_020_14;
    t2 = (s1 + s7) * 0.642_787_61;
    s1 = (s1 - s5 - s7) * 0.866_025_40;

    s5 = t0 - s3 - t2;
    s7 = t4 - s3 - t0;
    s3 = t4 + s3 - t2;

    y[0] = s4 - s7;
    y[1] = s2 + s1;
    y[2] = s0 - s3;
    y[3] = s8 + s5;
    y[5] = s8 - s5;
    y[6] = s0 + s3;
    y[7] = s2 - s1;
    y[8] = s4 + s7;
}

fn imdct36(grbuf: &mut [f32], overlap: &mut [f32], window: &[f32; 18], nbands: usize) {
    for j in 0..nbands {
        let g = j * 18;
        let o = j * 9;
        let mut co = [0.0f32; 9];
        let mut si = [0.0f32; 9];
        co[0] = -grbuf[g];
        si[0] = grbuf[g + 17];
        for i in 0..4 {
            si[8 - 2 * i] = grbuf[g + 4 * i + 1] - grbuf[g + 4 * i + 2];
            co[1 + 2 * i] = grbuf[g + 4 * i + 1] + grbuf[g + 4 * i + 2];
            si[7 - 2 * i] = grbuf[g + 4 * i + 4] - grbuf[g + 4 * i + 3];
            co[2 + 2 * i] = -(grbuf[g + 4 * i + 3] + grbuf[g + 4 * i + 4]);
        }
        dct3_9(&mut co);
        dct3_9(&mut si);

        si[1] = -si[1];
        si[3] = -si[3];
        si[5] = -si[5];
        si[7] = -si[7];

        for i in 0..9 {
            let ovl = overlap[o + i];
            let sum = co[i] * TWID9[9 + i] + si[i] * TWID9[i];
            overlap[o + i] = co[i] * TWID9[i] - si[i] * TWID9[9 + i];
            grbuf[g + i] = ovl * window[i] - sum * window[9 + i];
            grbuf[g + 17 - i] = ovl * window[9 + i] + sum * window[i];
        }
    }
}

fn idct3(x0: f32, x1: f32, x2: f32) -> [f32; 3] {
    let m1 = x1 * 0.866_025_40;
    let a1 = x0 - x2 * 0.5;
    [a1 + m1, x0 + x2, a1 - m1]
}

fn imdct12(x: &[f32], dst: &mut [f32], overlap: &mut [f32]) {
    let co = idct3(-x[0], x[6] + x[3], x[12] + x[9]);
    let mut si = idct3(x[15], x[12] - x[9], x[6] - x[3]);
    si[1] = -si[1];
    for i in 0..3 {
        let ovl = overlap[i];
        let sum = co[i] * TWID3[3 + i] + si[i] * TWID3[i];
        overlap[i] = co[i] * TWID3[i] - si[i] * TWID3[3 + i];
        dst[i] = ovl * TWID3[2 - i] - sum * TWID3[5 - i];
        dst[5 - i] = ovl * TWID3[5 - i] + sum * TWID3[2 - i];
    }
}

fn imdct_short(grbuf: &mut [f32], overlap: &mut [f32], nbands: usize) {
    for band in 0..nbands {
        let g = band * 18;
        let o = band * 9;
        let mut tmp = [0.0f32; 18];
        tmp.copy_from_slice(&grbuf[g..g + 18]);
        grbuf[g..g + 6].copy_from_slice(&overlap[o..o + 6]);
        imdct12(&tmp, &mut grbuf[g + 6..g + 12], &mut overlap[o + 6..o + 9]);
        imdct12(
            &tmp[1..],
            &mut grbuf[g + 12..g + 18],
            &mut overlap[o + 6..o + 9],
        );
        let (front, back) = overlap[o..o + 9].split_at_mut(6);
        imdct12(&tmp[2..], front, back);
    }
}

fn change_sign(grbuf: &mut [f32]) {
    for band in (1..32).step_by(2) {
        for i in (1..18).step_by(2) {
            grbuf[band * 18 + i] = -grbuf[band * 18 + i];
        }
    }
}

fn imdct_gr(grbuf: &mut [f32], overlap: &mut [f32], block_type: u8, n_long_bands: usize) {
    if n_long_bands != 0 {
        imdct36(grbuf, overlap, &MDCT_WINDOW[0], n_long_bands);
    }
    let grbuf = &mut grbuf[18 * n_long_bands..];
    let overlap = &mut overlap[9 * n_long_bands..];
    if block_type == SHORT_BLOCK_TYPE {
        imdct_short(grbuf, overlap, 32 - n_long_bands);
    } else {
        let window = &MDCT_WINDOW[(block_type == STOP_BLOCK_TYPE) as usize];
        imdct36(grbuf, overlap, window, 32 - n_long_bands);
    }
}

pub fn save_reservoir(res: Reservoir, maindata: &[u8], bs_pos: i32, bs_limit: i32) {
    let mut pos = ((bs_pos + 7) as u32 / 8) as i32;
    let mut remains = (bs_limit as u32 / 8) as i32 - pos;
    if remains > MAX_BITRESERVOIR_BYTES as i32 {
        pos += remains - MAX_BITRESERVOIR_BYTES as i32;
        remains = MAX_BITRESERVOIR_BYTES as i32;
    }
    if remains > 0 {
        let (pos, remains) = (pos as usize, remains as usize);
        res.reserv_buf[..remains].copy_from_slice(&maindata[pos..pos + remains]);
    }
    *res.reserv = remains;
}

pub fn restore_reservoir(
    res: &Reservoir,
    frame: &BitReader,
    maindata: &mut [u8],
    main_data_begin: i32,
) -> (bool, i32) {
    let reserv = *res.reserv;
    let frame_bytes = ((frame.limit - frame.pos) / 8).max(0) as usize;
    let bytes_have = reserv.min(main_data_begin).max(0) as usize;
    let from = (reserv - main_data_begin).max(0) as usize;
    maindata[..bytes_have].copy_from_slice(&res.reserv_buf[from..from + bytes_have]);
    let src = (frame.pos / 8) as usize;
    let frame_bytes = frame_bytes
        .min(maindata.len() - bytes_have)
        .min(frame.buf.len().saturating_sub(src));
    maindata[bytes_have..bytes_have + frame_bytes]
        .copy_from_slice(&frame.buf[src..src + frame_bytes]);
    (reserv >= main_data_begin, (bytes_have + frame_bytes) as i32)
}

pub fn decode(
    hdr: &[u8],
    mdct_overlap: &mut [[f32; 288]; 2],
    scratch: &mut Scratch,
    bs: &mut BitReader,
    gr_base: usize,
    nch: usize,
) {
    let Scratch {
        gr_info,
        work,
        syn,
        ist_pos,
    } = scratch;
    let (grbuf, scf) = work.split_at_mut(GRBUF);
    let scf: &mut [f32; 40] = scf.try_into().expect("scalefactor slot");

    for ch in 0..nch {
        let gr = &gr_info[gr_base + ch];
        let layer3gr_limit = bs.pos + gr.part_23_length as i32;
        decode_scalefactors(hdr, &mut ist_pos[ch], bs, gr, scf, ch);
        huffman(
            &mut grbuf[576 * ch..576 * ch + 576],
            bs,
            gr,
            scf,
            layer3gr_limit,
        );
    }

    if header::test_i_stereo(hdr) {
        let next = gr_info[(gr_base + 1).min(3)];
        intensity_stereo(grbuf, &mut ist_pos[1], &gr_info[gr_base], &next, hdr);
    } else if header::is_ms_stereo(hdr) {
        midside_stereo(grbuf, 0, 576);
    }

    for ch in 0..nch {
        let gr = &gr_info[gr_base + ch];
        let mut aa_bands = 31;
        let n_long_bands = (if gr.mixed_block_flag != 0 { 2 } else { 0 })
            << ((header::my_sample_rate(hdr) == 2) as u32);

        if gr.n_short_sfb != 0 {
            aa_bands = n_long_bands as i32 - 1;
            reorder(
                &mut work[..],
                576 * ch + n_long_bands * 18,
                &mut syn[..],
                &gr.sfbtab[gr.n_long_sfb as usize..],
            );
        }
        let channel = &mut work[576 * ch..576 * ch + 576];

        antialias(channel, aa_bands);
        imdct_gr(channel, &mut mdct_overlap[ch], gr.block_type, n_long_bands);
        change_sign(channel);
    }
}
