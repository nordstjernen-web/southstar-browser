//! Southstar — owners of the libav demuxer, decoder, packet, frame and scaler the video helper drives, each libav call one documented unsafe operation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use southstar_av::{
    self as av, AVCodec, AVCodecContext, AVCodecParameters, AVDictionary, AVFormatContext, AVFrame,
    AVPacket, AVStream, Field, SwsContext, format_context, frame,
};

pub fn quiet_logging() {
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

pub struct Format(NonNull<AVFormatContext>);

impl Format {
    pub fn open_file(path: &CStr) -> Option<Format> {
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
        NonNull::new(context).map(Format)
    }

    fn get<T: Copy>(&self, field: &Field<AVFormatContext, T>) -> T {
        // SAFETY: self.0 is an open format context and the field's offset and type come from the installed libav headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn find_stream_info(&mut self) -> bool {
        // SAFETY: self.0 is an open format context; no per-stream options are passed.
        unsafe { av::avformat_find_stream_info(self.0.as_ptr(), ptr::null_mut()) >= 0 }
    }

    pub fn best_stream(&mut self, media_type: c_int) -> Option<c_int> {
        // SAFETY: self.0 is an open format context; no decoder is asked for.
        let index = unsafe {
            av::av_find_best_stream(self.0.as_ptr(), media_type, -1, -1, ptr::null_mut(), 0)
        };
        (index >= 0).then_some(index)
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

    pub fn duration(&self) -> i64 {
        self.get(&format_context::DURATION)
    }

    pub fn read_packet(&mut self, packet: &mut Packet) -> bool {
        // SAFETY: self.0 is an open format context and packet an allocated, unreferenced packet libav fills.
        unsafe { av::av_read_frame(self.0.as_ptr(), packet.0.as_ptr()) >= 0 }
    }

    pub fn seek(&mut self, stream: c_int, timestamp: i64, flags: c_int) {
        // SAFETY: self.0 is an open format context; libav checks the stream index.
        unsafe { av::av_seek_frame(self.0.as_ptr(), stream, timestamp, flags) };
    }
}

impl Drop for Format {
    fn drop(&mut self) {
        let mut context = self.0.as_ptr();
        // SAFETY: context is the format context avformat_open_input opened, closed exactly once.
        unsafe { av::avformat_close_input(&mut context) };
    }
}

pub struct Stream<'a>(NonNull<AVStream>, PhantomData<&'a Format>);

impl Stream<'_> {
    pub fn get<T: Copy>(&self, field: &Field<AVStream, T>) -> T {
        // SAFETY: self.0 is a stream of a format context that outlives this borrow; the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn parameters(&self) -> Option<Parameters<'_>> {
        NonNull::new(self.get(&southstar_av::stream::CODECPAR))
            .map(|parameters| Parameters(parameters, PhantomData))
    }
}

pub struct Parameters<'a>(NonNull<AVCodecParameters>, PhantomData<&'a Format>);

impl Parameters<'_> {
    pub fn get<T: Copy>(&self, field: &Field<AVCodecParameters, T>) -> T {
        // SAFETY: self.0 is the codec parameters of a stream that outlives this borrow; the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }
}

pub struct Codec(NonNull<AVCodecContext>);

impl Codec {
    pub fn open(parameters: &Parameters, codec_id: c_int) -> Option<Codec> {
        // SAFETY: looks a decoder up by id; the result is null or a static codec description.
        let decoder: *const AVCodec = unsafe { av::avcodec_find_decoder(codec_id) };
        if decoder.is_null() {
            return None;
        }
        // SAFETY: decoder is a codec libav returned; the result is null or a new context this value owns.
        let codec = Codec(NonNull::new(unsafe {
            av::avcodec_alloc_context3(decoder)
        })?);
        // SAFETY: codec.0 is a fresh context and parameters live codec parameters, copied into it.
        if unsafe { av::avcodec_parameters_to_context(codec.0.as_ptr(), parameters.0.as_ptr()) } < 0
        {
            return None;
        }
        // SAFETY: codec.0 was allocated for decoder, which it is opened with; no options are passed.
        let opened = unsafe { av::avcodec_open2(codec.0.as_ptr(), decoder, ptr::null_mut()) };
        (opened >= 0).then_some(codec)
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
        unsafe { southstar_av::packet::STREAM_INDEX.get(self.0.as_ptr()) }
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

#[derive(Default)]
pub struct Scaler {
    context: Option<NonNull<SwsContext>>,
    width: u32,
    height: u32,
}

impl Scaler {
    pub fn prepare(&mut self, source: &Frame, width: u32, height: u32) -> bool {
        let previous = self.context.map_or(ptr::null_mut(), NonNull::as_ptr);
        // SAFETY: previous is null or the scaler this value owns, which libav reuses or frees; the filters and parameters are null.
        let context = unsafe {
            av::sws_getCachedContext(
                previous,
                source.get(&frame::WIDTH),
                source.get(&frame::HEIGHT),
                source.get(&frame::FORMAT),
                width as c_int,
                height as c_int,
                av::AV_PIX_FMT_BGRA,
                av::SWS_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        self.context = NonNull::new(context);
        self.width = width;
        self.height = height;
        self.context.is_some()
    }

    pub fn scale(&mut self, source: &Frame, target: &southstar_media_shm::Frame) {
        let Some(context) = self.context else {
            return;
        };
        if target.width != self.width || target.height != self.height {
            return;
        }
        let data = source.get(&frame::DATA);
        let linesize = source.get(&frame::LINESIZE);
        let planes = [
            target.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        ];
        let strides: [c_int; 4] = [target.stride as c_int, 0, 0, 0];
        // SAFETY: the scaler was prepared for source's size and format and for target's width and height; target holds height rows of stride bytes of at least width * 4 BGRA bytes each.
        unsafe {
            av::sws_scale(
                context.as_ptr(),
                data.as_ptr().cast(),
                linesize.as_ptr(),
                0,
                source.get(&frame::HEIGHT),
                planes.as_ptr(),
                strides.as_ptr(),
            )
        };
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        if let Some(context) = self.context {
            // SAFETY: context is the scaler sws_getCachedContext returned, freed exactly once.
            unsafe { av::sws_freeContext(context.as_ptr()) };
        }
    }
}
