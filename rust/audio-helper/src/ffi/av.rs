//! Southstar — WebM, Ogg and MP4 audio through libav: whole files decoded from memory, and growing files streamed in as they download.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::ffi::CString;

use crate::decode::{Decoded, Sink, file_size, max_floats};
use crate::ffi::sdl::AudioStream;
use crate::mixer::DEVICE_RATE;
pub use southstar_av::quiet_logging as quiet;
use southstar_av::{self as av, codec_context, frame};
use southstar_av::{Codec, Format, Frame, Packet, Resampler};

const DRAIN_SAMPLES: usize = 4096;

struct Decoder {
    resampler: Resampler,
    frame: Frame,
    packet: Packet,
    codec: Codec,
    sidx: c_int,
    rate: c_int,
    channels: c_int,
    sample_fmt: c_int,
}

impl Decoder {
    fn open(format: &mut Format) -> Option<Decoder> {
        let (sidx, description) = format.best_stream_and_decoder(av::AVMEDIA_TYPE_AUDIO)?;
        let codec = Codec::open(&format.stream(sidx)?.parameters()?, description)?;
        let rate = codec.get(&codec_context::SAMPLE_RATE);
        let channels = codec.channels();
        let sample_fmt = codec.get(&codec_context::SAMPLE_FMT);
        if rate <= 0 || rate > 192000 || !(1..=8).contains(&channels) {
            return None;
        }
        let resampler = Resampler::to_float(&codec, rate, sample_fmt)?;
        let packet = Packet::new()?;
        let frame = Frame::new()?;
        Some(Decoder {
            resampler,
            frame,
            packet,
            codec,
            sidx,
            rate,
            channels,
            sample_fmt,
        })
    }

    fn frame_matches(&self) -> bool {
        self.frame.channels() == self.channels && self.frame.get(&frame::FORMAT) == self.sample_fmt
    }

    fn out_samples(&self) -> c_int {
        self.resampler.out_samples(&self.frame)
    }

    fn convert_into(&self, dst: &mut Vec<f32>, out_samples: c_int) -> c_int {
        self.resampler
            .convert_into(&self.frame, self.channels, dst, out_samples)
    }

    fn unref(&mut self) {
        self.frame.unref();
    }

    fn time_base(&self, format: &Format) -> f64 {
        format
            .stream(self.sidx)
            .map_or(0.0, |stream| stream.time_base().to_f64())
    }
}

pub fn decode_memory(bytes: &[u8]) -> Option<Decoded> {
    quiet();
    let mut format = Format::open_memory(bytes)?;
    if !format.find_stream_info() {
        return None;
    }
    let mut d = Decoder::open(&mut format)?;
    let mut pcm = Vec::new();
    let max = max_floats(d.rate, d.channels);
    decode_all(&mut format, &mut d, &mut pcm, max);
    let decoded = (!pcm.is_empty()).then_some(Decoded {
        pcm,
        rate: d.rate,
        channels: d.channels,
    });
    drop(d);
    drop(format);
    decoded
}

fn decode_all(format: &mut Format, d: &mut Decoder, pcm: &mut Vec<f32>, max: usize) {
    let mut eof = false;
    while !eof {
        if !format.read_packet(&mut d.packet) {
            d.codec.send(None);
            eof = true;
        } else if d.packet.stream_index() == d.sidx {
            d.codec.send(Some(&d.packet));
            d.packet.unref();
        } else {
            d.packet.unref();
            continue;
        }
        loop {
            let rr = d.codec.receive(&mut d.frame);
            if rr == av::AVERROR_EAGAIN || rr == av::AVERROR_EOF {
                break;
            }
            if rr < 0 {
                return;
            }
            if !d.frame_matches() {
                d.unref();
                continue;
            }
            let out_samples = d.out_samples();
            let add = out_samples.max(0) as usize * d.channels as usize;
            if out_samples < 0 || pcm.try_reserve(add).is_err() {
                d.unref();
                return;
            }
            let got = d.convert_into(pcm, out_samples);
            d.unref();
            if got < 0 {
                return;
            }
            if pcm.len() >= max {
                return;
            }
        }
    }
}

pub struct Input {
    stream: AudioStream,
    decoder: Decoder,
    format: Format<'static>,
    path: Vec<u8>,
    known_size: i64,
    resume_target: f64,
    resume_pending: bool,
    discard_frames: usize,
}

