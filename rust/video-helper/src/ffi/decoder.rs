//! Southstar — one video stream demuxed and decoded by libav from a file that may still be growing, scaled to BGRA on demand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;
use std::ffi::CString;

use southstar_av::{
    self as av, AVCodecContext, AVFormatContext, AVFrame, AVPacket, AVRational, AVStream,
    SwsContext, codec_parameters, format_context, frame, packet, stream,
};

use super::os;

const MAX_DIMENSION: c_int = 4096;

pub enum Next {
    Frame,
    Grew,
    Idle,
}

pub struct Decoder {
    format: *mut AVFormatContext,
    codec: *mut AVCodecContext,
    scaler: *mut SwsContext,
    packet: *mut AVPacket,
    frame: *mut AVFrame,
    stream: c_int,
    time_base: AVRational,
    known_size: i64,
    draining: bool,
    pub width: u32,
    pub height: u32,
    pub default_duration: f64,
}

pub fn quiet_logging() {
    unsafe { av::av_log_set_level(av::AV_LOG_QUIET) };
}

impl Decoder {
    pub fn open(path: &[u8]) -> Option<(Decoder, f64)> {
        let c_path = CString::new(path).ok()?;
        let mut decoder = Decoder {
            format: ptr::null_mut(),
            codec: ptr::null_mut(),
            scaler: ptr::null_mut(),
            packet: ptr::null_mut(),
            frame: ptr::null_mut(),
            stream: -1,
            time_base: AVRational::default(),
            known_size: 0,
            draining: false,
            width: 0,
            height: 0,
            default_duration: 0.0,
        };
        let mut options = ptr::null_mut();
        let rc = unsafe {
            av::av_dict_set(
                &mut options,
                c"protocol_whitelist".as_ptr(),
                c"file".as_ptr(),
                0,
            );
            let rc = av::avformat_open_input(
                &mut decoder.format,
                c_path.as_ptr(),
                ptr::null(),
                &mut options,
            );
            av::av_dict_free(&mut options);
            rc
        };
        if rc < 0 || unsafe { av::avformat_find_stream_info(decoder.format, ptr::null_mut()) } < 0 {
            return None;
        }
        decoder.stream = unsafe {
            av::av_find_best_stream(
                decoder.format,
                av::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            )
        };
        if decoder.stream < 0 {
            return None;
        }
        let video = decoder.video_stream();
        let params = unsafe { stream::CODECPAR.get(video) };
        let codec = unsafe { av::avcodec_find_decoder(codec_parameters::CODEC_ID.get(params)) };
        if codec.is_null() {
            return None;
        }
        decoder.codec = unsafe { av::avcodec_alloc_context3(codec) };
        if decoder.codec.is_null()
            || unsafe { av::avcodec_parameters_to_context(decoder.codec, params) } < 0
            || unsafe { av::avcodec_open2(decoder.codec, codec, ptr::null_mut()) } < 0
        {
            return None;
        }
        let width = unsafe { codec_parameters::WIDTH.get(params) };
        let height = unsafe { codec_parameters::HEIGHT.get(params) };
        if width <= 0 || height <= 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return None;
        }
        decoder.width = width as u32;
        decoder.height = height as u32;
        decoder.time_base = unsafe { stream::TIME_BASE.get(video) };
        decoder.packet = unsafe { av::av_packet_alloc() };
        decoder.frame = unsafe { av::av_frame_alloc() };
        if decoder.packet.is_null() || decoder.frame.is_null() {
            return None;
        }
        decoder.known_size = os::file_size(path);
        let average = unsafe { stream::AVG_FRAME_RATE.get(video) };
        let rate = if average.num > 0 && average.den > 0 {
            average
        } else {
            unsafe { stream::R_FRAME_RATE.get(video) }
        };
        decoder.default_duration = if rate.num > 0 && rate.den > 0 {
            rate.inverse().to_f64()
        } else {
            1.0 / 30.0
        };
        if decoder.default_duration <= 0.0 || decoder.default_duration > 1.0 {
            decoder.default_duration = 1.0 / 30.0;
        }
        let container_duration = unsafe { format_context::DURATION.get(decoder.format) };
        let stream_duration = unsafe { stream::DURATION.get(video) };
        let duration = if container_duration > 0 {
            container_duration as f64 / av::AV_TIME_BASE as f64
        } else if stream_duration > 0 {
            stream_duration as f64 * decoder.time_base.to_f64()
        } else {
            0.0
        };
        Some((decoder, duration))
    }

    fn video_stream(&self) -> *mut AVStream {
        unsafe {
            *format_context::STREAMS
                .get(self.format)
                .add(self.stream as usize)
        }
    }

    fn file_grew(&self, path: &[u8]) -> bool {
        os::file_size(path) > self.known_size
    }

    pub fn next_frame(&mut self, path: &[u8]) -> Next {
        loop {
            let received = unsafe { av::avcodec_receive_frame(self.codec, self.frame) };
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
                if unsafe { av::av_read_frame(self.format, self.packet) } < 0 {
                    if self.file_grew(path) {
                        return Next::Grew;
                    }
                    unsafe { av::avcodec_send_packet(self.codec, ptr::null()) };
                    self.draining = true;
                    break;
                }
                if unsafe { packet::STREAM_INDEX.get(self.packet) } != self.stream {
                    unsafe { av::av_packet_unref(self.packet) };
                    continue;
                }
                let sent = unsafe { av::avcodec_send_packet(self.codec, self.packet) };
                unsafe { av::av_packet_unref(self.packet) };
                if sent < 0 && sent != av::AVERROR_EAGAIN {
                    return Next::Idle;
                }
                break;
            }
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        let timestamp = (seconds / self.time_base.to_f64()) as i64;
        unsafe {
            av::av_seek_frame(
                self.format,
                self.stream,
                timestamp,
                av::AVSEEK_FLAG_BACKWARD,
            );
            av::avcodec_flush_buffers(self.codec);
        }
        self.draining = false;
    }

    pub fn frame_pts(&self, fallback: f64) -> f64 {
        let timestamp = unsafe { frame::BEST_EFFORT_TIMESTAMP.get(self.frame) };
        if timestamp != av::AV_NOPTS_VALUE {
            timestamp as f64 * self.time_base.to_f64()
        } else {
            fallback
        }
    }

    pub fn unref_frame(&mut self) {
        unsafe { av::av_frame_unref(self.frame) };
    }

    pub fn prepare_scaler(&mut self, width: u32, height: u32) -> bool {
        self.scaler = unsafe {
            av::sws_getCachedContext(
                self.scaler,
                frame::WIDTH.get(self.frame),
                frame::HEIGHT.get(self.frame),
                frame::FORMAT.get(self.frame),
                width as c_int,
                height as c_int,
                av::AV_PIX_FMT_BGRA,
                av::SWS_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        !self.scaler.is_null()
    }

    pub unsafe fn scale_into(&mut self, dst: *mut u8, stride: u32) {
        let data = unsafe { frame::DATA.get(self.frame) };
        let linesize = unsafe { frame::LINESIZE.get(self.frame) };
        let dst_planes = [dst, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()];
        let dst_strides: [c_int; 4] = [stride as c_int, 0, 0, 0];
        unsafe {
            av::sws_scale(
                self.scaler,
                data.as_ptr().cast(),
                linesize.as_ptr(),
                0,
                frame::HEIGHT.get(self.frame),
                dst_planes.as_ptr(),
                dst_strides.as_ptr(),
            );
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            if !self.scaler.is_null() {
                av::sws_freeContext(self.scaler);
            }
            if !self.codec.is_null() {
                av::avcodec_free_context(&mut self.codec);
            }
            if !self.format.is_null() {
                av::avformat_close_input(&mut self.format);
            }
            if !self.packet.is_null() {
                av::av_packet_free(&mut self.packet);
            }
            if !self.frame.is_null() {
                av::av_frame_free(&mut self.frame);
            }
        }
    }
}
