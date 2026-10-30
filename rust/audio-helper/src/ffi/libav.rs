//! Southstar — owners of the libav demuxer, in-memory input, decoder, packet, frame and resampler the audio helper drives, each libav call one documented unsafe operation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use southstar_av::{
    self as av, AVChannelLayout, AVCodec, AVCodecContext, AVCodecParameters, AVDictionary,
    AVFormatContext, AVFrame, AVIOContext, AVPacket, AVStream, Field, SwrContext, channel_layout,
    codec_context, format_context, frame, io_context, packet, stream,
};

const IO_BUFFER: c_int = 32768;

const CODEC_CHANNELS: Field<AVCodecContext, c_int> =
    Field::at((codec_context::CH_LAYOUT.offset() + channel_layout::NB_CHANNELS.offset()) as i64);
const FRAME_CHANNELS: Field<AVFrame, c_int> =
    Field::at((frame::CH_LAYOUT.offset() + channel_layout::NB_CHANNELS.offset()) as i64);

pub fn quiet() {
    // SAFETY: sets libav's global log level; no pointers are involved.
    unsafe { av::av_log_set_level(av::AV_LOG_QUIET) };
}

struct Options(*mut AVDictionary);

impl Options {
    fn new(key: &CStr, value: &CStr) -> Options {
        let mut options = Options(ptr::null_mut());
        // SAFETY: options.0 is a null dictionary libav allocates into; key and value are NUL-terminated and copied.
        unsafe { av::av_dict_set(&mut options.0, key.as_ptr(), value.as_ptr(), 0) };
        options
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: options.0 is null or a dictionary libav allocated; av_dict_free frees it and nulls the pointer.
        unsafe { av::av_dict_free(&mut self.0) };
    }
}

struct Memory<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Memory<'_> {
    fn read(&mut self, out: &mut [u8]) -> c_int {
        let rest = &self.data[self.pos..];
        if rest.is_empty() {
            return av::AVERROR_EOF;
        }
        let n = out.len().min(rest.len());
        out[..n].copy_from_slice(&rest[..n]);
        self.pos += n;
        n as c_int
    }

    fn seek(&mut self, offset: i64, whence: c_int) -> i64 {
        let whence = whence & !av::AVSEEK_FORCE;
        if whence == av::AVSEEK_SIZE {
            return self.data.len() as i64;
        }
        let base = match whence {
            1 => self.pos as i64,
            2 => self.data.len() as i64,
            _ => 0,
        };
        let np = base.saturating_add(offset);
        if np < 0 || np as u64 > self.data.len() as u64 {
            return -1;
        }
        self.pos = np as usize;
        np
    }
}

unsafe extern "C" fn memory_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    // SAFETY: opaque is the boxed Memory the Avio owning this context passed, alive and used by this one context only.
    let memory = unsafe { &mut *opaque.cast::<Memory>() };
    // SAFETY: libav passes its I/O buffer with room for size bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(buf, size.max(0) as usize) };
    memory.read(out)
}

unsafe extern "C" fn memory_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    // SAFETY: opaque is the boxed Memory the Avio owning this context passed, alive and used by this one context only.
    let memory = unsafe { &mut *opaque.cast::<Memory>() };
    memory.seek(offset, whence)
}

struct Avio<'a> {
    context: NonNull<AVIOContext>,
    source: NonNull<Memory<'a>>,
}

impl<'a> Avio<'a> {
    fn new(data: &'a [u8]) -> Option<Avio<'a>> {
        // SAFETY: allocates an I/O buffer of IO_BUFFER bytes or returns null.
        let buffer = unsafe { av::av_malloc(IO_BUFFER as usize) }.cast::<u8>();
        if buffer.is_null() {
            return None;
        }
        let source = NonNull::from(Box::leak(Box::new(Memory { data, pos: 0 })));
        // SAFETY: buffer holds IO_BUFFER bytes the context takes over; the opaque Memory is boxed and outlives the context, which this Avio frees first.
        let context = unsafe {
            av::avio_alloc_context(
                buffer,
                IO_BUFFER,
                0,
                source.as_ptr().cast(),
                Some(memory_read),
                None,
                Some(memory_seek),
            )
        };
        let Some(context) = NonNull::new(context) else {
            // SAFETY: buffer came from av_malloc above and no context took it over.
            unsafe { av::av_free(buffer.cast()) };
            // SAFETY: source is the Memory leaked above, which no context was given.
            drop(unsafe { Box::from_raw(source.as_ptr()) });
            return None;
        };
        Some(Avio { context, source })
    }
}

impl Drop for Avio<'_> {
    fn drop(&mut self) {
        let buffer = io_context::BUFFER.ptr(self.context.as_ptr());
        // SAFETY: buffer points at the context's current I/O buffer, which libav allocated; av_freep frees it and nulls the field.
        unsafe { av::av_freep(buffer.cast()) };
        let mut context = self.context.as_ptr();
        // SAFETY: context is the I/O context avio_alloc_context returned, freed exactly once.
        unsafe { av::avio_context_free(&mut context) };
        // SAFETY: source is the Memory leaked in new(), reclaimed once now that no context reads it.
        drop(unsafe { Box::from_raw(self.source.as_ptr()) });
    }
}

