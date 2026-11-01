//! Southstar — the H.264 and AAC Media Foundation transforms driven synchronously: NV12 pictures and float or 16-bit PCM out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod com;

use std::sync::OnceLock;

use com::{Attributes, Buffer, MediaType, Sample, Transform};

use crate::bitstream::{aac_core_config, h264_config, to_annex_b};
use crate::{Pcm, Picture, Received, Sent};

const OUTPUT_ATTEMPTS: usize = 4;
const AAC_USER_DATA_HEADER: usize = 12;
const MAX_AAC_CHANNELS: u32 = 6;
const DEFAULT_AUDIO_BUFFER: u32 = 65536;

pub fn h264_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        com::decoder_exists(
            &com::CATEGORY_VIDEO_DECODER,
            &com::MEDIA_TYPE_VIDEO,
            &com::VIDEO_FORMAT_H264,
        )
    })
}

pub fn aac_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        com::decoder_exists(
            &com::CATEGORY_AUDIO_DECODER,
            &com::MEDIA_TYPE_AUDIO,
            &com::AUDIO_FORMAT_AAC,
        )
    })
}

fn start_streaming(transform: &Transform) {
    transform.message(com::MESSAGE_BEGIN_STREAMING);
    transform.message(com::MESSAGE_START_OF_STREAM);
}

fn restart(transform: &Transform) {
    transform.message(com::MESSAGE_FLUSH);
    transform.message(com::MESSAGE_START_OF_STREAM);
}

fn input_sample(parts: &[&[u8]], time: Option<i64>) -> Option<Sample> {
    let sample = Sample::with_buffer(&Buffer::filled(parts)?)?;
    if let Some(time) = time {
        sample.set_time(time);
    }
    Some(sample)
}

fn feed(transform: &Transform, sample: &Sample) -> Sent {
    match transform.process_input(sample) {
        com::E_NOT_ACCEPTING => Sent::Full,
        hr if com::succeeded(hr) => Sent::Accepted,
        _ => Sent::Failed,
    }
}

enum Output {
    Sample(Sample),
    Changed,
    NeedInput,
    Failed,
}

fn pull(transform: &Transform, provides_samples: bool, buffer_size: u32) -> Output {
    let provided = if provides_samples {
        None
    } else {
        match Buffer::new(buffer_size.max(1)).and_then(|buffer| Sample::with_buffer(&buffer)) {
            Some(sample) => Some(sample),
            None => return Output::Failed,
        }
    };
    let (hr, produced) = transform.process_output(provided.as_ref());
    match hr {
        com::E_STREAM_CHANGE => Output::Changed,
        com::E_NEED_MORE_INPUT => Output::NeedInput,
        hr if com::succeeded(hr) => produced.or(provided).map_or(Output::Failed, Output::Sample),
        _ => Output::Failed,
    }
}

struct VideoOutput {
    coded_width: u32,
    coded_height: u32,
    visible_width: u32,
    visible_height: u32,
    buffer_size: u32,
    provides_samples: bool,
}

pub struct VideoDecoder {
    transform: Transform,
    parameter_sets: Vec<u8>,
    nal_length_size: usize,
    need_parameter_sets: bool,
    width: u32,
    height: u32,
    output: Option<VideoOutput>,
    annex_b: Vec<u8>,
}

impl VideoDecoder {
    pub fn h264(config: &[u8], width: u32, height: u32) -> Option<VideoDecoder> {
        let config = h264_config(config)?;
        let transform = Transform::find(
            &com::CATEGORY_VIDEO_DECODER,
            &com::MEDIA_TYPE_VIDEO,
            &com::VIDEO_FORMAT_H264,
        )?;
        let input = MediaType::new()?;
        input.set_guid(&com::MT_MAJOR_TYPE, &com::MEDIA_TYPE_VIDEO);
        input.set_guid(&com::MT_SUBTYPE, &com::VIDEO_FORMAT_H264);
        if width > 0 && height > 0 {
            input.set_u64(
                &com::MT_FRAME_SIZE,
                (u64::from(width) << 32) | u64::from(height),
            );
        }
        input.set_u32(&com::MT_INTERLACE_MODE, com::INTERLACE_MIXED);
        if !transform.set_input_type(&input) {
            return None;
        }
        let mut decoder = VideoDecoder {
            transform,
            parameter_sets: config.parameter_sets,
            nal_length_size: config.nal_length_size,
            need_parameter_sets: true,
            width,
            height,
            output: None,
            annex_b: Vec::new(),
        };
        decoder.output = decoder.pick_output();
        start_streaming(&decoder.transform);
        Some(decoder)
    }

