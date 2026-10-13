//! Southstar — an MPEG-1/2/2.5 Layer I/II/III audio decoder producing float PCM one frame at a time. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![allow(
    clippy::excessive_precision,
    clippy::approx_constant,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)]

mod bits;
mod header;
mod layer12;
mod layer3;
mod synth;
mod tables;

use bits::BitReader;
use header::HDR_SIZE;
use layer3::{GRBUF, MAINDATA_BYTES, MAX_BITRESERVOIR_BYTES, Reservoir, Scratch};

pub const MAX_SAMPLES_PER_FRAME: usize = 1152 * 2;

const MAX_FREE_FORMAT_FRAME_SIZE: i32 = 2304;
const MAX_FRAME_SYNC_MATCHES: i32 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameInfo {
    pub frame_bytes: usize,
    pub frame_offset: usize,
    pub channels: u32,
    pub hz: u32,
    pub layer: u32,
    pub bitrate_kbps: u32,
}

pub struct Decoder {
    mdct_overlap: [[f32; 288]; 2],
    qmf_state: [f32; 960],
    reserv: i32,
    free_format_bytes: i32,
    header: [u8; 4],
    reserv_buf: [u8; MAX_BITRESERVOIR_BYTES],
    scratch: Box<Scratch>,
    maindata: Box<[u8; MAINDATA_BYTES]>,
}

impl Default for Decoder {
    fn default() -> Decoder {
        Decoder::new()
    }
}

fn match_frame(hdr: &[u8], frame_bytes: i32) -> bool {
    let mp3_bytes = hdr.len() as i64;
    let mut i = 0i64;
    for nmatch in 0..MAX_FRAME_SYNC_MATCHES {
        let at = &hdr[i as usize..];
        i += (header::frame_bytes(at, frame_bytes) + header::padding(at)) as i64;
        if i + HDR_SIZE as i64 > mp3_bytes {
            return nmatch > 0;
        }
        if !header::compare(hdr, &hdr[i as usize..]) {
            return false;
        }
    }
    true
}

