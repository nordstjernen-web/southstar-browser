//! Southstar — the libavformat, libavcodec, libavutil, libswscale and libswresample entry points the media code calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};

macro_rules! opaque {
    ($($name:ident),*) => {
        $(
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    AVFormatContext,
    AVInputFormat,
    AVStream,
    AVCodecParameters,
    AVCodecContext,
    AVCodec,
    AVIOContext,
    AVPacket,
    AVFrame,
    AVDictionary,
    AVChannelLayout,
    SwsContext,
    SwrContext
);

pub type AvioRead = unsafe extern "C" fn(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int;
pub type AvioWrite =
    unsafe extern "C" fn(opaque: *mut c_void, buf: *const u8, size: c_int) -> c_int;
pub type AvioSeek = unsafe extern "C" fn(opaque: *mut c_void, offset: i64, whence: c_int) -> i64;

unsafe extern "C" {
    pub fn avformat_alloc_context() -> *mut AVFormatContext;
    pub fn avformat_open_input(
        ps: *mut *mut AVFormatContext,
        url: *const c_char,
        fmt: *const AVInputFormat,
        options: *mut *mut AVDictionary,
    ) -> c_int;
    pub fn avformat_find_stream_info(
        ic: *mut AVFormatContext,
        options: *mut *mut AVDictionary,
    ) -> c_int;
    pub fn avformat_close_input(s: *mut *mut AVFormatContext);
    pub fn av_find_best_stream(
        ic: *mut AVFormatContext,
        media_type: c_int,
        wanted_stream_nb: c_int,
        related_stream: c_int,
        decoder_ret: *mut *const AVCodec,
        flags: c_int,
    ) -> c_int;
    pub fn av_read_frame(s: *mut AVFormatContext, pkt: *mut AVPacket) -> c_int;
    pub fn av_seek_frame(
        s: *mut AVFormatContext,
        stream_index: c_int,
        timestamp: i64,
        flags: c_int,
    ) -> c_int;
    pub fn avio_alloc_context(
        buffer: *mut u8,
        buffer_size: c_int,
        write_flag: c_int,
        opaque: *mut c_void,
        read_packet: Option<AvioRead>,
        write_packet: Option<AvioWrite>,
        seek: Option<AvioSeek>,
    ) -> *mut AVIOContext;
    pub fn avio_context_free(s: *mut *mut AVIOContext);

    pub fn avcodec_find_decoder(id: c_int) -> *const AVCodec;
    pub fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    pub fn avcodec_free_context(ctx: *mut *mut AVCodecContext);
    pub fn avcodec_parameters_to_context(
        codec: *mut AVCodecContext,
        par: *const AVCodecParameters,
    ) -> c_int;
    pub fn avcodec_open2(
        ctx: *mut AVCodecContext,
        codec: *const AVCodec,
        options: *mut *mut AVDictionary,
    ) -> c_int;
    pub fn avcodec_send_packet(ctx: *mut AVCodecContext, pkt: *const AVPacket) -> c_int;
    pub fn avcodec_receive_frame(ctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
    pub fn avcodec_flush_buffers(ctx: *mut AVCodecContext);
    pub fn av_packet_alloc() -> *mut AVPacket;
    pub fn av_packet_free(pkt: *mut *mut AVPacket);
    pub fn av_packet_unref(pkt: *mut AVPacket);

    pub fn av_frame_alloc() -> *mut AVFrame;
    pub fn av_frame_free(frame: *mut *mut AVFrame);
    pub fn av_frame_unref(frame: *mut AVFrame);
    pub fn av_malloc(size: usize) -> *mut c_void;
    pub fn av_free(ptr: *mut c_void);
    pub fn av_freep(ptr: *mut c_void);
    pub fn av_log_set_level(level: c_int);
    pub fn av_dict_set(
        pm: *mut *mut AVDictionary,
        key: *const c_char,
        value: *const c_char,
        flags: c_int,
    ) -> c_int;
    pub fn av_dict_free(m: *mut *mut AVDictionary);
    pub fn av_opt_set_int(
        obj: *mut c_void,
        name: *const c_char,
        val: i64,
        search_flags: c_int,
    ) -> c_int;
    pub fn av_opt_set_sample_fmt(
        obj: *mut c_void,
        name: *const c_char,
        fmt: c_int,
        search_flags: c_int,
    ) -> c_int;
    pub fn av_opt_set_chlayout(
        obj: *mut c_void,
        name: *const c_char,
        layout: *const AVChannelLayout,
        search_flags: c_int,
    ) -> c_int;

    pub fn sws_getCachedContext(
        context: *mut SwsContext,
        src_w: c_int,
        src_h: c_int,
        src_format: c_int,
        dst_w: c_int,
        dst_h: c_int,
        dst_format: c_int,
        flags: c_int,
        src_filter: *mut c_void,
        dst_filter: *mut c_void,
        param: *const f64,
    ) -> *mut SwsContext;
    pub fn sws_scale(
        c: *mut SwsContext,
        src_slice: *const *const u8,
        src_stride: *const c_int,
        src_slice_y: c_int,
        src_slice_h: c_int,
        dst: *const *mut u8,
        dst_stride: *const c_int,
    ) -> c_int;
    pub fn sws_freeContext(c: *mut SwsContext);

    pub fn swr_alloc() -> *mut SwrContext;
    pub fn swr_init(s: *mut SwrContext) -> c_int;
    pub fn swr_free(s: *mut *mut SwrContext);
    pub fn swr_convert(
        s: *mut SwrContext,
        out: *const *mut u8,
        out_count: c_int,
        input: *const *const u8,
        in_count: c_int,
    ) -> c_int;
    pub fn swr_get_out_samples(s: *mut SwrContext, in_samples: c_int) -> c_int;
}
