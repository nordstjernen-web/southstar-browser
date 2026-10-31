//! Southstar — the libav backend: Matroska/WebM and ISO-BMFF demuxed from a growing in-memory buffer, decoded and scaled to BGRA.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_av::{self as av, AVRational, CodecDescription, Format, Frame, Packet, Scaler};
use southstar_av::{codec_parameters, frame, stream};

use super::{Next, copy_bytes, pixel_buffer};
use crate::ffi::texture_from_bgra;

const SWS_ROW_ALIGN: usize = 64;

#[derive(Clone, Copy)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
    Vp9,
    Vp8,
    Aac,
    Mp3,
    Ac3,
    Eac3,
    Flac,
    Alac,
    Opus,
    Vorbis,
}

impl Codec {
    fn id(self) -> c_int {
        match self {
            Codec::H264 => av::AV_CODEC_ID_H264,
            Codec::Hevc => av::AV_CODEC_ID_HEVC,
            Codec::Av1 => av::AV_CODEC_ID_AV1,
            Codec::Vp9 => av::AV_CODEC_ID_VP9,
            Codec::Vp8 => av::AV_CODEC_ID_VP8,
            Codec::Aac => av::AV_CODEC_ID_AAC,
            Codec::Mp3 => av::AV_CODEC_ID_MP3,
            Codec::Ac3 => av::AV_CODEC_ID_AC3,
            Codec::Eac3 => av::AV_CODEC_ID_EAC3,
            Codec::Flac => av::AV_CODEC_ID_FLAC,
            Codec::Alac => av::AV_CODEC_ID_ALAC,
            Codec::Opus => av::AV_CODEC_ID_OPUS,
            Codec::Vorbis => av::AV_CODEC_ID_VORBIS,
        }
    }
}

pub fn decoder_exists(codec: Codec) -> bool {
    av::available() && CodecDescription::find(codec.id()).is_some()
}

const VIDEO_CODECS: [c_int; 7] = [
    av::AV_CODEC_ID_VP9,
    av::AV_CODEC_ID_VP8,
    av::AV_CODEC_ID_H264,
    av::AV_CODEC_ID_HEVC,
    av::AV_CODEC_ID_AV1,
    av::AV_CODEC_ID_MPEG4,
    av::AV_CODEC_ID_THEORA,
];

pub struct Demuxer {
    sws: Scaler,
    frame: Frame,
    pkt: Packet,
    vdec: av::Codec,
    input: Format<'static>,
    vstream: c_int,
    time_base: AVRational,
    draining: bool,
}

pub struct Opened {
    pub demuxer: Demuxer,
    pub width: i32,
    pub height: i32,
    pub duration: f64,
    pub demuxed_end: f64,
    pub has_audio: bool,
}

impl Demuxer {
    pub fn open(bytes: &[u8]) -> Option<Opened> {
        if !av::available() {
            return None;
        }
        av::quiet_logging();
        let mut input = Format::open_owned(copy_bytes(bytes)?)?;
        if !input.find_stream_info() {
            return None;
        }
        let (vstream, dec) = input.best_stream_and_decoder(av::AVMEDIA_TYPE_VIDEO)?;
        let vs = input.stream(vstream)?;
        let par = vs.parameters()?;
        if !VIDEO_CODECS.contains(&par.get(&codec_parameters::CODEC_ID)) {
            return None;
        }
        let vdec = av::Codec::open(&par, dec)?;
        let width = par.get(&codec_parameters::WIDTH);
        let height = par.get(&codec_parameters::HEIGHT);
        if !super::dimensions_valid(width, height) {
            return None;
        }
        let time_base = vs.time_base();
        let stream_duration = vs.get(&stream::DURATION);
        let mut pkt = Packet::new()?;
        let frame = Frame::new()?;

        let fmt_duration = input.duration();
        let duration = if fmt_duration > 0 {
            fmt_duration as f64 / av::AV_TIME_BASE as f64
        } else if stream_duration > 0 {
            stream_duration as f64 * time_base.to_f64()
        } else {
            0.0
        };

        let mut last_pts = 0.0;
        while input.read_packet(&mut pkt) {
            if pkt.stream_index() == vstream && pkt.pts() != av::AV_NOPTS_VALUE {
                let t = pkt.pts() as f64 * time_base.to_f64();
                if t > last_pts {
                    last_pts = t;
                }
            }
            pkt.unref();
        }
        input.seek(vstream, 0, av::AVSEEK_FLAG_BACKWARD);
        let has_audio = input.best_stream(av::AVMEDIA_TYPE_AUDIO).is_some();

        Some(Opened {
            demuxer: Demuxer {
                sws: Scaler::default(),
                frame,
                pkt,
                vdec,
                input,
                vstream,
                time_base,
                draining: false,
            },
            width,
            height,
            duration,
            demuxed_end: last_pts,
            has_audio,
        })
    }