fn find_frame(mp3: &[u8], free_format_bytes: &mut i32) -> (usize, i32) {
    let mp3_bytes = mp3.len() as i64;
    let hdr_size = HDR_SIZE as i64;
    for i in 0..mp3.len().saturating_sub(HDR_SIZE) {
        let at = &mp3[i..];
        if !header::valid(at) {
            continue;
        }
        let ii = i as i64;
        let mut frame_bytes = header::frame_bytes(at, *free_format_bytes);
        let mut frame_and_padding = frame_bytes + header::padding(at);

        let mut k = HDR_SIZE as i64;
        while frame_bytes == 0
            && k < MAX_FREE_FORMAT_FRAME_SIZE as i64
            && ii + 2 * k < mp3_bytes - hdr_size
        {
            if header::compare(at, &at[k as usize..]) {
                let fb = k as i32 - header::padding(at);
                let nextfb = fb + header::padding(&at[k as usize..]);
                if ii + k + nextfb as i64 + hdr_size > mp3_bytes
                    || !header::compare(at, &at[(k + nextfb as i64) as usize..])
                {
                    k += 1;
                    continue;
                }
                frame_and_padding = k as i32;
                frame_bytes = fb;
                *free_format_bytes = fb;
            }
            k += 1;
        }
        if (frame_bytes != 0
            && ii + frame_and_padding as i64 <= mp3_bytes
            && match_frame(at, frame_bytes))
            || (i == 0 && frame_and_padding as i64 == mp3_bytes)
        {
            return (i, frame_and_padding);
        }
        *free_format_bytes = 0;
    }
    (mp3.len(), 0)
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            mdct_overlap: [[0.0; 288]; 2],
            qmf_state: [0.0; 960],
            reserv: 0,
            free_format_bytes: 0,
            header: [0; 4],
            reserv_buf: [0; MAX_BITRESERVOIR_BYTES],
            scratch: Box::new(Scratch::new()),
            maindata: Box::new([0; MAINDATA_BYTES]),
        }
    }

    fn reset(&mut self) {
        self.mdct_overlap = [[0.0; 288]; 2];
        self.qmf_state = [0.0; 960];
        self.reserv = 0;
        self.free_format_bytes = 0;
        self.header = [0; 4];
        self.reserv_buf = [0; MAX_BITRESERVOIR_BYTES];
    }

    pub fn decode_frame(
        &mut self,
        mp3: &[u8],
        pcm: Option<&mut [f32; MAX_SAMPLES_PER_FRAME]>,
    ) -> (usize, FrameInfo) {
        let mp3_bytes = mp3.len() as i64;
        let mut info = FrameInfo::default();
        let mut i = 0usize;
        let mut frame_size: i32 = 0;

        if mp3.len() > 4 && self.header[0] == 0xff && header::compare(&self.header, mp3) {
            frame_size = header::frame_bytes(mp3, self.free_format_bytes) + header::padding(mp3);
            if frame_size as i64 != mp3_bytes
                && (frame_size as i64 + HDR_SIZE as i64 > mp3_bytes
                    || !header::compare(mp3, &mp3[frame_size as usize..]))
            {
                frame_size = 0;
            }
        }
        if frame_size == 0 {
            self.reset();
            let (offset, size) = find_frame(mp3, &mut self.free_format_bytes);
            i = offset;
            frame_size = size;
            if frame_size == 0 || i as i64 + frame_size as i64 > mp3_bytes {
                info.frame_bytes = i;
                return (0, info);
            }
        }

        let frame = &mp3[i..i + frame_size as usize];
        if frame.len() < HDR_SIZE {
            info.frame_bytes = i + frame.len();
            return (0, info);
        }
        self.header.copy_from_slice(&frame[..HDR_SIZE]);
        info.frame_bytes = i + frame_size as usize;
        info.frame_offset = i;
        info.channels = if header::is_mono(frame) { 1 } else { 2 };
        info.hz = header::sample_rate_hz(frame);
        info.layer = 4 - header::layer(frame);
        info.bitrate_kbps = header::bitrate_kbps(frame);

        let Some(pcm) = pcm else {
            return (header::frame_samples(frame) as usize, info);
        };

        let mut bs_frame = BitReader::new(&frame[HDR_SIZE..], frame_size - HDR_SIZE as i32);
        if header::is_crc(frame) {
            bs_frame.get_bits(16);
        }

        let nch = info.channels as usize;
        let mut success = true;
        if info.layer == 3 {
            let main_data_begin =
                layer3::read_side_info(&mut bs_frame, &mut self.scratch.gr_info, frame);
            if main_data_begin < 0 || bs_frame.pos > bs_frame.limit {
                self.header[0] = 0;
                return (0, info);
            }
            let scratch = &mut *self.scratch;
            let (ok, main_bytes) = layer3::restore_reservoir(
                &Reservoir {
                    reserv: &mut self.reserv,
                    reserv_buf: &mut self.reserv_buf,
                },
                &bs_frame,
                &mut self.maindata[..],
                main_data_begin,
            );
            success = ok;
            let mut bs = BitReader::new(&self.maindata[..], main_bytes);
            if success {
                let granules = if header::test_mpeg1(frame) { 2 } else { 1 };
                let mut pcm_offset = 0usize;
                for igr in 0..granules {
                    scratch.work[..GRBUF].fill(0.0);
                    layer3::decode(
                        frame,
                        &mut self.mdct_overlap,
                        scratch,
                        &mut bs,
                        igr * nch,
                        nch,
                    );
                    synth::synth_granule(
                        &mut self.qmf_state,
                        &mut scratch.work[..GRBUF],
                        18,
                        nch,
                        &mut pcm[..],
                        pcm_offset,
                        &mut scratch.syn,
                    );
                    pcm_offset += 576 * nch;
                }
            }
            let (pos, limit) = (bs.pos, bs.limit);
            layer3::save_reservoir(
                Reservoir {
                    reserv: &mut self.reserv,
                    reserv_buf: &mut self.reserv_buf,
                },
                &self.maindata[..],
                pos,
                limit,
            );
        } else {
            let scratch = &mut *self.scratch;
            let mut sci = layer12::ScaleInfo::new();
            layer12::read_scale_info(frame, &mut bs_frame, &mut sci);
            scratch.work[..GRBUF].fill(0.0);
            let group_size = (info.layer | 1) as usize;
            let mut pcm_offset = 0usize;
            let mut filled = 0usize;
            for igr in 0..3 {
                filled += layer12::dequantize_granule(
                    &mut scratch.work[..GRBUF],
                    filled,
                    &mut bs_frame,
                    &sci,
                    group_size,
                );
                if filled == 12 {
                    filled = 0;
                    layer12::apply_scf_384(&sci, igr, &mut scratch.work[..GRBUF]);
                    synth::synth_granule(
                        &mut self.qmf_state,
                        &mut scratch.work[..GRBUF],
                        12,
                        nch,
                        &mut pcm[..],
                        pcm_offset,
                        &mut scratch.syn,
                    );
                    scratch.work[..GRBUF].fill(0.0);
                    pcm_offset += 384 * nch;
                }
                if bs_frame.pos > bs_frame.limit {
                    self.header[0] = 0;
                    return (0, info);
                }
            }
        }
        let samples = if success {
            header::frame_samples(&self.header) as usize
        } else {
            0
        };
        (samples, info)
    }
}
