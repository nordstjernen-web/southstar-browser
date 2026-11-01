//! Southstar — the operating system's H.264 and AAC decoders behind libav's send/receive contract, filling NV12 and float AVFrames.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::NonNull;
use std::collections::VecDeque;

use southstar_media_foundation::{AudioDecoder, Pcm, Picture, Received, Sent, VideoDecoder};

use crate::decode::{Frame, Packet};
use crate::demux::Parameters;
use crate::{self as av, AVChannelLayout, AVRational, codec_parameters, frame};

const TICKS_PER_SECOND: i128 = 10_000_000;
const SEND_ATTEMPTS: usize = 16;
const FRAME_ALIGN: c_int = 32;
const MAX_DIMENSION: c_int = 8192;

pub(crate) fn preferred() -> bool {
    std::env::var_os("NS_MEDIA_NO_OS_DECODER").is_none()
}

pub(crate) fn supports(codec_id: c_int) -> bool {
    if !preferred() {
        return false;
    }
    if codec_id == av::AV_CODEC_ID_H264 {
        southstar_media_foundation::h264_available()
    } else if codec_id == av::AV_CODEC_ID_AAC {
        southstar_media_foundation::aac_available()
    } else {
        false
    }
}

pub(crate) struct ChannelLayout(Box<[u64]>);

impl ChannelLayout {
    pub(crate) fn with_channels(channels: c_int) -> ChannelLayout {
        let words = av::SIZEOF_AV_CHANNEL_LAYOUT.div_ceil(8).max(1);
        let layout = ChannelLayout(vec![0u64; words].into_boxed_slice());
        // SAFETY: the zeroed, 8-byte aligned buffer holds at least sizeof(AVChannelLayout) bytes for libav to fill.
        unsafe { av::av_channel_layout_default(layout.as_ptr().cast_mut(), channels) };
        layout
    }

    pub(crate) fn as_ptr(&self) -> *const AVChannelLayout {
        self.0.as_ptr().cast()
    }
}

impl Drop for ChannelLayout {
    fn drop(&mut self) {
        // SAFETY: the buffer holds a channel layout av_channel_layout_default initialized, uninitialized once.
        unsafe { av::av_channel_layout_uninit(self.0.as_mut_ptr().cast()) };
    }
}

enum Kind {
    Video(VideoDecoder),
    Audio(AudioDecoder, ChannelLayout),
}

pub struct OsDecoder {
    kind: Kind,
    time_base: AVRational,
    pending: VecDeque<Frame>,
    draining: bool,
}

enum Pulled {
    Frame,
    NeedInput,
    Failed,
}

impl OsDecoder {
    pub(crate) fn open(parameters: &Parameters, time_base: AVRational) -> Option<OsDecoder> {
        let codec_id = parameters.get(&codec_parameters::CODEC_ID);
        if !supports(codec_id) || time_base.num <= 0 || time_base.den <= 0 {
            return None;
        }
        let extradata = parameters.extradata();
        let kind = if codec_id == av::AV_CODEC_ID_H264 {
            let width = parameters.get(&codec_parameters::WIDTH);
            let height = parameters.get(&codec_parameters::HEIGHT);
            if extradata.is_empty()
                || !(0..=MAX_DIMENSION).contains(&width)
                || !(0..=MAX_DIMENSION).contains(&height)
            {
                return None;
            }
            Kind::Video(VideoDecoder::h264(extradata, width as u32, height as u32)?)
        } else {
            let rate = parameters.get(&codec_parameters::SAMPLE_RATE).max(0) as u32;
            let channels = parameters.channels().max(0) as u32;
            let decoder = AudioDecoder::aac(extradata, rate, channels)?;
            let layout = ChannelLayout::with_channels(decoder.channels() as c_int);
            Kind::Audio(decoder, layout)
        };
        Some(OsDecoder {
            kind,
            time_base,
            pending: VecDeque::new(),
            draining: false,
        })
    }

    pub(crate) fn audio_format(&self) -> Option<(c_int, c_int, *const AVChannelLayout)> {
        match &self.kind {
            Kind::Audio(decoder, layout) => Some((
                decoder.rate() as c_int,
                decoder.channels() as c_int,
                layout.as_ptr(),
            )),
            Kind::Video(_) => None,
        }
    }

    fn to_ticks(&self, pts: i64) -> Option<i64> {
        if pts == av::AV_NOPTS_VALUE {
            return None;
        }
        let ticks = rescale(
            i128::from(pts),
            i128::from(self.time_base.num) * TICKS_PER_SECOND,
            i128::from(self.time_base.den),
        );
        i64::try_from(ticks).ok()
    }

    fn pts_of(&self, ticks: Option<i64>) -> i64 {
        ticks
            .and_then(|ticks| {
                let pts = rescale(
                    i128::from(ticks),
                    i128::from(self.time_base.den),
                    i128::from(self.time_base.num) * TICKS_PER_SECOND,
                );
                i64::try_from(pts).ok()
            })
            .unwrap_or(av::AV_NOPTS_VALUE)
    }

