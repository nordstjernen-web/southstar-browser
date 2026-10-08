//! Southstar — the C ABI of microphone capture, as declared in src/mic.h, over SDL2's audio capture device.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

#[cfg(feature = "sdl")]
mod sdl {
    use core::ffi::{c_char, c_int, c_void};
    use core::ptr;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

    use crate::Ring;

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

    const SDL_INIT_AUDIO: u32 = 0x10;
    #[cfg(target_endian = "little")]
    const AUDIO_F32SYS: u16 = 0x8120;
    #[cfg(target_endian = "big")]
    const AUDIO_F32SYS: u16 = 0x9120;
    const ALLOW_FREQUENCY_CHANGE: c_int = 0x1;
    const ALLOW_CHANNELS_CHANGE: c_int = 0x4;

    unsafe extern "C" {
        fn SDL_WasInit(flags: u32) -> u32;
        fn SDL_InitSubSystem(flags: u32) -> c_int;
        fn SDL_OpenAudioDevice(
            device: *const c_char,
            iscapture: c_int,
            desired: *const AudioSpec,
            obtained: *mut AudioSpec,
            allowed_changes: c_int,
        ) -> u32;
        fn SDL_PauseAudioDevice(dev: u32, pause_on: c_int);
        fn SDL_CloseAudioDevice(dev: u32);
    }

    static DEVICE: AtomicU32 = AtomicU32::new(0);
    static CHANNELS: AtomicI32 = AtomicI32::new(1);
    static REFS: Mutex<i32> = Mutex::new(0);
    static RING: Mutex<Option<Ring>> = Mutex::new(None);

    fn ring<T>(f: impl FnOnce(&mut Ring) -> T) -> T {
        let mut guard = RING.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(Ring::default))
    }

    unsafe extern "C" fn capture(_user: *mut c_void, stream: *mut u8, len: c_int) {
        let stream = unsafe { core::slice::from_raw_parts(stream, len.max(0) as usize) };
        let channels = CHANNELS.load(Ordering::SeqCst);
        ring(|r| r.push_interleaved(stream, channels));
    }

    pub(super) fn acquire() -> bool {
        let mut refs = REFS.lock().unwrap_or_else(|e| e.into_inner());
        if DEVICE.load(Ordering::SeqCst) != 0 {
            *refs += 1;
            return true;
        }
        if unsafe { SDL_WasInit(SDL_INIT_AUDIO) } == 0
            && unsafe { SDL_InitSubSystem(SDL_INIT_AUDIO) } != 0
        {
            return false;
        }
        let want = AudioSpec {
            freq: 48000,
            format: AUDIO_F32SYS,
            channels: 1,
            silence: 0,
            samples: 1024,
            padding: 0,
            size: 0,
            callback: Some(capture),
            userdata: ptr::null_mut(),
        };
        let mut got = AudioSpec {
            freq: 0,
            format: 0,
            channels: 0,
            silence: 0,
            samples: 0,
            padding: 0,
            size: 0,
            callback: None,
            userdata: ptr::null_mut(),
        };
        ring(Ring::clear);
        let device = unsafe {
            SDL_OpenAudioDevice(
                ptr::null(),
                1,
                &want,
                &mut got,
                ALLOW_FREQUENCY_CHANGE | ALLOW_CHANNELS_CHANGE,
            )
        };
        if device == 0 {
            return false;
        }
        CHANNELS.store(c_int::from(got.channels), Ordering::SeqCst);
        DEVICE.store(device, Ordering::SeqCst);
        unsafe { SDL_PauseAudioDevice(device, 0) };
        *refs += 1;
        true
    }

    pub(super) fn release() {
        let mut refs = REFS.lock().unwrap_or_else(|e| e.into_inner());
        if *refs > 0 {
            *refs -= 1;
            let device = DEVICE.load(Ordering::SeqCst);
            if *refs == 0 && device != 0 {
                unsafe { SDL_CloseAudioDevice(device) };
                DEVICE.store(0, Ordering::SeqCst);
            }
        }
    }

    pub(super) fn active() -> bool {
        DEVICE.load(Ordering::SeqCst) != 0
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

#[unsafe(no_mangle)]
pub extern "C" fn ns_mic_acquire() -> c_int {
    c_int::from(sdl::acquire())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_mic_release() {
    sdl::release();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_mic_active() -> c_int {
    c_int::from(sdl::active())
}

unsafe fn out<'a>(out: *mut u8, n: c_int) -> Option<&'a mut [u8]> {
    (!out.is_null() && n > 0).then(|| unsafe { core::slice::from_raw_parts_mut(out, n as usize) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mic_fill_time_domain(dst: *mut u8, n: c_int) {
    let Some(dst) = (unsafe { out(dst, n) }) else {
        return;
    };
    if sdl::active() {
        sdl::time_domain(dst);
    } else {
        dst.fill(128);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mic_fill_frequency(dst: *mut u8, n: c_int) {
    let Some(dst) = (unsafe { out(dst, n) }) else {
        return;
    };
    if sdl::active() {
        sdl::frequency(dst);
    } else {
        dst.fill(0);
    }
}
