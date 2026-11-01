//! Southstar — owners of a libav decoder and of the packets it reads and frames it returns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::{self, NonNull};

use crate::demux::Parameters;
use crate::os::{OsDecoder, Plane};
use crate::{
    self as av, AVChannelLayout, AVCodec, AVCodecContext, AVFrame, AVPacket, AVRational, Field,
    channel_layout, codec_context, codec_parameters, frame,
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

    pub fn data(&self) -> &[u8] {
        let data = self.get(&av::packet::DATA);
        let size = usize::try_from(self.get(&av::packet::SIZE)).unwrap_or(0);
        if data.is_null() || size == 0 {
            return &[];
        }
        // SAFETY: a read packet's data points at size payload bytes that live as long as the packet's reference.
        unsafe { core::slice::from_raw_parts(data, size) }
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

    pub(crate) fn set<T: Copy>(&mut self, field: &Field<AVFrame, T>, value: T) {
        // SAFETY: self.0 is an allocated frame this value owns and the field's offset and type come from the installed headers.
        unsafe { field.set(self.0.as_ptr(), value) };
    }

    pub(crate) fn set_default_layout(&mut self, channels: c_int) {
        let layout = frame::CH_LAYOUT
            .ptr(self.0.as_ptr())
            .cast::<AVChannelLayout>();
        // SAFETY: the frame was just unreferenced, so its channel layout is reset and may be initialized in place.
        unsafe { av::av_channel_layout_default(layout, channels) };
    }

    pub(crate) fn allocate(&mut self, align: c_int) -> bool {
        // SAFETY: self.0 is an unreferenced frame whose format and size, or sample count and layout, were just set.
        unsafe { av::av_frame_get_buffer(self.0.as_ptr(), align) >= 0 }
    }

    pub(crate) fn plane(&mut self, index: usize, rows: usize) -> Option<Plane<'_>> {
        let base = NonNull::new(*self.get(&frame::DATA).get(index)?)?;
        let stride = usize::try_from(*self.get(&frame::LINESIZE).get(index)?).ok()?;
        Some(Plane::new(base, stride, rows))
    }

    pub(crate) fn float_samples(&mut self, count: usize) -> Option<&mut [f32]> {
        let base = NonNull::new(self.get(&frame::DATA)[0].cast::<f32>())?;
        // SAFETY: av_frame_get_buffer allocated plane 0 for nb_samples interleaved float frames of every channel, count floats in all, borrowed exclusively with self.
        Some(unsafe { core::slice::from_raw_parts_mut(base.as_ptr(), count) })
    }

    pub(crate) fn take_from(&mut self, source: &mut Frame) {
        self.unref();
        // SAFETY: both frames are allocated and self was just unreferenced, as av_frame_move_ref requires.
        unsafe { av::av_frame_move_ref(self.0.as_ptr(), source.0.as_ptr()) };
    }
}

pub enum Decoder {
    Libav(Codec),
    System(OsDecoder),
}

impl Decoder {
    pub fn open(parameters: &Parameters, time_base: AVRational) -> Option<Decoder> {
        if let Some(system) = OsDecoder::open(parameters, time_base) {
            return Some(Decoder::System(system));
        }
        let description = CodecDescription::find(parameters.get(&codec_parameters::CODEC_ID))?;
        Codec::open(parameters, description).map(Decoder::Libav)
    }

    pub fn exists(codec_id: c_int) -> bool {
        if std::env::var_os("NS_MEDIA_OS_CODECS_ONLY").is_some() {
            return crate::os::supports(codec_id);
        }
        crate::os::supports(codec_id) || CodecDescription::find(codec_id).is_some()
    }

    pub fn is_system(&self) -> bool {
        matches!(self, Decoder::System(_))
    }

    pub fn send(&mut self, packet: Option<&Packet>) -> c_int {
        match self {
            Decoder::Libav(codec) => codec.send(packet),
            Decoder::System(system) => system.send(packet),
        }
    }

    pub fn receive(&mut self, frame: &mut Frame) -> c_int {
        match self {
            Decoder::Libav(codec) => codec.receive(frame),
            Decoder::System(system) => system.receive(frame),
        }
    }

    pub fn flush(&mut self) {
        match self {
            Decoder::Libav(codec) => codec.flush(),
            Decoder::System(system) => system.flush(),
        }
    }

    pub fn sample_rate(&self) -> c_int {
        match self {
            Decoder::Libav(codec) => codec.get(&codec_context::SAMPLE_RATE),
            Decoder::System(system) => system.audio_format().map_or(0, |format| format.0),
        }
    }

    pub fn channels(&self) -> c_int {
        match self {
            Decoder::Libav(codec) => codec.channels(),
            Decoder::System(system) => system.audio_format().map_or(0, |format| format.1),
        }
    }

    pub fn sample_format(&self) -> c_int {
        match self {
            Decoder::Libav(codec) => codec.get(&codec_context::SAMPLE_FMT),
            Decoder::System(_) => av::AV_SAMPLE_FMT_FLT,
        }
    }

    #[cfg(feature = "resample")]
    pub(crate) fn channel_layout(&self) -> *const AVChannelLayout {
        match self {
            Decoder::Libav(codec) => codec.channel_layout(),
            Decoder::System(system) => system
                .audio_format()
                .map_or(core::ptr::null(), |format| format.2),
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let mut frame = self.0.as_ptr();
        // SAFETY: frame is the frame av_frame_alloc returned, freed exactly once.
        unsafe { av::av_frame_free(&mut frame) };
    }
}
