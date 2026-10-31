//! Southstar — owners of a libav decoder and of the packets it reads and frames it returns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::{self, NonNull};

use crate::demux::Parameters;
use crate::{
    self as av, AVCodec, AVCodecContext, AVFrame, AVPacket, Field, channel_layout, codec_context,
    frame,
};

const CODEC_CHANNELS: Field<AVCodecContext, c_int> =
    Field::at((codec_context::CH_LAYOUT.offset() + channel_layout::NB_CHANNELS.offset()) as i64);
const FRAME_CHANNELS: Field<AVFrame, c_int> =
    Field::at((frame::CH_LAYOUT.offset() + channel_layout::NB_CHANNELS.offset()) as i64);

#[derive(Clone, Copy)]
pub struct CodecDescription(NonNull<AVCodec>);

impl CodecDescription {
    pub fn find(codec_id: c_int) -> Option<CodecDescription> {
        // SAFETY: looks a decoder up by id; the result is null or a static codec description.
        let decoder = unsafe { av::avcodec_find_decoder(codec_id) };
        CodecDescription::from_libav(decoder)
    }

    pub(crate) fn from_libav(decoder: *const AVCodec) -> Option<CodecDescription> {
        NonNull::new(decoder.cast_mut()).map(CodecDescription)
    }
}

pub struct Codec(NonNull<AVCodecContext>);

impl Codec {
    pub fn open(parameters: &Parameters, decoder: CodecDescription) -> Option<Codec> {
        // SAFETY: decoder is a codec libav returned; the result is null or a new context this value owns.
        let codec = Codec(NonNull::new(unsafe {
            av::avcodec_alloc_context3(decoder.0.as_ptr())
        })?);
        // SAFETY: codec.0 is a fresh context and parameters live codec parameters, copied into it.
        if unsafe { av::avcodec_parameters_to_context(codec.0.as_ptr(), parameters.as_ptr()) } < 0 {
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

    #[cfg(feature = "resample")]
    pub(crate) fn channel_layout(&self) -> *const crate::AVChannelLayout {
        codec_context::CH_LAYOUT
            .ptr(self.0.as_ptr())
            .cast::<crate::AVChannelLayout>()
            .cast_const()
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

    pub fn get<T: Copy>(&self, field: &Field<AVPacket, T>) -> T {
        // SAFETY: self.0 is an allocated packet and the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn stream_index(&self) -> c_int {
        self.get(&av::packet::STREAM_INDEX)
    }

    pub fn pts(&self) -> i64 {
        self.get(&av::packet::PTS)
    }

    pub fn duration(&self) -> i64 {
        self.get(&av::packet::DURATION)
    }

    pub fn unref(&mut self) {
        // SAFETY: self.0 is an allocated packet; its payload reference is dropped.
        unsafe { av::av_packet_unref(self.0.as_ptr()) };
    }

    pub(crate) fn as_ptr(&mut self) -> *mut AVPacket {
        self.0.as_ptr()
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
