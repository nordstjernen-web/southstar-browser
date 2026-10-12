//! Southstar — WebM, Ogg and MP4 audio through libav: whole files decoded from memory, and growing files streamed in as they download.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_av::{
    self as av, AVChannelLayout, AVCodec, AVCodecContext, AVFormatContext, AVFrame, AVIOContext,
    AVPacket, AVRational, AVStream, SwrContext, channel_layout, codec_context, format_context,
    frame, io_context, packet, stream,
};

use crate::decode::{Decoded, Sink, file_size, max_floats};
use crate::ffi::sdl::AudioStream;
use crate::mixer::DEVICE_RATE;

const IO_BUFFER: c_int = 32768;
const DRAIN_SAMPLES: usize = 4096;

pub fn quiet() {
    unsafe { av::av_log_set_level(av::AV_LOG_QUIET) };
}

fn layout_channels(layout: *mut u8) -> c_int {
    unsafe { channel_layout::NB_CHANNELS.get(layout.cast::<AVChannelLayout>()) }
}

struct Format(*mut AVFormatContext);

impl Format {
    fn stream(&self, index: c_int) -> *mut AVStream {
        unsafe { *format_context::STREAMS.get(self.0).add(index as usize) }
    }

    fn time_base(&self, index: c_int) -> AVRational {
        unsafe { stream::TIME_BASE.get(self.stream(index)) }
    }
}

impl Drop for Format {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { av::avformat_close_input(&mut self.0) };
        }
    }
}

struct Decoder {
    dec: *mut AVCodecContext,
    swr: *mut SwrContext,
    pkt: *mut AVPacket,
    frame: *mut AVFrame,
    sidx: c_int,
    rate: c_int,
    channels: c_int,
    sample_fmt: c_int,
}

impl Decoder {
    fn open(fmt: &Format) -> Option<Decoder> {
        let mut codec: *const AVCodec = ptr::null();
        let sidx = unsafe {
            av::av_find_best_stream(fmt.0, av::AVMEDIA_TYPE_AUDIO, -1, -1, &mut codec, 0)
        };
        if sidx < 0 || codec.is_null() {
            return None;
        }
        let mut d = Decoder {
            dec: unsafe { av::avcodec_alloc_context3(codec) },
            swr: ptr::null_mut(),
            pkt: ptr::null_mut(),
            frame: ptr::null_mut(),
            sidx,
            rate: 0,
            channels: 0,
            sample_fmt: 0,
        };
        if d.dec.is_null() {
            return None;
        }
        let par = unsafe { stream::CODECPAR.get(fmt.stream(sidx)) };
        if unsafe { av::avcodec_parameters_to_context(d.dec, par) } < 0
            || unsafe { av::avcodec_open2(d.dec, codec, ptr::null_mut()) } < 0
        {
            return None;
        }
        let layout = unsafe { codec_context::CH_LAYOUT.ptr(d.dec) };
        d.rate = unsafe { codec_context::SAMPLE_RATE.get(d.dec) };
        d.channels = layout_channels(layout);
        d.sample_fmt = unsafe { codec_context::SAMPLE_FMT.get(d.dec) };
        if d.rate <= 0 || d.rate > 192000 || !(1..=8).contains(&d.channels) {
            return None;
        }
        d.swr = unsafe { av::swr_alloc() };
        if d.swr.is_null() {
            return None;
        }
        let swr = d.swr.cast::<c_void>();
        let layout = layout.cast::<AVChannelLayout>();
        unsafe {
            av::av_opt_set_chlayout(swr, c"in_chlayout".as_ptr(), layout, 0);
            av::av_opt_set_chlayout(swr, c"out_chlayout".as_ptr(), layout, 0);
            av::av_opt_set_int(swr, c"in_sample_rate".as_ptr(), i64::from(d.rate), 0);
            av::av_opt_set_int(swr, c"out_sample_rate".as_ptr(), i64::from(d.rate), 0);
            av::av_opt_set_sample_fmt(swr, c"in_sample_fmt".as_ptr(), d.sample_fmt, 0);
            av::av_opt_set_sample_fmt(swr, c"out_sample_fmt".as_ptr(), av::AV_SAMPLE_FMT_FLT, 0);
        }
        if unsafe { av::swr_init(d.swr) } < 0 {
            return None;
        }
        d.pkt = unsafe { av::av_packet_alloc() };
        d.frame = unsafe { av::av_frame_alloc() };
        if d.pkt.is_null() || d.frame.is_null() {
            return None;
        }
        Some(d)
    }

