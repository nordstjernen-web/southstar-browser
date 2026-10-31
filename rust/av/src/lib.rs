//! Southstar — FFmpeg's libav* declarations, with field offsets and enum values taken from the installed headers, and safe owners of the contexts built on them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "scale")]
pub mod convert;
pub mod decode;
pub mod demux;
pub mod ffi;
pub mod layout;
#[cfg(feature = "resample")]
pub mod resample;

#[cfg(feature = "scale")]
pub use convert::Scaler;
pub use decode::{Codec, CodecDescription, Frame, Packet};
pub use demux::{Format, Parameters, Stream, quiet_logging};
pub use ffi::*;
pub use layout::Field;
#[cfg(feature = "resample")]
pub use resample::Resampler;

const LAYOUT: Option<&str> = match option_env!("NS_AV_LAYOUT") {
    Some(text) if !text.is_empty() => Some(text),
    _ => None,
};

pub const fn available() -> bool {
    LAYOUT.is_some()
}

const fn value(key: &str) -> i64 {
    match LAYOUT {
        Some(text) => layout::lookup(text, key),
        None => 0,
    }
}

const fn constant(key: &str) -> core::ffi::c_int {
    value(key) as core::ffi::c_int
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AVRational {
    pub num: core::ffi::c_int,
    pub den: core::ffi::c_int,
}

impl AVRational {
    pub fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn inverse(self) -> AVRational {
        AVRational {
            num: self.den,
            den: self.num,
        }
    }
}

pub const AV_NOPTS_VALUE: i64 = i64::MIN;
pub const AV_TIME_BASE: i64 = 1_000_000;
pub const AV_NUM_DATA_POINTERS: usize = 8;

pub const SIZEOF_AV_CHANNEL_LAYOUT: usize = value("sizeof.AVChannelLayout") as usize;

pub const AVERROR_EAGAIN: core::ffi::c_int = constant("AVERROR_EAGAIN");
pub const AVERROR_EOF: core::ffi::c_int = constant("AVERROR_EOF");
pub const AVMEDIA_TYPE_VIDEO: core::ffi::c_int = constant("AVMEDIA_TYPE_VIDEO");
pub const AVMEDIA_TYPE_AUDIO: core::ffi::c_int = constant("AVMEDIA_TYPE_AUDIO");
pub const AV_PIX_FMT_BGRA: core::ffi::c_int = constant("AV_PIX_FMT_BGRA");
pub const AV_SAMPLE_FMT_FLT: core::ffi::c_int = constant("AV_SAMPLE_FMT_FLT");
pub const AV_SAMPLE_FMT_NONE: core::ffi::c_int = constant("AV_SAMPLE_FMT_NONE");
pub const AV_LOG_QUIET: core::ffi::c_int = constant("AV_LOG_QUIET");
pub const SWS_BILINEAR: core::ffi::c_int = constant("SWS_BILINEAR");
pub const AVSEEK_FLAG_BACKWARD: core::ffi::c_int = constant("AVSEEK_FLAG_BACKWARD");
pub const AVSEEK_SIZE: core::ffi::c_int = constant("AVSEEK_SIZE");
pub const AVSEEK_FORCE: core::ffi::c_int = constant("AVSEEK_FORCE");

pub const AV_CODEC_ID_NONE: core::ffi::c_int = constant("AV_CODEC_ID_NONE");
pub const AV_CODEC_ID_VP8: core::ffi::c_int = constant("AV_CODEC_ID_VP8");
pub const AV_CODEC_ID_VP9: core::ffi::c_int = constant("AV_CODEC_ID_VP9");
pub const AV_CODEC_ID_AV1: core::ffi::c_int = constant("AV_CODEC_ID_AV1");
pub const AV_CODEC_ID_H264: core::ffi::c_int = constant("AV_CODEC_ID_H264");
pub const AV_CODEC_ID_HEVC: core::ffi::c_int = constant("AV_CODEC_ID_HEVC");
pub const AV_CODEC_ID_MPEG4: core::ffi::c_int = constant("AV_CODEC_ID_MPEG4");
pub const AV_CODEC_ID_THEORA: core::ffi::c_int = constant("AV_CODEC_ID_THEORA");
pub const AV_CODEC_ID_OPUS: core::ffi::c_int = constant("AV_CODEC_ID_OPUS");
pub const AV_CODEC_ID_VORBIS: core::ffi::c_int = constant("AV_CODEC_ID_VORBIS");
pub const AV_CODEC_ID_AAC: core::ffi::c_int = constant("AV_CODEC_ID_AAC");
pub const AV_CODEC_ID_MP3: core::ffi::c_int = constant("AV_CODEC_ID_MP3");
pub const AV_CODEC_ID_FLAC: core::ffi::c_int = constant("AV_CODEC_ID_FLAC");
pub const AV_CODEC_ID_ALAC: core::ffi::c_int = constant("AV_CODEC_ID_ALAC");
pub const AV_CODEC_ID_AC3: core::ffi::c_int = constant("AV_CODEC_ID_AC3");
pub const AV_CODEC_ID_EAC3: core::ffi::c_int = constant("AV_CODEC_ID_EAC3");

const fn field<S, T: Copy>(key: &str) -> Field<S, T> {
    Field::at(value(key))
}

pub mod format_context {
    use super::*;
    pub const NB_STREAMS: Field<AVFormatContext, core::ffi::c_uint> =
        field("AVFormatContext.nb_streams");
    pub const STREAMS: Field<AVFormatContext, *mut *mut AVStream> =
        field("AVFormatContext.streams");
    pub const DURATION: Field<AVFormatContext, i64> = field("AVFormatContext.duration");
    pub const PB: Field<AVFormatContext, *mut AVIOContext> = field("AVFormatContext.pb");
}

pub mod stream {
    use super::*;
    pub const CODECPAR: Field<AVStream, *mut AVCodecParameters> = field("AVStream.codecpar");
    pub const TIME_BASE: Field<AVStream, AVRational> = field("AVStream.time_base");
    pub const DURATION: Field<AVStream, i64> = field("AVStream.duration");
    pub const AVG_FRAME_RATE: Field<AVStream, AVRational> = field("AVStream.avg_frame_rate");
    pub const R_FRAME_RATE: Field<AVStream, AVRational> = field("AVStream.r_frame_rate");
}

pub mod codec_parameters {
    use super::*;
    pub const CODEC_ID: Field<AVCodecParameters, core::ffi::c_int> =
        field("AVCodecParameters.codec_id");
    pub const WIDTH: Field<AVCodecParameters, core::ffi::c_int> = field("AVCodecParameters.width");
    pub const HEIGHT: Field<AVCodecParameters, core::ffi::c_int> =
        field("AVCodecParameters.height");
}

pub mod codec_context {
    use super::*;
    pub const CH_LAYOUT: Field<AVCodecContext, u8> = field("AVCodecContext.ch_layout");
    pub const SAMPLE_RATE: Field<AVCodecContext, core::ffi::c_int> =
        field("AVCodecContext.sample_rate");
    pub const SAMPLE_FMT: Field<AVCodecContext, core::ffi::c_int> =
        field("AVCodecContext.sample_fmt");
}

pub mod io_context {
    use super::*;
    pub const BUFFER: Field<AVIOContext, *mut u8> = field("AVIOContext.buffer");
    pub const EOF_REACHED: Field<AVIOContext, core::ffi::c_int> = field("AVIOContext.eof_reached");
    pub const ERROR: Field<AVIOContext, core::ffi::c_int> = field("AVIOContext.error");
}

pub mod packet {
    use super::*;
    pub const STREAM_INDEX: Field<AVPacket, core::ffi::c_int> = field("AVPacket.stream_index");
    pub const PTS: Field<AVPacket, i64> = field("AVPacket.pts");
    pub const DURATION: Field<AVPacket, i64> = field("AVPacket.duration");
}

pub mod frame {
    use super::*;
    pub const DATA: Field<AVFrame, [*mut u8; AV_NUM_DATA_POINTERS]> = field("AVFrame.data");
    pub const LINESIZE: Field<AVFrame, [core::ffi::c_int; AV_NUM_DATA_POINTERS]> =
        field("AVFrame.linesize");
    pub const WIDTH: Field<AVFrame, core::ffi::c_int> = field("AVFrame.width");
    pub const HEIGHT: Field<AVFrame, core::ffi::c_int> = field("AVFrame.height");
    pub const FORMAT: Field<AVFrame, core::ffi::c_int> = field("AVFrame.format");
    pub const EXTENDED_DATA: Field<AVFrame, *mut *mut u8> = field("AVFrame.extended_data");
    pub const PTS: Field<AVFrame, i64> = field("AVFrame.pts");
    pub const NB_SAMPLES: Field<AVFrame, core::ffi::c_int> = field("AVFrame.nb_samples");
    pub const CH_LAYOUT: Field<AVFrame, u8> = field("AVFrame.ch_layout");
    pub const BEST_EFFORT_TIMESTAMP: Field<AVFrame, i64> = field("AVFrame.best_effort_timestamp");
}

pub mod channel_layout {
    use super::*;
    pub const NB_CHANNELS: Field<AVChannelLayout, core::ffi::c_int> =
        field("AVChannelLayout.nb_channels");
}