    pub(crate) fn send(&mut self, packet: Option<&Packet>) -> c_int {
        let Some(packet) = packet else {
            match &mut self.kind {
                Kind::Video(decoder) => decoder.drain(),
                Kind::Audio(decoder, _) => decoder.drain(),
            }
            self.draining = true;
            return 0;
        };
        let data = packet.data();
        let time = self.to_ticks(packet.pts());
        for _ in 0..SEND_ATTEMPTS {
            let sent = match &mut self.kind {
                Kind::Video(decoder) => decoder.send(data, time),
                Kind::Audio(decoder, _) => decoder.send(data, time),
            };
            match sent {
                Sent::Accepted => return 0,
                Sent::Failed => return -1,
                Sent::Full => {
                    let Some(mut frame) = Frame::new() else {
                        return -1;
                    };
                    match self.pull(&mut frame) {
                        Pulled::Frame => self.pending.push_back(frame),
                        Pulled::NeedInput | Pulled::Failed => return -1,
                    }
                }
            }
        }
        -1
    }

    pub(crate) fn receive(&mut self, target: &mut Frame) -> c_int {
        if let Some(mut queued) = self.pending.pop_front() {
            target.take_from(&mut queued);
            return 0;
        }
        match self.pull(target) {
            Pulled::Frame => 0,
            Pulled::NeedInput if self.draining => av::AVERROR_EOF,
            Pulled::NeedInput => av::AVERROR_EAGAIN,
            Pulled::Failed => -1,
        }
    }

    fn pull(&mut self, target: &mut Frame) -> Pulled {
        let received = match &mut self.kind {
            Kind::Video(decoder) => decoder.receive(|picture| fill_picture(target, picture)),
            Kind::Audio(decoder, _) => decoder.receive(|pcm| fill_pcm(target, pcm)),
        };
        match received {
            Received::Output(Some(ticks)) => {
                let pts = self.pts_of(ticks);
                target.set(&frame::PTS, pts);
                target.set(&frame::BEST_EFFORT_TIMESTAMP, pts);
                Pulled::Frame
            }
            Received::Output(None) | Received::Failed => Pulled::Failed,
            Received::NeedInput => Pulled::NeedInput,
        }
    }

    pub(crate) fn flush(&mut self) {
        match &mut self.kind {
            Kind::Video(decoder) => decoder.flush(),
            Kind::Audio(decoder, _) => decoder.flush(),
        }
        self.pending.clear();
        self.draining = false;
    }
}

fn rescale(value: i128, multiply: i128, divide: i128) -> i128 {
    let product = value * multiply;
    let half = divide / 2;
    if product >= 0 {
        (product + half) / divide
    } else {
        (product - half) / divide
    }
}

fn fill_picture(target: &mut Frame, picture: &Picture<'_>) -> Option<Option<i64>> {
    let width = c_int::try_from(picture.width).ok()?;
    let height = c_int::try_from(picture.height).ok()?;
    target.unref();
    target.set(&frame::FORMAT, av::AV_PIX_FMT_NV12);
    target.set(&frame::WIDTH, width);
    target.set(&frame::HEIGHT, height);
    if !target.allocate(FRAME_ALIGN) {
        return None;
    }
    let luma_rows = picture.height as usize;
    let chroma_rows = luma_rows.div_ceil(2);
    let mut luma = target.plane(0, luma_rows)?;
    for row in 0..luma_rows {
        let source = picture.luma_row(row)?;
        luma.row(row)?[..source.len()].copy_from_slice(source);
    }
    let mut chroma = target.plane(1, chroma_rows)?;
    for row in 0..chroma_rows {
        let source = picture.chroma_row(row)?;
        let destination = chroma.row(row)?;
        let length = source.len().min(destination.len());
        destination[..length].copy_from_slice(&source[..length]);
    }
    Some(picture.time)
}

fn fill_pcm(target: &mut Frame, pcm: &Pcm<'_>) -> Option<Option<i64>> {
    let frames = pcm.frames();
    let channels = c_int::try_from(pcm.channels).ok()?;
    if frames == 0 || channels <= 0 {
        return None;
    }
    target.unref();
    target.set(&frame::FORMAT, av::AV_SAMPLE_FMT_FLT);
    target.set(&frame::NB_SAMPLES, c_int::try_from(frames).ok()?);
    target.set(&frame::SAMPLE_RATE, c_int::try_from(pcm.rate).ok()?);
    target.set_default_layout(channels);
    if !target.allocate(0) {
        return None;
    }
    let floats = target.float_samples(frames * channels as usize)?;
    pcm.write_floats(floats);
    Some(pcm.time)
}

pub(crate) struct Plane<'a> {
    base: NonNull<u8>,
    stride: usize,
    rows: usize,
    marker: core::marker::PhantomData<&'a mut [u8]>,
}

impl<'a> Plane<'a> {
    pub(crate) fn new(base: NonNull<u8>, stride: usize, rows: usize) -> Plane<'a> {
        Plane {
            base,
            stride,
            rows,
            marker: core::marker::PhantomData,
        }
    }

    fn row(&mut self, row: usize) -> Option<&mut [u8]> {
        if row >= self.rows {
            return None;
        }
        let start = self.base.as_ptr().wrapping_add(row * self.stride);
        // SAFETY: the plane was allocated by av_frame_get_buffer with rows rows of stride bytes, and the frame stays exclusively borrowed for 'a.
        Some(unsafe { core::slice::from_raw_parts_mut(start, self.stride) })
    }
}