pub struct Format<'a> {
    context: NonNull<AVFormatContext>,
    _io: Option<Avio<'a>>,
}

impl Format<'static> {
    pub fn open_file(path: &CStr) -> Option<Format<'static>> {
        let mut options = Options::new(c"protocol_whitelist", c"file");
        let mut context = ptr::null_mut();
        // SAFETY: context is null so libav allocates it, and frees it again on failure; path is NUL-terminated; options is a live dictionary libav may replace.
        let opened = unsafe {
            av::avformat_open_input(&mut context, path.as_ptr(), ptr::null(), &mut options.0)
        };
        drop(options);
        if opened < 0 {
            return None;
        }
        Some(Format {
            context: NonNull::new(context)?,
            _io: None,
        })
    }
}

impl<'a> Format<'a> {
    pub fn open_memory(data: &'a [u8]) -> Option<Format<'a>> {
        let io = Avio::new(data)?;
        // SAFETY: allocates an empty format context or returns null.
        let mut context = unsafe { av::avformat_alloc_context() };
        if context.is_null() {
            return None;
        }
        // SAFETY: context is a fresh format context and the offset of its pb field comes from the installed headers; io outlives the opened context.
        unsafe { format_context::PB.set(context, io.context.as_ptr()) };
        // SAFETY: context carries the custom I/O context, so libav reads through it and leaves it to io; on failure libav frees context.
        let opened = unsafe {
            av::avformat_open_input(&mut context, ptr::null(), ptr::null(), ptr::null_mut())
        };
        if opened < 0 {
            return None;
        }
        Some(Format {
            context: NonNull::new(context)?,
            _io: Some(io),
        })
    }

    fn get<T: Copy>(&self, field: &Field<AVFormatContext, T>) -> T {
        // SAFETY: self.context is an open format context and the field's offset and type come from the installed libav headers.
        unsafe { field.get(self.context.as_ptr()) }
    }

    pub fn find_stream_info(&mut self) -> bool {
        // SAFETY: self.context is an open format context; no per-stream options are passed.
        unsafe { av::avformat_find_stream_info(self.context.as_ptr(), ptr::null_mut()) >= 0 }
    }

    pub fn best_audio_stream(&mut self) -> Option<(c_int, CodecDescription)> {
        let mut decoder: *const AVCodec = ptr::null();
        // SAFETY: self.context is an open format context; decoder is a writable pointer libav sets to a static codec description.
        let index = unsafe {
            av::av_find_best_stream(
                self.context.as_ptr(),
                av::AVMEDIA_TYPE_AUDIO,
                -1,
                -1,
                &mut decoder,
                0,
            )
        };
        if index < 0 {
            return None;
        }
        Some((index, CodecDescription(NonNull::new(decoder.cast_mut())?)))
    }

    pub fn stream(&self, index: c_int) -> Option<Stream<'_>> {
        let count = self.get(&format_context::NB_STREAMS);
        let index = u32::try_from(index).ok().filter(|&index| index < count)?;
        let entry = self
            .get(&format_context::STREAMS)
            .wrapping_add(index as usize);
        // SAFETY: streams is libav's array of nb_streams stream pointers and index is below that count.
        let stream = unsafe { entry.read() };
        NonNull::new(stream).map(|stream| Stream(stream, PhantomData))
    }

    pub fn read_packet(&mut self, packet: &mut Packet) -> bool {
        // SAFETY: self.context is an open format context and packet an allocated, unreferenced packet libav fills.
        unsafe { av::av_read_frame(self.context.as_ptr(), packet.0.as_ptr()) >= 0 }
    }

