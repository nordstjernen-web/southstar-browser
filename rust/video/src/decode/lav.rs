//! Southstar — the libav backend: Matroska/WebM and ISO-BMFF demuxed from a growing in-memory buffer, decoded and scaled to BGRA.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr::{self, null_mut};

use southstar_av::{self as av, AVCodecContext, AVFormatContext, AVFrame, AVIOContext};
use southstar_av::{AVPacket, AVRational, AVStream, SwsContext};
use southstar_av::{codec_parameters, format_context, frame, io_context, packet, stream};

use super::{Next, copy_bytes, pixel_buffer};
use crate::ffi::texture_from_bgra;

const IO_BUFFER_SIZE: c_int = 32768;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;

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
    av::available() && !unsafe { av::avcodec_find_decoder(codec.id()) }.is_null()
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

struct Source {
    data: Vec<u8>,
    pos: usize,
}

unsafe extern "C" fn source_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    let src = unsafe { &mut *opaque.cast::<Source>() };
    let avail = src.data.len() - src.pos;
    if avail == 0 {
        return av::AVERROR_EOF;
    }
    let n = (size.max(0) as usize).min(avail);
    unsafe { ptr::copy_nonoverlapping(src.data.as_ptr().add(src.pos), buf, n) };
    src.pos += n;
    n as c_int
}

unsafe extern "C" fn source_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    let src = unsafe { &mut *opaque.cast::<Source>() };
    let whence = whence & !av::AVSEEK_FORCE;
    let len = src.data.len() as i64;
    if whence == av::AVSEEK_SIZE {
        return len;
    }
    let base = match whence {
        SEEK_CUR => src.pos as i64,
        SEEK_END => len,
        _ => 0,
    };
    let target = base + offset;
    if target < 0 || target > len {
        return -1;
    }
    src.pos = target as usize;
    target
}

struct Io {
    avio: *mut AVIOContext,
    source: *mut Source,
}

impl Io {
    fn new(data: Vec<u8>) -> Option<Io> {
        let source = Box::into_raw(Box::new(Source { data, pos: 0 }));
        let mut io = Io {
            avio: null_mut(),
            source,
        };
        let buffer = unsafe { av::av_malloc(IO_BUFFER_SIZE as usize) }.cast::<u8>();
        if buffer.is_null() {
            return None;
        }
        io.avio = unsafe {
            av::avio_alloc_context(
                buffer,
                IO_BUFFER_SIZE,
                0,
                source.cast(),
                Some(source_read),
                None,
                Some(source_seek),
            )
        };
        if io.avio.is_null() {
            unsafe { av::av_free(buffer.cast()) };
            return None;
        }
        Some(io)
    }

    fn source(&mut self) -> &mut Source {
        unsafe { &mut *self.source }
    }
}

impl Drop for Io {
    fn drop(&mut self) {
        unsafe {
            if !self.avio.is_null() {
                av::av_freep(io_context::BUFFER.ptr(self.avio).cast());
                av::avio_context_free(&mut self.avio);
            }
            drop(Box::from_raw(self.source));
        }
    }
}

struct Input {
    fmt: *mut AVFormatContext,
    io: Io,
}

impl Input {
    fn open(data: Vec<u8>) -> Option<Input> {
        let io = Io::new(data)?;
        let mut fmt = unsafe { av::avformat_alloc_context() };
        if fmt.is_null() {
            return None;
        }
        unsafe { format_context::PB.set(fmt, io.avio) };
        if unsafe { av::avformat_open_input(&mut fmt, ptr::null(), ptr::null(), null_mut()) } < 0 {
            return None;
        }
        Some(Input { fmt, io })
    }

    fn stream(&self, index: c_int) -> *mut AVStream {
        unsafe { *format_context::STREAMS.get(self.fmt).add(index as usize) }
    }

    fn stream_count(&self) -> c_int {
        unsafe { format_context::NB_STREAMS.get(self.fmt) as c_int }
    }

    fn read(&mut self, pkt: &Packet) -> bool {
        unsafe { av::av_read_frame(self.fmt, pkt.0) >= 0 }
    }

    fn seek(&mut self, stream_index: c_int, ts: i64) -> bool {
        unsafe { av::av_seek_frame(self.fmt, stream_index, ts, av::AVSEEK_FLAG_BACKWARD) >= 0 }
    }

