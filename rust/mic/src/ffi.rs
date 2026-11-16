//! Southstar — microphone capture over an SDL3 recording stream: acquiring, releasing and reading the captured signal.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "sdl")]
mod sdl {
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicPtr, Ordering};

    use crate::Ring;

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
    const SDL_AUDIO_DEVICE_DEFAULT_RECORDING: u32 = 0xFFFF_FFFE;

    unsafe extern "C" {
        safe fn SDL_InitSubSystem(flags: u32) -> bool;
        fn SDL_OpenAudioDeviceStream(
            devid: u32,
            spec: *const AudioSpec,
            callback: Option<AudioStreamCallback>,
            userdata: *mut c_void,
        ) -> *mut SdlAudioStream;
        fn SDL_ResumeAudioStreamDevice(stream: *mut SdlAudioStream) -> bool;
        fn SDL_GetAudioStreamData(
            stream: *mut SdlAudioStream,
            buf: *mut c_void,
            len: c_int,
        ) -> c_int;
        fn SDL_DestroyAudioStream(stream: *mut SdlAudioStream);
    }

    static DEVICE: AtomicPtr<SdlAudioStream> = AtomicPtr::new(ptr::null_mut());
    static REFS: Mutex<i32> = Mutex::new(0);
    static RING: Mutex<Option<Ring>> = Mutex::new(None);

    fn ring<T>(f: impl FnOnce(&mut Ring) -> T) -> T {
        let mut guard = RING.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(Ring::default))
    }

    unsafe extern "C" fn capture(
        _user: *mut c_void,
        stream: *mut SdlAudioStream,
        _added: c_int,
        _total: c_int,
    ) {
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: stream is the recording stream SDL passes its own callback, and buf has room for its length in bytes.
            let got = unsafe {
                SDL_GetAudioStreamData(stream, buf.as_mut_ptr().cast(), buf.len() as c_int)
            };
            if got <= 0 {
                break;
            }
            ring(|r| r.push_interleaved(&buf[..got as usize], 1));
        }
    }

    pub(super) fn acquire() -> bool {
        let mut refs = REFS.lock().unwrap_or_else(|e| e.into_inner());
        if !DEVICE.load(Ordering::SeqCst).is_null() {
            *refs += 1;
            return true;
        }
        if !SDL_InitSubSystem(SDL_INIT_AUDIO) {
            return false;
        }
        let want = AudioSpec {
            format: SDL_AUDIO_F32,
            channels: 1,
            freq: 48000,
        };
        ring(Ring::clear);
        // SAFETY: want is a live AudioSpec, which SDL copies, and the callback matches SDL's signature and only
        // touches the ring behind its lock.
        let device = unsafe {
            SDL_OpenAudioDeviceStream(
                SDL_AUDIO_DEVICE_DEFAULT_RECORDING,
                &want,
                Some(capture),
                ptr::null_mut(),
            )
        };
        if device.is_null() {
            return false;
        }
        DEVICE.store(device, Ordering::SeqCst);
        // SAFETY: device is the recording stream SDL_OpenAudioDeviceStream just opened; its device starts paused.
        unsafe { SDL_ResumeAudioStreamDevice(device) };
        *refs += 1;
        true
    }

    pub(super) fn release() {
        let mut refs = REFS.lock().unwrap_or_else(|e| e.into_inner());
        if *refs > 0 {
            *refs -= 1;
            let device = DEVICE.load(Ordering::SeqCst);
            if *refs == 0 && !device.is_null() {
                // SAFETY: device is the recording stream this module opened, destroyed once here before DEVICE is
                // cleared, so no later call reuses the stale pointer.
                unsafe { SDL_DestroyAudioStream(device) };
                DEVICE.store(ptr::null_mut(), Ordering::SeqCst);
            }
        }
    }

    pub(super) fn active() -> bool {
        !DEVICE.load(Ordering::SeqCst).is_null()
    }

    pub(super) fn time_domain(out: &mut [u8]) {
        ring(|r| r.time_domain(out));
    }

    pub(super) fn frequency(out: &mut [u8]) {
        let snap = ring(|r| r.window(out.len()));
        crate::frequency(&snap, out);
    }
}

#[cfg(not(feature = "sdl"))]
mod sdl {
    pub(super) fn acquire() -> bool {
        false
    }

    pub(super) fn release() {}

    pub(super) fn active() -> bool {
        false
    }

    pub(super) fn time_domain(_out: &mut [u8]) {}

    pub(super) fn frequency(_out: &mut [u8]) {}
}

pub fn acquire() -> bool {
    sdl::acquire()
}

pub fn release() {
    sdl::release();
}

const MAX_TIME_DOMAIN: usize = 32768;
const MAX_FREQUENCY_BINS: usize = 16384;

pub fn fill_time_domain(dst: &mut [u8]) {
    let (head, rest) = dst.split_at_mut(dst.len().min(MAX_TIME_DOMAIN));
    rest.fill(128);
    if sdl::active() {
        sdl::time_domain(head);
    } else {
        head.fill(128);
    }
}

pub fn fill_frequency(dst: &mut [u8]) {
    let (head, rest) = dst.split_at_mut(dst.len().min(MAX_FREQUENCY_BINS));
    rest.fill(0);
    if sdl::active() {
        sdl::frequency(head);
    } else {
        head.fill(0);
    }
}
