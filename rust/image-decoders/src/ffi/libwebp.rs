//! Southstar — libwebp's decoder and animation-decoder entry points, at the ABI versions their headers check by major number.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

const DECODER_ABI_VERSION: c_int = 0x0209;
const DEMUX_ABI_VERSION: c_int = 0x0107;
const MODE_BGRA_PREMULTIPLIED: c_int = 8;
const VP8_STATUS_OK: c_int = 0;

#[repr(C)]
struct BitstreamFeatures {
    width: c_int,
    height: c_int,
    has_alpha: c_int,
    has_animation: c_int,
    format: c_int,
    pad: [u32; 5],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RgbaBuffer {
    rgba: *mut u8,
    stride: c_int,
    size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct YuvaBuffer {
    planes: [*mut u8; 4],
    strides: [c_int; 4],
    sizes: [usize; 4],
}

#[repr(C)]
union BufferUnion {
    rgba: RgbaBuffer,
    yuva: YuvaBuffer,
}

#[repr(C)]
struct DecBuffer {
    colorspace: c_int,
    width: c_int,
    height: c_int,
    is_external_memory: c_int,
    u: BufferUnion,
    pad: [u32; 4],
    private_memory: *mut u8,
}

#[repr(C)]
struct DecoderOptions {
    fields: [c_int; 14],
    pad: [u32; 5],
}

#[repr(C)]
struct DecoderConfig {
    input: BitstreamFeatures,
    output: DecBuffer,
    options: DecoderOptions,
}

#[repr(C)]
struct AnimDecoderOptions {
    color_mode: c_int,
    use_threads: c_int,
    padding: [u32; 7],
}

#[repr(C)]
#[derive(Default)]
pub struct AnimInfo {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub loop_count: u32,
    pub bgcolor: u32,
    pub frame_count: u32,
    pad: [u32; 4],
}

#[repr(C)]
struct WebPData {
    bytes: *const u8,
    size: usize,
}

#[repr(C)]
struct WebPAnimDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn WebPGetInfo(data: *const u8, size: usize, width: *mut c_int, height: *mut c_int) -> c_int;
    fn WebPInitDecoderConfigInternal(config: *mut DecoderConfig, version: c_int) -> c_int;
    fn WebPDecode(data: *const u8, size: usize, config: *mut DecoderConfig) -> c_int;
    fn WebPAnimDecoderOptionsInitInternal(
        options: *mut AnimDecoderOptions,
        version: c_int,
    ) -> c_int;
    fn WebPAnimDecoderNewInternal(
        data: *const WebPData,
        options: *const AnimDecoderOptions,
        version: c_int,
    ) -> *mut WebPAnimDecoder;
    fn WebPAnimDecoderGetInfo(decoder: *const WebPAnimDecoder, info: *mut AnimInfo) -> c_int;
    fn WebPAnimDecoderGetNext(
        decoder: *mut WebPAnimDecoder,
        buf: *mut *mut u8,
        timestamp: *mut c_int,
    ) -> c_int;
    fn WebPAnimDecoderHasMoreFrames(decoder: *const WebPAnimDecoder) -> c_int;
    fn WebPAnimDecoderDelete(decoder: *mut WebPAnimDecoder);
}

pub fn info(data: &[u8]) -> Option<(i32, i32)> {
    let (mut width, mut height) = (0, 0);
    (unsafe { WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) } != 0)
        .then_some((width, height))
}

pub fn decode_premultiplied_bgra(data: &[u8], out: &mut [u8], stride: usize) -> bool {
    let Ok(stride) = c_int::try_from(stride) else {
        return false;
    };
    let mut config = core::mem::MaybeUninit::<DecoderConfig>::zeroed();
    if unsafe { WebPInitDecoderConfigInternal(config.as_mut_ptr(), DECODER_ABI_VERSION) } == 0 {
        return false;
    }
    let mut config = unsafe { config.assume_init() };
    config.output.colorspace = MODE_BGRA_PREMULTIPLIED;
    config.output.is_external_memory = 1;
    config.output.u.rgba = RgbaBuffer {
        rgba: out.as_mut_ptr(),
        stride,
        size: out.len(),
    };
    unsafe { WebPDecode(data.as_ptr(), data.len(), &mut config) == VP8_STATUS_OK }
}

pub struct AnimDecoder<'a> {
    decoder: NonNull<WebPAnimDecoder>,
    _data: PhantomData<&'a [u8]>,
}

impl Drop for AnimDecoder<'_> {
    fn drop(&mut self) {
        unsafe { WebPAnimDecoderDelete(self.decoder.as_ptr()) };
    }
}

impl<'a> AnimDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Option<AnimDecoder<'a>> {
        let mut options = AnimDecoderOptions {
            color_mode: 0,
            use_threads: 0,
            padding: [0; 7],
        };
        if unsafe { WebPAnimDecoderOptionsInitInternal(&mut options, DEMUX_ABI_VERSION) } == 0 {
            return None;
        }
        options.color_mode = MODE_BGRA_PREMULTIPLIED;
        let webp = WebPData {
            bytes: data.as_ptr(),
            size: data.len(),
        };
        let decoder = unsafe { WebPAnimDecoderNewInternal(&webp, &options, DEMUX_ABI_VERSION) };
        NonNull::new(decoder).map(|decoder| AnimDecoder {
            decoder,
            _data: PhantomData,
        })
    }

    pub fn info(&self) -> Option<AnimInfo> {
        let mut info = AnimInfo::default();
        (unsafe { WebPAnimDecoderGetInfo(self.decoder.as_ptr(), &mut info) } != 0).then_some(info)
    }

    pub fn has_more_frames(&self) -> bool {
        unsafe { WebPAnimDecoderHasMoreFrames(self.decoder.as_ptr()) != 0 }
    }

    pub fn next_frame(&mut self, canvas_width: u32, canvas_height: u32) -> Option<(&[u8], i32)> {
        let mut buf = ptr::null_mut();
        let mut timestamp = 0;
        if unsafe { WebPAnimDecoderGetNext(self.decoder.as_ptr(), &mut buf, &mut timestamp) } == 0
            || buf.is_null()
        {
            return None;
        }
        let len = canvas_width as usize * 4 * canvas_height as usize;
        Some((unsafe { core::slice::from_raw_parts(buf, len) }, timestamp))
    }
}