    fn pick_output(&self) -> Option<VideoOutput> {
        let chosen = (0..)
            .map_while(|index| self.transform.output_type(index))
            .find(|candidate| {
                candidate.get_guid(&com::MT_SUBTYPE) == Some(com::VIDEO_FORMAT_NV12)
                    && self.transform.set_output_type(candidate)
            })?;
        let (coded_width, coded_height) = chosen
            .get_u64(&com::MT_FRAME_SIZE)
            .map_or((self.width, self.height), |packed| {
                ((packed >> 32) as u32, packed as u32)
            });
        let (visible_width, visible_height) = match chosen
            .get_area(&com::MT_MINIMUM_DISPLAY_APERTURE)
            .filter(|area| area.width > 0 && area.height > 0)
        {
            Some(area) => (area.width as u32, area.height as u32),
            None if self.width > 0
                && self.height > 0
                && self.width <= coded_width
                && self.height <= coded_height =>
            {
                (self.width, self.height)
            }
            None => (coded_width, coded_height),
        };
        let info = self.transform.output_stream_info();
        Some(VideoOutput {
            coded_width,
            coded_height,
            visible_width: visible_width.min(coded_width),
            visible_height: visible_height.min(coded_height),
            buffer_size: info.size,
            provides_samples: info.flags & com::OUTPUT_PROVIDES_SAMPLES != 0,
        })
    }

    pub fn send(&mut self, packet: &[u8], time: Option<i64>) -> Sent {
        if packet.is_empty() || !to_annex_b(packet, self.nal_length_size, &mut self.annex_b) {
            return Sent::Failed;
        }
        let parts: [&[u8]; 2] = if self.need_parameter_sets {
            [&self.parameter_sets, &self.annex_b]
        } else {
            [&[], &self.annex_b]
        };
        let Some(sample) = input_sample(&parts, time) else {
            return Sent::Failed;
        };
        if self.need_parameter_sets {
            sample.set_u32(&com::SAMPLE_DISCONTINUITY, 1);
        }
        let sent = feed(&self.transform, &sample);
        if matches!(sent, Sent::Accepted) {
            self.need_parameter_sets = false;
        }
        sent
    }

    pub fn receive<R>(&mut self, use_picture: impl FnOnce(&Picture<'_>) -> R) -> Received<R> {
        for _ in 0..OUTPUT_ATTEMPTS {
            if self.output.is_none() {
                self.output = self.pick_output();
            }
            let Some(output) = &self.output else {
                return Received::NeedInput;
            };
            match pull(&self.transform, output.provides_samples, output.buffer_size) {
                Output::Changed => self.output = None,
                Output::NeedInput => return Received::NeedInput,
                Output::Failed => return Received::Failed,
                Output::Sample(sample) => {
                    return picture_of(&sample, output, use_picture)
                        .map_or(Received::Failed, Received::Output);
                }
            }
        }
        Received::Failed
    }

    pub fn drain(&mut self) {
        self.transform.message(com::MESSAGE_DRAIN);
    }

    pub fn flush(&mut self) {
        restart(&self.transform);
        self.need_parameter_sets = true;
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.transform.message(com::MESSAGE_END_STREAMING);
    }
}

fn picture_of<R>(
    sample: &Sample,
    output: &VideoOutput,
    use_picture: impl FnOnce(&Picture<'_>) -> R,
) -> Option<R> {
    let buffer = sample.contiguous()?;
    let time = sample.time();
    let rows = picture_rows(output);
    match buffer.two_dimensional() {
        Some(surface) => surface
            .read(rows, |bytes, pitch| {
                picture_view(bytes, pitch, output, time).map(|p| use_picture(&p))
            })
            .flatten(),
        None => buffer
            .read(|bytes| {
                picture_view(bytes, output.coded_width as usize, output, time)
                    .map(|p| use_picture(&p))
            })
            .flatten(),
    }
}

fn picture_rows(output: &VideoOutput) -> usize {
    let coded_rows = output.coded_height as usize;
    coded_rows + coded_rows.div_ceil(2)
}

fn picture_view<'a>(
    bytes: &'a [u8],
    stride: usize,
    output: &VideoOutput,
    time: Option<i64>,
) -> Option<Picture<'a>> {
    let split = stride * output.coded_height as usize;
    (bytes.len() >= stride * picture_rows(output)).then(|| Picture {
        luma: &bytes[..split],
        chroma: &bytes[split..],
        stride,
        width: output.visible_width,
        height: output.visible_height,
        time,
    })
}

pub struct AudioDecoder {
    transform: Transform,
    rate: u32,
    channels: u32,
    float: bool,
    buffer_size: u32,
    provides_samples: bool,
}

