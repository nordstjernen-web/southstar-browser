//! Southstar — one video stream demuxed and decoded by libav from a file that may still be growing, scaled to BGRA on demand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::ffi::CString;

use super::os;
pub use southstar_av::quiet_logging;
use southstar_av::{self as av, AVRational, codec_parameters, frame, stream};
use southstar_av::{Codec, CodecDescription, Format, Frame, Packet, Scaler};

const MAX_DIMENSION: c_int = 4096;

pub enum Next {
    Frame,
    Grew,
    Idle,
}

pub struct Decoder {
    scaler: Scaler,
    codec: Codec,
    format: Format<'static>,
    packet: Packet,
    frame: Frame,
    stream: c_int,
    time_base: AVRational,
    known_size: i64,
    draining: bool,
    pub width: u32,
    pub height: u32,
    pub default_duration: f64,
}

impl Decoder {
    pub fn open(path: &[u8]) -> Option<(Decoder, f64)> {
        let c_path = CString::new(path).ok()?;
        let mut format = Format::open_file(&c_path)?;
        if !format.find_stream_info() {
            return None;
        }
        let index = format.best_stream(av::AVMEDIA_TYPE_VIDEO)?;
        let video = format.stream(index)?;
        let params = video.parameters()?;
        let decoder = CodecDescription::find(params.get(&codec_parameters::CODEC_ID))?;
        let codec = Codec::open(&params, decoder)?;
        let width = params.get(&codec_parameters::WIDTH);
        let height = params.get(&codec_parameters::HEIGHT);
        if width <= 0 || height <= 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return None;
        }
        let time_base = video.get(&stream::TIME_BASE);
        let average = video.get(&stream::AVG_FRAME_RATE);
        let rate = if average.num > 0 && average.den > 0 {
            average
        } else {
            video.get(&stream::R_FRAME_RATE)
        };
        let mut default_duration = if rate.num > 0 && rate.den > 0 {
            rate.inverse().to_f64()
        } else {
            1.0 / 30.0
        };
        if default_duration <= 0.0 || default_duration > 1.0 {
            default_duration = 1.0 / 30.0;
        }
        let container_duration = format.duration();
        let stream_duration = video.get(&stream::DURATION);
        let duration = if container_duration > 0 {
            container_duration as f64 / av::AV_TIME_BASE as f64
        } else if stream_duration > 0 {
            stream_duration as f64 * time_base.to_f64()
        } else {
            0.0
        };
        let packet = Packet::new()?;
        let frame = Frame::new()?;
        let decoder = Decoder {
            scaler: Scaler::default(),
            codec,
            format,
            packet,
            frame,
            stream: index,
            time_base,
            known_size: os::file_size(path),
            draining: false,
            width: width as u32,
            height: height as u32,
            default_duration,
        };
        Some((decoder, duration))
    }

    fn file_grew(&self, path: &[u8]) -> bool {
        os::file_size(path) > self.known_size
    }

    pub fn next_frame(&mut self, path: &[u8]) -> Next {
        loop {
            let received = self.codec.receive(&mut self.frame);
            if received == 0 {
                return Next::Frame;
            }
            if self.draining {
                return if self.file_grew(path) {
                    Next::Grew
                } else {
                    Next::Idle
                };
            }
            if received != av::AVERROR_EAGAIN && received != av::AVERROR_EOF {
                return Next::Idle;
            }
            loop {
                if !self.format.read_packet(&mut self.packet) {
                    if self.file_grew(path) {
                        return Next::Grew;
                    }
                    self.codec.send(None);
                    self.draining = true;
                    break;
                }
                if self.packet.stream_index() != self.stream {
                    self.packet.unref();
                    continue;
                }
                let sent = self.codec.send(Some(&self.packet));
                self.packet.unref();
                if sent < 0 && sent != av::AVERROR_EAGAIN {
                    return Next::Idle;
                }
                break;
            }
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        let timestamp = (seconds / self.time_base.to_f64()) as i64;
        self.format
            .seek(self.stream, timestamp, av::AVSEEK_FLAG_BACKWARD);
        self.codec.flush();
        self.draining = false;
    }

    pub fn frame_pts(&self, fallback: f64) -> f64 {
        let timestamp = self.frame.get(&frame::BEST_EFFORT_TIMESTAMP);
        if timestamp != av::AV_NOPTS_VALUE {
            timestamp as f64 * self.time_base.to_f64()
        } else {
            fallback
        }
    }

    pub fn unref_frame(&mut self) {
        self.frame.unref();
    }

    pub fn prepare_scaler(&mut self, width: u32, height: u32) -> bool {
        self.scaler.prepare(&self.frame, width, height)
    }

    pub fn scale_into(&mut self, target: &southstar_media_shm::Frame) {
        // SAFETY: the ring's frame() checked that target holds height rows of stride bytes, each with width * 4 pixel bytes, inside the mapping; the slot is unpublished while it is written.
        unsafe {
            self.scaler.scale_to(
                &self.frame,
                target.as_ptr(),
                target.stride as usize,
                target.width,
                target.height,
            )
        };
    }
}
