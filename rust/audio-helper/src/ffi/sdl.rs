//! Southstar — the SDL3 audio device stream and audio streams the helper uses, declared by hand.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

type AudioStreamCallback = unsafe extern "C" fn(*mut c_void, *mut SdlAudioStream, c_int, c_int);

#[repr(C)]
struct AudioSpec {
    format: c_int,
    channels: c_int,
    freq: c_int,
}

#[repr(C)]
struct SdlAudioStream {
    _private: [u8; 0],
}

const SDL_INIT_AUDIO: u32 = 0x10;
#[cfg(target_endian = "little")]
const SDL_AUDIO_F32: c_int = 0x8120;
#[cfg(target_endian = "big")]
const SDL_AUDIO_F32: c_int = 0x9120;
const SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK: u32 = 0xFFFF_FFFF;
const FRAME_BYTES: c_int = 8;
const CHUNK_FRAMES: usize = 1024;

const HINTS: [(&CStr, &CStr); 3] = [
    (c"SDL_APP_NAME", c"Southstar Browser"),
    (c"SDL_APP_ID", c"org.southstar.WebBrowser"),
    (c"SDL_AUDIO_DEVICE_SAMPLE_FRAMES", c"1024"),
];

unsafe extern "C" {
    safe fn SDL_SetMainReady();
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> bool;
    safe fn SDL_Init(flags: u32) -> bool;
    fn SDL_Quit();
    fn SDL_OpenAudioDeviceStream(
        devid: u32,
        spec: *const AudioSpec,
        callback: Option<AudioStreamCallback>,
        userdata: *mut c_void,
    ) -> *mut SdlAudioStream;
    fn SDL_ResumeAudioStreamDevice(stream: *mut SdlAudioStream) -> bool;
    fn SDL_CreateAudioStream(
        src_spec: *const AudioSpec,
        dst_spec: *const AudioSpec,
    ) -> *mut SdlAudioStream;
    fn SDL_PutAudioStreamData(stream: *mut SdlAudioStream, buf: *const c_void, len: c_int) -> bool;
    fn SDL_GetAudioStreamData(stream: *mut SdlAudioStream, buf: *mut c_void, len: c_int) -> c_int;
    fn SDL_GetAudioStreamAvailable(stream: *mut SdlAudioStream) -> c_int;
    fn SDL_FlushAudioStream(stream: *mut SdlAudioStream) -> bool;
    fn SDL_DestroyAudioStream(stream: *mut SdlAudioStream);
}

pub fn init_audio() -> bool {
    SDL_SetMainReady();
    for (name, value) in HINTS {
        // SAFETY: both are NUL-terminated static strings, which SDL copies.
        unsafe { SDL_SetHint(name.as_ptr(), value.as_ptr()) };
    }
    SDL_Init(SDL_INIT_AUDIO)
}

pub fn quit() {
    // SAFETY: shuts SDL down after the device and streams were dropped.
    unsafe { SDL_Quit() };
}

unsafe extern "C" fn mix(
    _user: *mut c_void,
    stream: *mut SdlAudioStream,
    needed: c_int,
    _total: c_int,
) {
    let mut chunk = [0f32; CHUNK_FRAMES * 2];
    let mut frames = (needed.max(0) as usize).div_ceil(FRAME_BYTES as usize);
    while frames > 0 {
        let n = frames.min(CHUNK_FRAMES);
        let out = &mut chunk[..n * 2];
        out.fill(0.0);
        crate::mixer::lock().mix(out);
        // SAFETY: stream is the device stream SDL passes its own callback, and out holds n frames of floats, which SDL copies.
        unsafe { SDL_PutAudioStreamData(stream, out.as_ptr().cast(), n as c_int * FRAME_BYTES) };
        frames -= n;
    }
}

fn spec(channels: c_int, rate: c_int) -> AudioSpec {
    AudioSpec {
        format: SDL_AUDIO_F32,
        channels,
        freq: rate,
    }
}

pub struct Device(*mut SdlAudioStream);

impl Device {
    pub fn open(rate: c_int) -> Option<Device> {
        let want = spec(2, rate);
        // SAFETY: want is a live AudioSpec; the callback is a static function that only touches the mixer behind its lock.
        let stream = unsafe {
            SDL_OpenAudioDeviceStream(
                SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
                &want,
                Some(mix),
                ptr::null_mut(),
            )
        };
        if stream.is_null() {
            return None;
        }
        let device = Device(stream);
        // SAFETY: device.0 is the stream SDL_OpenAudioDeviceStream just opened; its device starts paused.
        unsafe { SDL_ResumeAudioStreamDevice(device.0) };
        Some(device)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: self.0 is an open device stream, destroyed exactly once, which also closes its device.
        unsafe { SDL_DestroyAudioStream(self.0) };
    }
}

pub struct AudioStream(*mut SdlAudioStream);

impl AudioStream {
    pub fn to_stereo(channels: c_int, rate: c_int, device_rate: c_int) -> Option<AudioStream> {
        let src = spec(channels, rate);
        let dst = spec(2, device_rate);
        // SAFETY: src and dst are live AudioSpecs, which SDL copies; the result is null or a new stream this value owns.
        let stream = unsafe { SDL_CreateAudioStream(&src, &dst) };
        (!stream.is_null()).then(|| AudioStream(stream))
    }

    pub fn put(&mut self, samples: &[f32]) -> bool {
        let Ok(bytes) = c_int::try_from(core::mem::size_of_val(samples)) else {
            return false;
        };
        // SAFETY: self.0 is a live stream and samples holds bytes readable bytes, which SDL copies.
        unsafe { SDL_PutAudioStreamData(self.0, samples.as_ptr().cast(), bytes) }
    }

    pub fn flush(&mut self) {
        // SAFETY: self.0 is a live stream.
        unsafe { SDL_FlushAudioStream(self.0) };
    }

    pub fn available(&self) -> c_int {
        // SAFETY: self.0 is a live stream.
        unsafe { SDL_GetAudioStreamAvailable(self.0) }
    }

    #[cfg(feature = "libav")]
    pub fn get_frames(&mut self, out: &mut [f32]) -> usize {
        let room = c_int::try_from(core::mem::size_of_val(out)).unwrap_or(c_int::MAX);
        let mut want = self.available().min(room);
        want -= want % FRAME_BYTES;
        if want < FRAME_BYTES {
            return 0;
        }
        // SAFETY: self.0 is a live stream and out has room for want bytes, want being at most its size.
        let got = unsafe { SDL_GetAudioStreamData(self.0, out.as_mut_ptr().cast(), want) };
        (got.max(0) / FRAME_BYTES) as usize
    }

    pub fn get_all(&mut self) -> Option<Vec<f32>> {
        let avail = self.available();
        if avail <= 0 {
            return None;
        }
        let mut buf = vec![0f32; (avail as usize).div_ceil(4)];
        // SAFETY: self.0 is a live stream and buf has room for avail bytes.
        let got = unsafe { SDL_GetAudioStreamData(self.0, buf.as_mut_ptr().cast(), avail) };
        if got <= 0 {
            return None;
        }
        buf.truncate((got / FRAME_BYTES) as usize * 2);
        Some(buf)
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        // SAFETY: self.0 is the stream SDL_CreateAudioStream returned, destroyed exactly once.
        unsafe { SDL_DestroyAudioStream(self.0) };
    }
}