    pub fn seek(&mut self, stream: c_int, timestamp: i64, flags: c_int) -> bool {
        // SAFETY: self.context is an open format context; libav checks the stream index.
        unsafe { av::av_seek_frame(self.context.as_ptr(), stream, timestamp, flags) >= 0 }
    }
}

impl Drop for Format<'_> {
    fn drop(&mut self) {
        let mut context = self.context.as_ptr();
        // SAFETY: context is the format context avformat_open_input opened, closed exactly once and before its custom I/O context drops.
        unsafe { av::avformat_close_input(&mut context) };
    }
}

pub struct Stream<'a>(NonNull<AVStream>, PhantomData<&'a ()>);

impl Stream<'_> {
    pub fn time_base(&self) -> av::AVRational {
        // SAFETY: self.0 is a stream of a format context that outlives this borrow; the offset comes from the installed headers.
        unsafe { stream::TIME_BASE.get(self.0.as_ptr()) }
    }

    fn parameters(&self) -> *const AVCodecParameters {
        // SAFETY: self.0 is a stream of a format context that outlives this borrow; the offset comes from the installed headers.
        unsafe { stream::CODECPAR.get(self.0.as_ptr()) }
    }
}

#[derive(Clone, Copy)]
pub struct CodecDescription(NonNull<AVCodec>);

pub struct Codec(NonNull<AVCodecContext>);

impl Codec {
    pub fn open(stream: &Stream, decoder: CodecDescription) -> Option<Codec> {
        // SAFETY: decoder is a codec libav returned; the result is null or a new context this value owns.
        let codec = Codec(NonNull::new(unsafe {
            av::avcodec_alloc_context3(decoder.0.as_ptr())
        })?);
        // SAFETY: codec.0 is a fresh context and the stream's codec parameters are live, copied into it.
        if unsafe { av::avcodec_parameters_to_context(codec.0.as_ptr(), stream.parameters()) } < 0 {
            return None;
        }
        // SAFETY: codec.0 was allocated for decoder, which it is opened with; no options are passed.
        let opened =
            unsafe { av::avcodec_open2(codec.0.as_ptr(), decoder.0.as_ptr(), ptr::null_mut()) };
        (opened >= 0).then_some(codec)
    }

    pub fn get<T: Copy>(&self, field: &Field<AVCodecContext, T>) -> T {
        // SAFETY: self.0 is an open codec context and the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn channels(&self) -> c_int {
        self.get(&CODEC_CHANNELS)
    }

    pub fn send(&mut self, packet: Option<&Packet>) -> c_int {
        let packet = packet.map_or(ptr::null(), |packet| packet.0.as_ptr().cast_const());
        // SAFETY: self.0 is an open decoder; packet is null (drain) or a live packet libav references.
        unsafe { av::avcodec_send_packet(self.0.as_ptr(), packet) }
    }

    pub fn receive(&mut self, frame: &mut Frame) -> c_int {
        // SAFETY: self.0 is an open decoder and frame an allocated frame it fills.
        unsafe { av::avcodec_receive_frame(self.0.as_ptr(), frame.0.as_ptr()) }
    }

    pub fn flush(&mut self) {
        // SAFETY: self.0 is an open decoder.
        unsafe { av::avcodec_flush_buffers(self.0.as_ptr()) };
    }
}

impl Drop for Codec {
    fn drop(&mut self) {
        let mut context = self.0.as_ptr();
        // SAFETY: context is the codec context avcodec_alloc_context3 returned, freed exactly once.
        unsafe { av::avcodec_free_context(&mut context) };
    }
}

pub struct Packet(NonNull<AVPacket>);

impl Packet {
    pub fn new() -> Option<Packet> {
        // SAFETY: allocates an empty packet or returns null.
        NonNull::new(unsafe { av::av_packet_alloc() }).map(Packet)
    }

    pub fn stream_index(&self) -> c_int {
        // SAFETY: self.0 is an allocated packet and the offset comes from the installed headers.
        unsafe { packet::STREAM_INDEX.get(self.0.as_ptr()) }
    }

    pub fn unref(&mut self) {
        // SAFETY: self.0 is an allocated packet; its payload reference is dropped.
        unsafe { av::av_packet_unref(self.0.as_ptr()) };
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        let mut packet = self.0.as_ptr();
        // SAFETY: packet is the packet av_packet_alloc returned, freed exactly once.
        unsafe { av::av_packet_free(&mut packet) };
    }
}

pub struct Frame(NonNull<AVFrame>);

