//! Southstar — the shared-memory frame ring a player publishes BGRA frames and its clock into, laid out as rust/media-shm declares.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use southstar_media_shm::{
    MEDIA_CLOCK_PLAYING, MEDIA_CLOCK_USED, VIDEO_RING_MAGIC, VIDEO_RING_SLOTS, VIDEO_RING_VERSION,
    VideoRingHdr, VideoRingSlot,
};

use super::decoder::Decoder;
use super::os::{self, SharedMemory};

const HEADER_BYTES: usize = core::mem::size_of::<VideoRingHdr>();

static NEXT_NAME: AtomicU32 = AtomicU32::new(0);

pub struct Ring {
    memory: SharedMemory,
}

#[derive(Clone, Copy)]
pub struct RingView(*mut VideoRingHdr);

unsafe impl Send for RingView {}
unsafe impl Sync for RingView {}

impl Ring {
    pub fn create(width: u32, height: u32) -> Option<Ring> {
        let stride = (width * 4 + 63) & !63;
        let frame_bytes = stride * height;
        let total = HEADER_BYTES + frame_bytes as usize * VIDEO_RING_SLOTS as usize;
        let name = format!(
            "/nsvid-{}-{}",
            std::process::id(),
            NEXT_NAME.fetch_add(1, Ordering::Relaxed) + 1
        );
        let memory = SharedMemory::create(&name, total)?;
        let hdr = memory.base().cast::<VideoRingHdr>();
        unsafe {
            ptr::write_bytes(hdr.cast::<u8>(), 0, HEADER_BYTES);
            (&raw mut (*hdr).magic).write(VIDEO_RING_MAGIC);
            (&raw mut (*hdr).version).write(VIDEO_RING_VERSION);
            (&raw mut (*hdr).width).write(width);
            (&raw mut (*hdr).height).write(height);
            (&raw mut (*hdr).stride).write(stride);
            (&raw mut (*hdr).nslots).write(VIDEO_RING_SLOTS);
            (&raw mut (*hdr).frame_bytes).write(frame_bytes);
            (*hdr).generation.store(1, Ordering::Release);
        }
        Some(Ring { memory })
    }

    pub fn view(&self) -> RingView {
        RingView(self.memory.base().cast())
    }

    pub fn name(&self) -> &str {
        self.memory.name()
    }
}

impl RingView {
    fn published_counter(&self) -> &AtomicU32 {
        unsafe { &(*self.0).published }
    }

    fn released_counter(&self) -> &AtomicU32 {
        unsafe { &(*self.0).released }
    }

    fn generation(&self) -> &AtomicU32 {
        unsafe { &(*self.0).generation }
    }

    fn clock_sequence(&self) -> &AtomicU32 {
        unsafe { &(*self.0).clock_sequence }
    }

    fn slot(&self, index: usize) -> *mut VideoRingSlot {
        unsafe {
            (&raw mut (*self.0).slots)
                .cast::<VideoRingSlot>()
                .add(index)
        }
    }

    pub fn width(&self) -> u32 {
        unsafe { (&raw const (*self.0).width).read_volatile() }
    }

    pub fn height(&self) -> u32 {
        unsafe { (&raw const (*self.0).height).read_volatile() }
    }

    pub fn stride(&self) -> u32 {
        unsafe { (&raw const (*self.0).stride).read_volatile() }
    }

    fn nslots(&self) -> u32 {
        unsafe { (&raw const (*self.0).nslots).read_volatile() }
    }

    fn frame_bytes(&self) -> u32 {
        unsafe { (&raw const (*self.0).frame_bytes).read_volatile() }
    }

    pub fn published(&self) -> u32 {
        self.published_counter().load(Ordering::Acquire)
    }

    pub fn released(&self) -> u32 {
        self.released_counter().load(Ordering::Acquire)
    }

    pub fn store_clock(&self, position: f64, playing: bool) {
        let clock_sequence = self.clock_sequence();
        let mut sequence = clock_sequence.load(Ordering::Relaxed);
        if sequence & 1 != 0 {
            sequence = sequence.wrapping_add(1);
        }
        clock_sequence.store(sequence.wrapping_add(1), Ordering::Release);
        let flags = MEDIA_CLOCK_USED | if playing { MEDIA_CLOCK_PLAYING } else { 0 };
        unsafe {
            (&raw mut (*self.0).clock_flags).write_volatile(flags);
            (&raw mut (*self.0).clock_monotonic_us).write_volatile(os::now_us());
            (&raw mut (*self.0).clock_position).write_volatile(position);
        }
        clock_sequence.store(sequence.wrapping_add(2), Ordering::Release);
    }

    pub fn reset(&self) {
        let published = self.published();
        self.released_counter().store(published, Ordering::Release);
        self.generation().fetch_add(1, Ordering::AcqRel);
    }

    pub fn has_space(&self) -> bool {
        self.published().wrapping_sub(self.released()) < self.nslots()
    }

    pub fn publish(&self, decoder: &mut Decoder, pts: f64, duration: f64) -> bool {
        if !self.has_space() {
            return false;
        }
        let sequence = self
            .published_counter()
            .load(Ordering::Relaxed)
            .wrapping_add(1);
        let index = (sequence.wrapping_sub(1) % self.nslots()) as usize;
        if !decoder.prepare_scaler(self.width(), self.height()) {
            return false;
        }
        let meta = self.slot(index);
        let meta_sequence = unsafe { &(*meta).sequence };
        let pixels = unsafe {
            self.0
                .cast::<u8>()
                .add(HEADER_BYTES + index * self.frame_bytes() as usize)
        };
        meta_sequence.store(0, Ordering::Release);
        unsafe {
            decoder.scale_into(pixels, self.stride());
            (&raw mut (*meta).generation).write_volatile(self.generation().load(Ordering::Acquire));
            (&raw mut (*meta).pts).write_volatile(pts);
            (&raw mut (*meta).duration).write_volatile(duration);
        }
        meta_sequence.store(sequence, Ordering::Release);
        self.published_counter().store(sequence, Ordering::Release);
        true
    }
}