    fn frame_matches(&self) -> bool {
        let layout = unsafe { frame::CH_LAYOUT.ptr(self.frame) };
        layout_channels(layout) == self.channels
            && unsafe { frame::FORMAT.get(self.frame) } == self.sample_fmt
    }

    fn out_samples(&self) -> c_int {
        unsafe { av::swr_get_out_samples(self.swr, frame::NB_SAMPLES.get(self.frame)) }
    }

    fn convert_into(&self, dst: &mut Vec<f32>, out_samples: c_int) -> c_int {
        let at = dst.len();
        let out = unsafe { dst.as_mut_ptr().add(at).cast::<u8>() };
        let got = unsafe {
            av::swr_convert(
                self.swr,
                &out,
                out_samples,
                frame::EXTENDED_DATA.get(self.frame).cast::<*const u8>(),
                frame::NB_SAMPLES.get(self.frame),
            )
        };
        if got > 0 {
            let room = dst.capacity() - at;
            let filled = (got as usize * self.channels as usize).min(room);
            unsafe { dst.set_len(at + filled) };
        }
        got
    }

    fn unref(&self) {
        unsafe { av::av_frame_unref(self.frame) };
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            if !self.swr.is_null() {
                av::swr_free(&mut self.swr);
            }
            if !self.frame.is_null() {
                av::av_frame_free(&mut self.frame);
            }
            if !self.pkt.is_null() {
                av::av_packet_free(&mut self.pkt);
            }
            if !self.dec.is_null() {
                av::avcodec_free_context(&mut self.dec);
            }
        }
    }
}

struct Memory<'a> {
    data: &'a [u8],
    pos: usize,
}

unsafe extern "C" fn mem_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    let m = unsafe { &mut *opaque.cast::<Memory>() };
    let avail = m.data.len() - m.pos;
    if avail == 0 {
        return av::AVERROR_EOF;
    }
    let n = (size.max(0) as usize).min(avail);
    unsafe { ptr::copy_nonoverlapping(m.data.as_ptr().add(m.pos), buf, n) };
    m.pos += n;
    n as c_int
}

unsafe extern "C" fn mem_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    let m = unsafe { &mut *opaque.cast::<Memory>() };
    let whence = whence & !av::AVSEEK_FORCE;
    if whence == av::AVSEEK_SIZE {
        return m.data.len() as i64;
    }
    let base = match whence {
        1 => m.pos as i64,
        2 => m.data.len() as i64,
        _ => 0,
    };
    let np = base.saturating_add(offset);
    if np < 0 || np as u64 > m.data.len() as u64 {
        return -1;
    }
    m.pos = np as usize;
    np
}

struct Avio(*mut AVIOContext);

impl Drop for Avio {
    fn drop(&mut self) {
        unsafe {
            av::av_freep(io_context::BUFFER.ptr(self.0).cast());
            av::avio_context_free(&mut self.0);
        }
    }
}

