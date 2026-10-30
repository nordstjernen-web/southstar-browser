//! Southstar — the shared-memory frame ring a player publishes BGRA frames and its clock into, laid out as rust/media-shm declares.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use southstar_media_shm::{
    ClockSample, Geometry, MEDIA_CLOCK_PLAYING, MEDIA_CLOCK_USED, SharedMemory, VIDEO_RING_SLOTS,
    VideoRing, VideoRingHdr,
};

use super::decoder::Decoder;
use super::os;

const HEADER_BYTES: usize = core::mem::size_of::<VideoRingHdr>();

static NEXT_NAME: AtomicU32 = AtomicU32::new(0);

pub struct Ring {
    view: RingView,
    name: String,
}

#[derive(Clone)]
pub struct RingView(Arc<SharedMemory>);

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
        let memory = SharedMemory::create(name.as_bytes(), total)?;
        memory.video_ring()?.initialize(Geometry {
            width,
            height,
            stride,
            frame_bytes,
        });
        Some(Ring {
            view: RingView(Arc::new(memory)),
            name,
        })
    }

    pub fn view(&self) -> RingView {
        self.view.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl RingView {
    fn ring(&self) -> VideoRing<'_> {
        self.0
            .video_ring()
            .expect("a created ring holds its header")
    }

    pub fn width(&self) -> u32 {
        self.ring().geometry().width
    }

    pub fn height(&self) -> u32 {
        self.ring().geometry().height
    }

    pub fn stride(&self) -> u32 {
        self.ring().geometry().stride
    }

    pub fn published(&self) -> u32 {
        self.ring().published().load(Ordering::Acquire)
    }

    pub fn released(&self) -> u32 {
        self.ring().released().load(Ordering::Acquire)
    }

    pub fn store_clock(&self, position: f64, playing: bool) {
        let flags = MEDIA_CLOCK_USED | if playing { MEDIA_CLOCK_PLAYING } else { 0 };
        self.ring().store_clock(ClockSample {
            flags,
            monotonic_us: os::now_us(),
            position,
        });
    }

    pub fn reset(&self) {
        let ring = self.ring();
        let published = self.published();
        ring.released().store(published, Ordering::Release);
        ring.generation().fetch_add(1, Ordering::AcqRel);
    }

    fn nslots(&self) -> u32 {
        self.ring().nslots().clamp(1, VIDEO_RING_SLOTS)
    }

    pub fn has_space(&self) -> bool {
        self.published().wrapping_sub(self.released()) < self.nslots()
    }

    pub fn publish(&self, decoder: &mut Decoder, pts: f64, duration: f64) -> bool {
        if !self.has_space() {
            return false;
        }
        let ring = self.ring();
        let sequence = ring.published().load(Ordering::Relaxed).wrapping_add(1);
        let index = sequence.wrapping_sub(1) % self.nslots();
        let geometry = ring.geometry();
        if !decoder.prepare_scaler(geometry.width, geometry.height) {
            return false;
        }
        let Some(pixels) = ring.frame(index, geometry) else {
            return false;
        };
        let slot_sequence = ring.slot_sequence(index);
        slot_sequence.store(0, Ordering::Release);
        decoder.scale_into(&pixels);
        ring.write_slot(
            index,
            ring.generation().load(Ordering::Acquire),
            pts,
            duration,
        );
        slot_sequence.store(sequence, Ordering::Release);
        ring.published().store(sequence, Ordering::Release);
        true
    }
}