    fn best_stream(&self, media_type: c_int) -> c_int {
        unsafe { av::av_find_best_stream(self.fmt, media_type, -1, -1, null_mut(), 0) }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        unsafe { av::avformat_close_input(&mut self.fmt) };
    }
}

struct Packet(*mut AVPacket);

impl Packet {
    fn alloc() -> Option<Packet> {
        let p = unsafe { av::av_packet_alloc() };
        (!p.is_null()).then(|| Packet(p))
    }

    fn stream_index(&self) -> c_int {
        unsafe { packet::STREAM_INDEX.get(self.0) }
    }

    fn pts(&self) -> i64 {
        unsafe { packet::PTS.get(self.0) }
    }

    fn duration(&self) -> i64 {
        unsafe { packet::DURATION.get(self.0) }
    }

    fn unref(&self) {
        unsafe { av::av_packet_unref(self.0) };
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe { av::av_packet_free(&mut self.0) };
    }
}

struct Frame(*mut AVFrame);

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe { av::av_frame_free(&mut self.0) };
    }
}

struct CodecContext(*mut AVCodecContext);

impl Drop for CodecContext {
    fn drop(&mut self) {
        unsafe { av::avcodec_free_context(&mut self.0) };
    }
}

struct Scaler(*mut SwsContext);

impl Drop for Scaler {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { av::sws_freeContext(self.0) };
        }
    }
}

pub struct Demuxer {
    sws: Scaler,
    frame: Frame,
    pkt: Packet,
    vdec: CodecContext,
    input: Input,
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
        unsafe { av::av_log_set_level(av::AV_LOG_QUIET) };
        let mut input = Input::open(copy_bytes(bytes)?)?;
        if unsafe { av::avformat_find_stream_info(input.fmt, null_mut()) } < 0 {
            return None;
        }
        let mut dec = ptr::null();
        let vstream = unsafe {
            av::av_find_best_stream(input.fmt, av::AVMEDIA_TYPE_VIDEO, -1, -1, &mut dec, 0)
        };
        if vstream < 0 || dec.is_null() {
            return None;
        }
        let vs = input.stream(vstream);
        let par = unsafe { stream::CODECPAR.get(vs) };
        let id = unsafe { codec_parameters::CODEC_ID.get(par) };
        if !VIDEO_CODECS.contains(&id) {
            return None;
        }
        let vdec = CodecContext(unsafe { av::avcodec_alloc_context3(dec) });
        if vdec.0.is_null() {
            return None;
        }
        if unsafe { av::avcodec_parameters_to_context(vdec.0, par) } < 0
            || unsafe { av::avcodec_open2(vdec.0, dec, null_mut()) } < 0
        {
            return None;
        }
        let (width, height) = unsafe {
            (
                codec_parameters::WIDTH.get(par),
                codec_parameters::HEIGHT.get(par),
            )
        };
        if !super::dimensions_valid(width, height) {
            return None;
        }
        let time_base = unsafe { stream::TIME_BASE.get(vs) };
        let pkt = Packet::alloc()?;
        let frame = Frame(unsafe { av::av_frame_alloc() });
        if frame.0.is_null() {
            return None;
        }

        let fmt_duration = unsafe { format_context::DURATION.get(input.fmt) };
        let stream_duration = unsafe { stream::DURATION.get(vs) };
        let duration = if fmt_duration > 0 {
            fmt_duration as f64 / av::AV_TIME_BASE as f64
        } else if stream_duration > 0 {
            stream_duration as f64 * time_base.to_f64()
        } else {
            0.0
        };

        let mut last_pts = 0.0;
        while input.read(&pkt) {
            if pkt.stream_index() == vstream && pkt.pts() != av::AV_NOPTS_VALUE {
                let t = pkt.pts() as f64 * time_base.to_f64();
                if t > last_pts {
                    last_pts = t;
                }
            }
            pkt.unref();
        }
        input.seek(vstream, 0);
        let has_audio = input.best_stream(av::AVMEDIA_TYPE_AUDIO) >= 0;