pub fn decode_memory(bytes: &[u8]) -> Option<Decoded> {
    quiet();
    let mut src = Memory {
        data: bytes,
        pos: 0,
    };
    let iobuf = unsafe { av::av_malloc(IO_BUFFER as usize) }.cast::<u8>();
    if iobuf.is_null() {
        return None;
    }
    let avio = unsafe {
        av::avio_alloc_context(
            iobuf,
            IO_BUFFER,
            0,
            (&raw mut src).cast(),
            Some(mem_read),
            None,
            Some(mem_seek),
        )
    };
    if avio.is_null() {
        unsafe { av::av_free(iobuf.cast()) };
        return None;
    }
    let avio = Avio(avio);
    let mut fmt = unsafe { av::avformat_alloc_context() };
    if fmt.is_null() {
        return None;
    }
    unsafe { format_context::PB.set(fmt, avio.0) };
    if unsafe { av::avformat_open_input(&mut fmt, ptr::null(), ptr::null(), ptr::null_mut()) } < 0 {
        return None;
    }
    let fmt = Format(fmt);
    if unsafe { av::avformat_find_stream_info(fmt.0, ptr::null_mut()) } < 0 {
        return None;
    }
    let d = Decoder::open(&fmt)?;
    let mut pcm = Vec::new();
    decode_all(&fmt, &d, &mut pcm, max_floats(d.rate, d.channels));
    let decoded = (!pcm.is_empty()).then_some(Decoded {
        pcm,
        rate: d.rate,
        channels: d.channels,
    });
    drop(d);
    drop(fmt);
    drop(avio);
    decoded
}

fn decode_all(fmt: &Format, d: &Decoder, pcm: &mut Vec<f32>, max: usize) {
    let mut eof = false;
    while !eof {
        if unsafe { av::av_read_frame(fmt.0, d.pkt) } < 0 {
            unsafe { av::avcodec_send_packet(d.dec, ptr::null()) };
            eof = true;
        } else if unsafe { packet::STREAM_INDEX.get(d.pkt) } == d.sidx {
            unsafe {
                av::avcodec_send_packet(d.dec, d.pkt);
                av::av_packet_unref(d.pkt);
            }
        } else {
            unsafe { av::av_packet_unref(d.pkt) };
            continue;
        }
        loop {
            let rr = unsafe { av::avcodec_receive_frame(d.dec, d.frame) };
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
    format: Format,
    path: Vec<u8>,
    known_size: i64,
    resume_target: f64,
    resume_pending: bool,
    discard_frames: usize,
}

impl Input {
    pub fn open(path: &[u8]) -> Option<Input> {
        let cpath = CString::new(path).ok()?;
        let mut opts = ptr::null_mut();
        let mut fmt = ptr::null_mut();
        let rc = unsafe {
            av::av_dict_set(
                &mut opts,
                c"protocol_whitelist".as_ptr(),
                c"file".as_ptr(),
                0,
            );
            let rc = av::avformat_open_input(&mut fmt, cpath.as_ptr(), ptr::null(), &mut opts);
            av::av_dict_free(&mut opts);
            rc
        };
        if rc < 0 {
            return None;
        }
        let format = Format(fmt);
        if unsafe { av::avformat_find_stream_info(format.0, ptr::null_mut()) } < 0 {
            return None;
        }
        let decoder = Decoder::open(&format)?;
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
        let pts = unsafe { frame::PTS.get(self.decoder.frame) };
        let t0 = if pts != av::AV_NOPTS_VALUE {
            pts as f64 * self.format.time_base(self.decoder.sidx).to_f64()
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
        let ts = (decoded_end / input.format.time_base(sidx).to_f64()) as i64;
        if unsafe { av::av_seek_frame(input.format.0, sidx, ts, av::AVSEEK_FLAG_BACKWARD) } >= 0 {
            unsafe { av::avcodec_flush_buffers(input.decoder.dec) };
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
        let (fmt, pkt, dec) = (input.format.0, input.decoder.pkt, input.decoder.dec);
        if unsafe { av::av_read_frame(fmt, pkt) } < 0 {
            if !reopened && file_size(&input.path) > input.known_size {
                reopened = true;
                if reopen(slot, sink) {
                    continue;
                }
            }
            break;
        }
        if unsafe { packet::STREAM_INDEX.get(pkt) } != input.decoder.sidx {
            unsafe { av::av_packet_unref(pkt) };
            continue;
        }
        unsafe {
            av::avcodec_send_packet(dec, pkt);
            av::av_packet_unref(pkt);
        }
        while unsafe { av::avcodec_receive_frame(dec, input.decoder.frame) } == 0 {
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