impl Frame {
    pub fn new() -> Option<Frame> {
        // SAFETY: allocates an empty frame or returns null.
        NonNull::new(unsafe { av::av_frame_alloc() }).map(Frame)
    }

    pub fn get<T: Copy>(&self, field: &Field<AVFrame, T>) -> T {
        // SAFETY: self.0 is an allocated frame and the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn channels(&self) -> c_int {
        self.get(&FRAME_CHANNELS)
    }

    pub fn unref(&mut self) {
        // SAFETY: self.0 is an allocated frame; its buffer references are dropped.
        unsafe { av::av_frame_unref(self.0.as_ptr()) };
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let mut frame = self.0.as_ptr();
        // SAFETY: frame is the frame av_frame_alloc returned, freed exactly once.
        unsafe { av::av_frame_free(&mut frame) };
    }
}

pub struct Resampler(NonNull<SwrContext>);

impl Resampler {
    pub fn to_float(codec: &Codec, rate: c_int, sample_fmt: c_int) -> Option<Resampler> {
        // SAFETY: allocates an unconfigured resampler or returns null.
        let resampler = Resampler(NonNull::new(unsafe { av::swr_alloc() })?);
        let layout = codec_context::CH_LAYOUT.ptr(codec.0.as_ptr());
        let layout = layout.cast::<AVChannelLayout>().cast_const();
        resampler.set_layout(c"in_chlayout", layout);
        resampler.set_layout(c"out_chlayout", layout);
        resampler.set_int(c"in_sample_rate", i64::from(rate));
        resampler.set_int(c"out_sample_rate", i64::from(rate));
        resampler.set_sample_fmt(c"in_sample_fmt", sample_fmt);
        resampler.set_sample_fmt(c"out_sample_fmt", av::AV_SAMPLE_FMT_FLT);
        // SAFETY: resampler.0 is an allocated, configured resampler.
        let initialized = unsafe { av::swr_init(resampler.0.as_ptr()) };
        (initialized >= 0).then_some(resampler)
    }

    fn set_layout(&self, name: &CStr, layout: *const AVChannelLayout) {
        // SAFETY: self.0 is an allocated resampler, name is NUL-terminated and layout points at a live channel layout libav copies.
        unsafe { av::av_opt_set_chlayout(self.0.as_ptr().cast(), name.as_ptr(), layout, 0) };
    }

    fn set_int(&self, name: &CStr, value: i64) {
        // SAFETY: self.0 is an allocated resampler and name is NUL-terminated.
        unsafe { av::av_opt_set_int(self.0.as_ptr().cast(), name.as_ptr(), value, 0) };
    }

    fn set_sample_fmt(&self, name: &CStr, format: c_int) {
        // SAFETY: self.0 is an allocated resampler and name is NUL-terminated.
        unsafe { av::av_opt_set_sample_fmt(self.0.as_ptr().cast(), name.as_ptr(), format, 0) };
    }

    pub fn out_samples(&self, frame: &Frame) -> c_int {
        // SAFETY: self.0 is an initialized resampler.
        unsafe { av::swr_get_out_samples(self.0.as_ptr(), frame.get(&frame::NB_SAMPLES)) }
    }

    pub fn convert_into(
        &self,
        frame: &Frame,
        channels: c_int,
        dst: &mut Vec<f32>,
        out_samples: c_int,
    ) -> c_int {
        let at = dst.len();
        let room = dst.capacity() - at;
        let out = dst.spare_capacity_mut().as_mut_ptr().cast::<u8>();
        let out_samples = out_samples.min((room / channels.max(1) as usize) as c_int);
        // SAFETY: self.0 was initialized for frame's layout and format; out has room for out_samples interleaved float frames of channels samples, and frame's planes hold nb_samples samples.
        let got = unsafe {
            av::swr_convert(
                self.0.as_ptr(),
                &out,
                out_samples,
                frame.get(&frame::EXTENDED_DATA).cast::<*const u8>(),
                frame.get(&frame::NB_SAMPLES),
            )
        };
        if got > 0 {
            let filled = (got as usize * channels as usize).min(room);
            // SAFETY: swr_convert initialized filled floats past at, all within the capacity.
            unsafe { dst.set_len(at + filled) };
        }
        got
    }
}

impl Drop for Resampler {
    fn drop(&mut self) {
        let mut context = self.0.as_ptr();
        // SAFETY: context is the resampler swr_alloc returned, freed exactly once.
        unsafe { av::swr_free(&mut context) };
    }
}