    fn seek(&mut self, ts: i64) -> bool {
        self.input.seek(self.vstream, ts, av::AVSEEK_FLAG_BACKWARD)
    }

    pub fn rewind(&mut self) {
        self.seek(0);
        self.vdec.flush();
        self.draining = false;
    }

    pub fn seek_to(&mut self, seconds: f64) -> bool {
        let ts = (seconds / self.time_base.to_f64()) as i64;
        if !self.seek(ts) {
            return false;
        }
        self.vdec.flush();
        self.draining = false;
        true
    }

    pub fn extend(&mut self, bytes: &[u8]) -> bool {
        let Some(data) = self.input.buffered() else {
            return false;
        };
        let cur = data.len();
        if bytes.len() < cur {
            return false;
        }
        if bytes.len() == cur {
            return true;
        }
        let head = cur.min(4096);
        if bytes[..head] != data[..head] {
            return false;
        }
        if cur > 4096 && bytes[cur - 4096..cur] != data[cur - 4096..] {
            return false;
        }
        if !self.input.append(&bytes[cur..]) {
            return false;
        }
        if self.draining {
            self.vdec.flush();
            self.draining = false;
        }
        true
    }

    fn frame_to_texture(&mut self, w: i32, h: i32) -> Option<crate::ffi::Texture> {
        let stride = (w as usize * 4).next_multiple_of(SWS_ROW_ALIGN);
        let mut bgra = pixel_buffer(stride * h as usize, 0)?;
        if !self.sws.prepare(&self.frame, w as u32, h as u32) {
            return None;
        }
        self.sws.scale_into(&self.frame, &mut bgra, stride);
        texture_from_bgra(w, h, &bgra, stride)
    }

    pub fn next(&mut self, w: i32, h: i32, fallback_time: f64) -> Next {
        loop {
            let r = self.vdec.receive(&mut self.frame);
            if r == 0 {
                let pts = self.frame.get(&frame::PTS);
                let t = if pts == av::AV_NOPTS_VALUE {
                    fallback_time
                } else {
                    pts as f64 * self.time_base.to_f64()
                };
                let tex = self.frame_to_texture(w, h);
                self.frame.unref();
                return Next::Frame(tex, t);
            }
            if r == av::AVERROR_EOF || (r < 0 && r != av::AVERROR_EAGAIN) {
                return Next::Ended;
            }

            let mut got = self.input.read_packet(&mut self.pkt);
            while got && self.pkt.stream_index() != self.vstream {
                self.pkt.unref();
                got = self.input.read_packet(&mut self.pkt);
            }
            if got {
                self.vdec.send(Some(&self.pkt));
                self.pkt.unref();
            } else if !self.draining {
                self.vdec.send(None);
                self.draining = true;
            } else {
                return Next::Ended;
            }
        }
    }
}

pub fn probe_chunk_range(init: &[u8], chunk: &[u8]) -> Option<(f64, f64)> {
    if chunk.is_empty() || !av::available() {
        return None;
    }
    let mut joined = Vec::new();
    joined.try_reserve_exact(init.len() + chunk.len()).ok()?;
    joined.extend_from_slice(init);
    joined.extend_from_slice(chunk);

    let mut input = Format::open_owned(joined)?;
    let mut start = f64::MAX;
    let mut end = 0.0;
    if let Some(mut pkt) = Packet::new() {
        while input.read_packet(&mut pkt) {
            let stream = input.stream(pkt.stream_index());
            if let Some(stream) = stream.filter(|_| pkt.pts() != av::AV_NOPTS_VALUE) {
                let tb = stream.time_base().to_f64();
                let pts = pkt.pts() as f64;
                let span = if pkt.duration() > 0 {
                    pkt.duration() as f64
                } else {
                    0.0
                };
                let t = (pts + span) * tb;
                let t0 = pts * tb;
                if t0 < start {
                    start = t0;
                }
                if t > end {
                    end = t;
                }
            }
            pkt.unref();
        }
    }
    if start == f64::MAX || end <= start {
        return None;
    }
    Some((start, end))
}
