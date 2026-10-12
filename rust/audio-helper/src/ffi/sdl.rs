//! Southstar — the SDL2 audio device, audio streams and tick counter the helper uses, declared by hand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

type AudioCallback = unsafe extern "C" fn(*mut c_void, *mut u8, c_int);

#[repr(C)]
struct AudioSpec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<AudioCallback>,
    userdata: *mut c_void,
}

#[repr(C)]
struct SdlAudioStream {
    _private: [u8; 0],
}

const SDL_INIT_AUDIO: u32 = 0x10;
#[cfg(target_endian = "little")]
const AUDIO_F32SYS: u16 = 0x8120;
#[cfg(target_endian = "big")]
const AUDIO_F32SYS: u16 = 0x9120;
const FRAME_BYTES: c_int = 8;

unsafe extern "C" {
    fn SDL_SetMainReady();
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_Quit();
    fn SDL_GetTicks() -> u32;
    fn SDL_OpenAudioDevice(
        device: *const c_char,
        iscapture: c_int,
        desired: *const AudioSpec,
        obtained: *mut AudioSpec,
        allowed_changes: c_int,
    ) -> u32;
    fn SDL_PauseAudioDevice(dev: u32, pause_on: c_int);
    fn SDL_CloseAudioDevice(dev: u32);
    fn SDL_NewAudioStream(
        src_format: u16,
        src_channels: u8,
        src_rate: c_int,
        dst_format: u16,
        dst_channels: u8,
        dst_rate: c_int,
    ) -> *mut SdlAudioStream;
    fn SDL_AudioStreamPut(stream: *mut SdlAudioStream, buf: *const c_void, len: c_int) -> c_int;
    fn SDL_AudioStreamGet(stream: *mut SdlAudioStream, buf: *mut c_void, len: c_int) -> c_int;
    fn SDL_AudioStreamAvailable(stream: *mut SdlAudioStream) -> c_int;
    fn SDL_AudioStreamFlush(stream: *mut SdlAudioStream) -> c_int;
    fn SDL_FreeAudioStream(stream: *mut SdlAudioStream);
}

pub fn init_audio() -> bool {
    unsafe { SDL_SetMainReady() };
    unsafe { SDL_Init(SDL_INIT_AUDIO) == 0 }
}

pub fn quit() {
    unsafe { SDL_Quit() };
}

pub fn ticks() -> u32 {
    unsafe { SDL_GetTicks() }
}

unsafe extern "C" fn mix(_user: *mut c_void, stream: *mut u8, len: c_int) {
    let len = len.max(0) as usize;
    unsafe { ptr::write_bytes(stream, 0, len) };
    let out = unsafe { core::slice::from_raw_parts_mut(stream.cast::<f32>(), len / 4) };
    crate::mixer::lock().mix(out);
}

pub struct Device(u32);

impl Device {
    pub fn open(rate: c_int) -> Option<Device> {
        let want = AudioSpec {
            freq: rate,
            format: AUDIO_F32SYS,
            channels: 2,
            silence: 0,
            samples: 1024,
            padding: 0,
            size: 0,
            callback: Some(mix),
            userdata: ptr::null_mut(),
        };
        let mut have = AudioSpec { ..want };
        let id = unsafe { SDL_OpenAudioDevice(ptr::null(), 0, &want, &mut have, 0) };
        if id == 0 {
            return None;
        }
        unsafe { SDL_PauseAudioDevice(id, 0) };
        Some(Device(id))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { SDL_CloseAudioDevice(self.0) };
    }
}

pub struct AudioStream(*mut SdlAudioStream);

impl AudioStream {
    pub fn to_stereo(channels: c_int, rate: c_int, device_rate: c_int) -> Option<AudioStream> {
        let stream = unsafe {
            SDL_NewAudioStream(
                AUDIO_F32SYS,
                channels as u8,
                rate,
                AUDIO_F32SYS,
                2,
                device_rate,
            )
        };
        (!stream.is_null()).then_some(AudioStream(stream))
    }

    pub fn put(&mut self, samples: &[f32]) -> bool {
        let Ok(bytes) = c_int::try_from(core::mem::size_of_val(samples)) else {
            return false;
        };
        unsafe { SDL_AudioStreamPut(self.0, samples.as_ptr().cast(), bytes) >= 0 }
    }

    pub fn flush(&mut self) {
        unsafe { SDL_AudioStreamFlush(self.0) };
    }

    pub fn available(&self) -> c_int {
        unsafe { SDL_AudioStreamAvailable(self.0) }
    }

    #[cfg(feature = "libav")]
    pub fn get_frames(&mut self, out: &mut [f32]) -> usize {
        let room = c_int::try_from(core::mem::size_of_val(out)).unwrap_or(c_int::MAX);
        let mut want = self.available().min(room);
        want -= want % FRAME_BYTES;
        if want < FRAME_BYTES {
            return 0;
        }
        let got = unsafe { SDL_AudioStreamGet(self.0, out.as_mut_ptr().cast(), want) };
        (got.max(0) / FRAME_BYTES) as usize
    }

    pub fn get_all(&mut self) -> Option<Vec<f32>> {
        let avail = self.available();
        if avail <= 0 {
            return None;
        }
        let mut buf = vec![0f32; (avail as usize).div_ceil(4)];
        let got = unsafe { SDL_AudioStreamGet(self.0, buf.as_mut_ptr().cast(), avail) };
        if got <= 0 {
            return None;
        }
        buf.truncate((got / FRAME_BYTES) as usize * 2);
        Some(buf)
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        unsafe { SDL_FreeAudioStream(self.0) };
    }
}