impl Input {
    pub fn open(path: &[u8]) -> Option<Input> {
        let cpath = CString::new(path).ok()?;
        let mut format = Format::open_file(&cpath)?;
        if !format.find_stream_info() {
            return None;
        }
        let decoder = Decoder::open(&mut format)?;
        let stream = AudioStream::to_stereo(decoder.channels, decoder.rate, DEVICE_RATE as c_int)?;
        Some(Input {
            stream,
            decoder,
            format,
            path: path.to_vec(),
            known_size: file_size(path),
            resume_target: 0.0,
            resume_pending: false,
            discard_frames: 0,
        })
    }

    pub fn path(&self) -> &[u8] {
        &self.path
    }

    fn drain(&mut self, sink: &mut dyn Sink) {
        let mut buf = [0f32; DRAIN_SAMPLES];
        loop {
            let mut frames = self.stream.get_frames(&mut buf);
            if frames == 0 {
                break;
            }
            let mut start = 0;
            if self.discard_frames > 0 {
                let skip = self.discard_frames.min(frames);
                self.discard_frames -= skip;
                start = skip * 2;
                frames -= skip;
            }
            if frames > 0 && !sink.append(&buf[start..start + frames * 2]) {
                break;
            }
        }
    }

    fn note_resume_start(&mut self) {
        if !self.resume_pending {
            return;
        }
        self.resume_pending = false;
        let pts = self.decoder.frame.get(&frame::PTS);
        let t0 = if pts != av::AV_NOPTS_VALUE {
            pts as f64 * self.decoder.time_base(&self.format)
        } else {
            self.resume_target
        };
        let ahead = self.resume_target - t0;
        self.discard_frames = if ahead > 0.0 {
            (ahead * DEVICE_RATE as f64) as usize
        } else {
            0
        };
    }

    fn feed_frame(&mut self) {
        let d = &self.decoder;
        let out_samples = d.out_samples();
        if out_samples > 0 {
            let mut conv = Vec::new();
            if conv
                .try_reserve_exact(out_samples as usize * d.channels as usize)
                .is_ok()
                && d.convert_into(&mut conv, out_samples) > 0
            {
                self.stream.put(&conv);
            }
        }
    }
}

fn reopen(slot: &mut Option<Input>, sink: &mut dyn Sink) -> bool {
    let Some(old) = slot.as_mut() else {
        return false;
    };
    old.drain(sink);
    let path = old.path.clone();
    let carried = (old.resume_target, old.resume_pending, old.discard_frames);
    let decoded_end = sink.frames() as f64 / DEVICE_RATE as f64;
    *slot = None;
    let Some(mut input) = Input::open(&path) else {
        return false;
    };
    (
        input.resume_target,
        input.resume_pending,
        input.discard_frames,
    ) = carried;
    if decoded_end > 0.05 {
        let sidx = input.decoder.sidx;
        let ts = (decoded_end / input.decoder.time_base(&input.format)) as i64;
        if input.format.seek(sidx, ts, av::AVSEEK_FLAG_BACKWARD) {
            input.decoder.codec.flush();
            input.resume_target = decoded_end;
            input.resume_pending = true;
        }
    }
    *slot = Some(input);
    true
}

pub fn pump(slot: &mut Option<Input>, sink: &mut dyn Sink) {
    let mut reopened = false;
    loop {
        let Some(input) = slot.as_mut() else {
            return;
        };
        if !input.format.read_packet(&mut input.decoder.packet) {
            if !reopened && file_size(&input.path) > input.known_size {
                reopened = true;
                if reopen(slot, sink) {
                    continue;
                }
            }
            break;
        }
        let d = &mut input.decoder;
        if d.packet.stream_index() != d.sidx {
            d.packet.unref();
            continue;
        }
        d.codec.send(Some(&d.packet));
        d.packet.unref();
        while input.decoder.codec.receive(&mut input.decoder.frame) == 0 {
            input.note_resume_start();
            if !input.decoder.frame_matches() {
                input.decoder.unref();
                continue;
            }
            input.feed_frame();
            input.decoder.unref();
            input.drain(sink);
        }
    }
    if let Some(input) = slot.as_mut() {
        input.drain(sink);
    }
}