impl AudioDecoder {
    pub fn aac(config: &[u8], rate: u32, channels: u32) -> Option<AudioDecoder> {
        let (core, parsed) = aac_core_config(config)?;
        let rate = if parsed.rate > 0 { parsed.rate } else { rate };
        let channels = if parsed.channels > 0 {
            parsed.channels
        } else {
            channels
        };
        if rate == 0 || !(1..=MAX_AAC_CHANNELS).contains(&channels) {
            return None;
        }
        let transform = Transform::find(
            &com::CATEGORY_AUDIO_DECODER,
            &com::MEDIA_TYPE_AUDIO,
            &com::AUDIO_FORMAT_AAC,
        )?;
        let input = MediaType::new()?;
        input.set_guid(&com::MT_MAJOR_TYPE, &com::MEDIA_TYPE_AUDIO);
        input.set_guid(&com::MT_SUBTYPE, &com::AUDIO_FORMAT_AAC);
        input.set_u32(&com::MT_AUDIO_SAMPLES_PER_SECOND, rate);
        input.set_u32(&com::MT_AUDIO_NUM_CHANNELS, channels);
        input.set_u32(&com::MT_AAC_PAYLOAD_TYPE, 0);
        let mut user_data = vec![0u8; AAC_USER_DATA_HEADER];
        user_data.extend_from_slice(&core);
        input.set_blob(&com::MT_USER_DATA, &user_data);
        if !transform.set_input_type(&input) {
            return None;
        }
        let mut decoder = AudioDecoder {
            transform,
            rate,
            channels,
            float: false,
            buffer_size: DEFAULT_AUDIO_BUFFER,
            provides_samples: false,
        };
        if !decoder.pick_output() {
            return None;
        }
        start_streaming(&decoder.transform);
        Some(decoder)
    }

    fn pick_output(&mut self) -> bool {
        let mut pcm16 = None;
        let mut float = None;
        for candidate in (0..).map_while(|index| self.transform.output_type(index)) {
            match candidate.get_guid(&com::MT_SUBTYPE) {
                Some(subtype) if subtype == com::AUDIO_FORMAT_FLOAT => {
                    float = Some(candidate);
                    break;
                }
                Some(subtype)
                    if subtype == com::AUDIO_FORMAT_PCM
                        && pcm16.is_none()
                        && candidate.get_u32(&com::MT_AUDIO_BITS_PER_SAMPLE) == Some(16) =>
                {
                    pcm16 = Some(candidate);
                }
                _ => {}
            }
        }
        let is_float = float.is_some();
        let Some(chosen) = float
            .or(pcm16)
            .filter(|chosen| self.transform.set_output_type(chosen))
        else {
            return false;
        };
        self.float = is_float;
        if let Some(rate) = chosen
            .get_u32(&com::MT_AUDIO_SAMPLES_PER_SECOND)
            .filter(|&rate| rate > 0)
        {
            self.rate = rate;
        }
        if let Some(channels) = chosen
            .get_u32(&com::MT_AUDIO_NUM_CHANNELS)
            .filter(|&channels| channels > 0)
        {
            self.channels = channels;
        }
        let info = self.transform.output_stream_info();
        self.buffer_size = if info.size > 0 {
            info.size
        } else {
            DEFAULT_AUDIO_BUFFER
        };
        self.provides_samples = info.flags & com::OUTPUT_PROVIDES_SAMPLES != 0;
        true
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    pub fn send(&mut self, packet: &[u8], time: Option<i64>) -> Sent {
        match input_sample(&[packet], time) {
            Some(sample) => feed(&self.transform, &sample),
            None => Sent::Failed,
        }
    }

    pub fn receive<R>(&mut self, use_pcm: impl FnOnce(&Pcm<'_>) -> R) -> Received<R> {
        for _ in 0..OUTPUT_ATTEMPTS {
            match pull(&self.transform, self.provides_samples, self.buffer_size) {
                Output::Changed => {
                    if !self.pick_output() {
                        return Received::Failed;
                    }
                }
                Output::NeedInput => return Received::NeedInput,
                Output::Failed => return Received::Failed,
                Output::Sample(sample) => {
                    let time = sample.time();
                    let pcm = sample.contiguous().and_then(|buffer| {
                        buffer.read(|bytes| {
                            use_pcm(&Pcm {
                                bytes,
                                float: self.float,
                                rate: self.rate,
                                channels: self.channels,
                                time,
                            })
                        })
                    });
                    return pcm.map_or(Received::Failed, Received::Output);
                }
            }
        }
        Received::Failed
    }

    pub fn drain(&mut self) {
        self.transform.message(com::MESSAGE_DRAIN);
    }

    pub fn flush(&mut self) {
        restart(&self.transform);
    }
}

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        self.transform.message(com::MESSAGE_END_STREAMING);
    }
}
