//! Southstar — the audio-clock and video-frame-ring layouts the media helpers share with the shell.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use core::sync::atomic::AtomicU32;

pub const AUDIO_CLOCK_MAGIC: u32 = 0x4e53_4143;
pub const AUDIO_CLOCK_VERSION: u32 = 1;
pub const AUDIO_CLOCK_SLOTS: u32 = 16;
pub const MEDIA_CLOCK_USED: u32 = 1;
pub const MEDIA_CLOCK_PLAYING: u32 = 2;

pub const VIDEO_RING_MAGIC: u32 = 0x4e53_5647;
pub const VIDEO_RING_VERSION: u32 = 2;
pub const VIDEO_RING_SLOTS: u32 = 8;

#[repr(C)]
pub struct AudioClockSlot {
    pub sequence: AtomicU32,
    pub flags: u32,
    pub monotonic_us: i64,
    pub position: f64,
    pub token: [u8; 64],
}

#[repr(C)]
pub struct AudioClockHdr {
    pub magic: u32,
    pub version: u32,
    pub nslots: u32,
    pub reserved: u32,
    pub slots: [AudioClockSlot; AUDIO_CLOCK_SLOTS as usize],
}

#[repr(C)]
pub struct VideoRingSlot {
    pub sequence: AtomicU32,
    pub generation: u32,
    pub pts: f64,
    pub duration: f64,
}

#[repr(C)]
pub struct VideoRingHdr {
    pub magic: u32,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub nslots: u32,
    pub frame_bytes: u32,
    pub published: AtomicU32,
    pub released: AtomicU32,
    pub generation: AtomicU32,
    pub clock_sequence: AtomicU32,
    pub clock_flags: u32,
    pub clock_monotonic_us: i64,
    pub clock_position: f64,
    pub slots: [VideoRingSlot; VIDEO_RING_SLOTS as usize],
}

const _: () = assert!(core::mem::size_of::<AudioClockHdr>() == 1424);
const _: () = assert!(core::mem::size_of::<VideoRingHdr>() == 256);