        Some(Opened {
            demuxer: Demuxer {
                sws: Scaler(null_mut()),
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

    pub fn rewind(&mut self) {
        self.input.seek(self.vstream, 0);
        unsafe { av::avcodec_flush_buffers(self.vdec.0) };
        self.draining = false;
    }

    pub fn seek_to(&mut self, seconds: f64) -> bool {
        let ts = (seconds / self.time_base.to_f64()) as i64;
        if !self.input.seek(self.vstream, ts) {
            return false;
        }
        unsafe { av::avcodec_flush_buffers(self.vdec.0) };
        self.draining = false;
        true
    }

    pub fn extend(&mut self, bytes: &[u8]) -> bool {
        let src = self.input.io.source();
        let cur = src.data.len();
        if bytes.len() < cur {
            return false;
        }
        if bytes.len() == cur {
            return true;
        }
        let head = cur.min(4096);
        if bytes[..head] != src.data[..head] {
            return false;
        }
        if cur > 4096 && bytes[cur - 4096..cur] != src.data[cur - 4096..] {
            return false;
        }
        let extra = &bytes[cur..];
        if src.data.try_reserve(extra.len()).is_err() {
            return false;
        }
        src.data.extend_from_slice(extra);
        let avio = self.input.io.avio;
        unsafe {
            io_context::EOF_REACHED.set(avio, 0);
            io_context::ERROR.set(avio, 0);
        }
        if self.draining {
            unsafe { av::avcodec_flush_buffers(self.vdec.0) };
            self.draining = false;
        }
        true
    }

    fn frame_to_texture(&mut self, w: i32, h: i32) -> Option<crate::ffi::Texture> {
        let stride = w as usize * 4;
        let mut bgra = pixel_buffer(stride * h as usize, 0)?;
        let f = self.frame.0;
        let (fw, fh, format, data, linesize) = unsafe {
            (
                frame::WIDTH.get(f),
                frame::HEIGHT.get(f),
                frame::FORMAT.get(f),
                frame::DATA.get(f),
                frame::LINESIZE.get(f),
            )
        };
        self.sws.0 = unsafe {
            av::sws_getCachedContext(
                self.sws.0,
                fw,
                fh,
                format,
                w,
                h,
                av::AV_PIX_FMT_BGRA,
                av::SWS_BILINEAR,
                null_mut(),
                null_mut(),
                ptr::null(),
            )
        };
        if self.sws.0.is_null() {
            return None;
        }
        let dst = [bgra.as_mut_ptr(), null_mut(), null_mut(), null_mut()];
        let dst_stride = [stride as c_int, 0, 0, 0];
        unsafe {
            av::sws_scale(
                self.sws.0,
                data.as_ptr().cast(),
                linesize.as_ptr(),
                0,
                fh,
                dst.as_ptr(),
                dst_stride.as_ptr(),
            )
        };
        texture_from_bgra(w, h, &bgra, stride)
    }

    pub fn next(&mut self, w: i32, h: i32, fallback_time: f64) -> Next {
        loop {
            let r = unsafe { av::avcodec_receive_frame(self.vdec.0, self.frame.0) };
            if r == 0 {
                let pts = unsafe { frame::PTS.get(self.frame.0) };
                let t = if pts == av::AV_NOPTS_VALUE {
                    fallback_time
                } else {
                    pts as f64 * self.time_base.to_f64()
                };
                let tex = self.frame_to_texture(w, h);
                unsafe { av::av_frame_unref(self.frame.0) };
                return Next::Frame(tex, t);
            }
            if r == av::AVERROR_EOF || (r < 0 && r != av::AVERROR_EAGAIN) {
                return Next::Ended;
            }

            let mut got = self.input.read(&self.pkt);
            while got && self.pkt.stream_index() != self.vstream {
                self.pkt.unref();
                got = self.input.read(&self.pkt);
            }
            if got {
                unsafe { av::avcodec_send_packet(self.vdec.0, self.pkt.0) };
                self.pkt.unref();
            } else if !self.draining {
                unsafe { av::avcodec_send_packet(self.vdec.0, ptr::null()) };
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

    let mut input = Input::open(joined)?;
    let mut start = f64::MAX;
    let mut end = 0.0;
    if let Some(pkt) = Packet::alloc() {
        while input.read(&pkt) {
            let index = pkt.stream_index();
            if pkt.pts() != av::AV_NOPTS_VALUE && index >= 0 && index < input.stream_count() {
                let tb = unsafe { stream::TIME_BASE.get(input.stream(index)) }.to_f64();
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
